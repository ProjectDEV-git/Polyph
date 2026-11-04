//! The TUI's link to the daemon: concurrent calls and a stream of events.
//! [`Backend`] is what the app needs; [`WsConn`] provides it over the local
//! API's WebSocket, and tests provide it in-process.

use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use futures_util::{SinkExt, StreamExt};
use serde_json::{Value, json};
use tokio::sync::{Mutex, mpsc, oneshot};
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;

pub type BoxFut<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

pub trait Backend: Send + Sync + 'static {
    /// One local API call; errors are the daemon's message, ready to show.
    fn call(&self, method: &str, params: Value) -> BoxFut<'_, Result<Value, String>>;
}

type Pending = Arc<Mutex<HashMap<u64, oneshot::Sender<Result<Value, String>>>>>;

pub struct WsConn {
    out: mpsc::UnboundedSender<String>,
    pending: Pending,
    next: AtomicU64,
}

impl std::fmt::Debug for WsConn {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("WsConn")
    }
}

/// Connects to the daemon at `port` with `token`. Events (`{room, event}`)
/// arrive on the returned receiver; it closes when the connection does.
pub async fn connect(port: u16, token: &str) -> anyhow::Result<(Arc<WsConn>, mpsc::UnboundedReceiver<Value>)> {
    let mut req = format!("ws://127.0.0.1:{port}/ws").into_client_request()?;
    req.headers_mut().insert("authorization", format!("Bearer {}", token.trim()).parse()?);
    let (ws, _) = tokio_tungstenite::connect_async(req).await?;
    let (mut sink, mut stream) = ws.split();
    let (out_tx, mut out_rx) = mpsc::unbounded_channel::<String>();
    let (ev_tx, ev_rx) = mpsc::unbounded_channel::<Value>();
    let pending: Pending = Default::default();
    tokio::spawn(async move {
        while let Some(m) = out_rx.recv().await {
            if sink.send(Message::Text(m.into())).await.is_err() {
                break;
            }
        }
    });
    let p2 = pending.clone();
    tokio::spawn(async move {
        while let Some(Ok(m)) = stream.next().await {
            let Message::Text(t) = m else { continue };
            let Ok(v) = serde_json::from_str::<Value>(&t) else { continue };
            if let Some(id) = v.get("id").and_then(|i| i.as_u64()) {
                if let Some(tx) = p2.lock().await.remove(&id) {
                    let r = match v.get("error") {
                        Some(e) => Err(e.get("message").and_then(|m| m.as_str()).unwrap_or("error").to_string()),
                        None => Ok(v.get("result").cloned().unwrap_or(Value::Null)),
                    };
                    let _ = tx.send(r);
                }
            } else if v.get("method").and_then(|m| m.as_str()) == Some("event") {
                let _ = ev_tx.send(v["params"].clone());
            }
        }
        // Connection gone: fail whatever still waits.
        for (_, tx) in p2.lock().await.drain() {
            let _ = tx.send(Err("lost the connection to the daemon".into()));
        }
    });
    Ok((Arc::new(WsConn { out: out_tx, pending, next: AtomicU64::new(1) }), ev_rx))
}

impl Backend for WsConn {
    fn call(&self, method: &str, params: Value) -> BoxFut<'_, Result<Value, String>> {
        let id = self.next.fetch_add(1, Ordering::Relaxed);
        let msg = json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params }).to_string();
        Box::pin(async move {
            let (tx, rx) = oneshot::channel();
            self.pending.lock().await.insert(id, tx);
            self.out.send(msg).map_err(|_| "lost the connection to the daemon".to_string())?;
            rx.await.map_err(|_| "lost the connection to the daemon".to_string())?
        })
    }
}
