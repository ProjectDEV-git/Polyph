//! Runs the phone engine (`polyph-phone`) natively over a real WebSocket to
//! the relay, doing what the PWA's JavaScript does in a browser: pass frames
//! in, send the frames that come out, tick, and collect events.

use std::sync::Arc;
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use polyph_daemon::Clock;
use polyph_phone::{Output, Phone};
use serde_json::Value;
use tokio::sync::{Mutex, broadcast, mpsc};
use tokio::task::JoinHandle;
use tokio_tungstenite::tungstenite::Message;

pub struct PhoneDriver {
    pub phone: Arc<Mutex<Phone>>,
    out: mpsc::UnboundedSender<Vec<u8>>,
    pub events: broadcast::Receiver<Value>,
    clock: Clock,
    tasks: Vec<JoinHandle<()>>,
}

impl std::fmt::Debug for PhoneDriver {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "PhoneDriver")
    }
}

impl Drop for PhoneDriver {
    fn drop(&mut self) {
        for t in &self.tasks {
            t.abort();
        }
    }
}

fn emit(o: Output, out: &mpsc::UnboundedSender<Vec<u8>>, ev: &broadcast::Sender<Value>) {
    for f in o.frames {
        let _ = out.send(f);
    }
    for e in o.events {
        let _ = ev.send(e);
    }
}

impl PhoneDriver {
    /// Connects `phone` to `url` and keeps it running until dropped.
    pub async fn connect(phone: Phone, url: &str, clock: Clock) -> anyhow::Result<Self> {
        let (ws, _) = tokio_tungstenite::connect_async(url).await?;
        let (mut sink, mut stream) = ws.split();
        let phone = Arc::new(Mutex::new(phone));
        let (out_tx, mut out_rx) = mpsc::unbounded_channel::<Vec<u8>>();
        let (ev_tx, events) = broadcast::channel(4096);
        let writer = tokio::spawn(async move {
            while let Some(f) = out_rx.recv().await {
                if sink.send(Message::Binary(f.into())).await.is_err() {
                    break;
                }
            }
        });
        emit(phone.lock().await.connected(), &out_tx, &ev_tx);
        let reader = {
            let (phone, out_tx, ev_tx, clock) = (phone.clone(), out_tx.clone(), ev_tx.clone(), clock.clone());
            tokio::spawn(async move {
                while let Some(Ok(m)) = stream.next().await {
                    if let Message::Binary(b) = m {
                        let o = phone.lock().await.frame(&b, clock.now());
                        emit(o, &out_tx, &ev_tx);
                    }
                }
                let o = phone.lock().await.disconnected();
                emit(o, &out_tx, &ev_tx);
            })
        };
        let ticker = {
            let (phone, out_tx, ev_tx, clock) = (phone.clone(), out_tx.clone(), ev_tx.clone(), clock.clone());
            tokio::spawn(async move {
                loop {
                    tokio::time::sleep(Duration::from_millis(40)).await;
                    let o = phone.lock().await.tick(clock.now());
                    emit(o, &out_tx, &ev_tx);
                }
            })
        };
        let me = Self { phone, out: out_tx, events, clock, tasks: vec![writer, reader, ticker] };
        me.wait_open().await?;
        Ok(me)
    }

    async fn wait_open(&self) -> anyhow::Result<()> {
        let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
        while tokio::time::Instant::now() < deadline {
            if self.phone.lock().await.is_open() {
                return Ok(());
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        anyhow::bail!("the phone did not authenticate with the relay")
    }

    fn send(&self, o: Output) {
        for f in o.frames {
            let _ = self.out.send(f);
        }
    }

    /// One API call, like the PWA's transport.
    pub async fn call(&self, method: &str, params: Value) -> anyhow::Result<Value> {
        let r = self.phone.lock().await.call(method, &params, self.clock.now());
        match r {
            Ok((v, o)) => {
                self.send(o);
                Ok(v)
            }
            Err(e) => anyhow::bail!("phone {method}: {e}"),
        }
    }

    pub async fn start_join(&self, code: &str, name: &str) -> anyhow::Result<()> {
        let o = self.phone.lock().await.start_join(code, Some(name)).map_err(|e| anyhow::anyhow!("{e}"))?;
        self.send(o);
        Ok(())
    }

    /// Waits for an event whose `event.type` is `kind` and that matches `pred`.
    pub async fn wait_event<F: Fn(&Value) -> bool>(&mut self, kind: &str, pred: F) -> anyhow::Result<Value> {
        let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
        loop {
            let left = deadline.saturating_duration_since(tokio::time::Instant::now());
            match tokio::time::timeout(left, self.events.recv()).await {
                Ok(Ok(e)) if e["event"]["type"] == kind && pred(&e) => return Ok(e),
                Ok(Ok(_)) | Ok(Err(broadcast::error::RecvError::Lagged(_))) => continue,
                _ => anyhow::bail!("phone: timed out waiting for {kind}"),
            }
        }
    }
}
