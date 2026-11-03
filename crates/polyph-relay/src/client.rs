//! A relay client: connects, authenticates the device, then exchanges frames
//! over channels. Used by the daemon, the test kit and LAN mode.

use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use polyph_proto::identity::{DeviceIdentity, ctx};
use polyph_proto::ids::DeviceId;
use polyph_proto::relay_wire::{ClientFrame, MAX_FRAME, RelayFrame, auth_message};
use polyph_proto::version::PROTOCOL_VERSION;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::tungstenite::protocol::WebSocketConfig;
use tokio_tungstenite::{Connector, MaybeTlsStream, WebSocketStream};

#[derive(Debug, thiserror::Error)]
pub enum ClientError {
    #[error("could not connect to the relay at {url}: {reason}")]
    Connect { url: String, reason: String },
    #[error("the relay rejected this device: {0}")]
    Rejected(String),
    #[error("the relay closed the connection")]
    Closed,
    #[error("protocol error: {0}")]
    Protocol(String),
}

type Ws = WebSocketStream<MaybeTlsStream<tokio::net::TcpStream>>;

#[derive(Debug, Clone)]
pub struct RelaySender {
    tx: mpsc::Sender<ClientFrame>,
}

impl RelaySender {
    pub async fn send(&self, f: ClientFrame) -> Result<(), ClientError> {
        self.tx.send(f).await.map_err(|_| ClientError::Closed)
    }
}

#[derive(Debug)]
pub struct RelayClient {
    pub device: DeviceId,
    pub url: String,
    sender: RelaySender,
    rx: mpsc::Receiver<RelayFrame>,
    task: JoinHandle<()>,
}

impl Drop for RelayClient {
    fn drop(&mut self) {
        self.task.abort();
    }
}

async fn read_frame(ws: &mut Ws) -> Result<RelayFrame, ClientError> {
    loop {
        match tokio::time::timeout(Duration::from_secs(15), ws.next()).await {
            Ok(Some(Ok(Message::Binary(b)))) => {
                return RelayFrame::decode(&b).map_err(|e| ClientError::Protocol(e.to_string()));
            }
            Ok(Some(Ok(Message::Ping(_) | Message::Pong(_) | Message::Frame(_)))) => continue,
            Ok(Some(Ok(_))) => return Err(ClientError::Protocol("unexpected text frame".into())),
            Ok(Some(Err(e))) => return Err(ClientError::Protocol(e.to_string())),
            Ok(None) => return Err(ClientError::Closed),
            Err(_) => return Err(ClientError::Protocol("timed out waiting for the relay".into())),
        }
    }
}

async fn write_frame(ws: &mut Ws, f: &ClientFrame) -> Result<(), ClientError> {
    let b = f.encode().map_err(|e| ClientError::Protocol(e.to_string()))?;
    ws.send(Message::Binary(b.into())).await.map_err(|_| ClientError::Closed)
}

impl RelayClient {
    /// Connects and authenticates. `pin` is the SHA-256 fingerprint of a LAN
    /// relay's self-signed certificate; normal relays use public CAs.
    pub async fn connect(url: &str, identity: &DeviceIdentity, pin: Option<[u8; 32]>) -> Result<Self, ClientError> {
        let mut wscfg = WebSocketConfig::default();
        wscfg.max_message_size = Some(MAX_FRAME);
        wscfg.max_frame_size = Some(MAX_FRAME);
        let connector = match pin {
            Some(fp) => Some(Connector::Rustls(
                crate::tls::pinned_client(fp)
                    .map_err(|e| ClientError::Connect { url: url.into(), reason: e.to_string() })?,
            )),
            None => None,
        };
        let connect = tokio_tungstenite::connect_async_tls_with_config(url, Some(wscfg), false, connector);
        let (mut ws, _) = tokio::time::timeout(Duration::from_secs(15), connect)
            .await
            .map_err(|_| ClientError::Connect { url: url.into(), reason: "timed out".into() })?
            .map_err(|e| ClientError::Connect { url: url.into(), reason: e.to_string() })?;

        let device = identity.device_id();
        write_frame(
            &mut ws,
            &ClientFrame::Hello {
                protocol: PROTOCOL_VERSION,
                device,
                client: format!("polyph/{}", env!("CARGO_PKG_VERSION")),
            },
        )
        .await?;
        let nonce = match read_frame(&mut ws).await? {
            RelayFrame::Challenge { nonce, .. } => nonce,
            RelayFrame::Error { detail, .. } => return Err(ClientError::Rejected(detail)),
            other => return Err(ClientError::Protocol(format!("expected a challenge, got {other:?}"))),
        };
        let sig = identity.sign(ctx::RELAY_AUTH, &auth_message(&nonce, &device));
        write_frame(&mut ws, &ClientFrame::Auth { sig }).await?;
        match read_frame(&mut ws).await? {
            RelayFrame::Welcome { device: d } if d == device => {}
            RelayFrame::Error { detail, .. } => return Err(ClientError::Rejected(detail)),
            other => return Err(ClientError::Protocol(format!("expected welcome, got {other:?}"))),
        }

        let (out_tx, mut out_rx) = mpsc::channel::<ClientFrame>(1024);
        let (in_tx, in_rx) = mpsc::channel::<RelayFrame>(4096);
        let task = tokio::spawn(async move {
            let mut ping = tokio::time::interval(Duration::from_secs(30));
            ping.tick().await;
            loop {
                tokio::select! {
                    f = out_rx.recv() => match f {
                        Some(f) => if write_frame(&mut ws, &f).await.is_err() { break },
                        None => { let _ = ws.close(None).await; break }
                    },
                    m = ws.next() => match m {
                        Some(Ok(Message::Binary(b))) => match RelayFrame::decode(&b) {
                            Ok(f) => if in_tx.send(f).await.is_err() { break },
                            Err(_) => continue,
                        },
                        Some(Ok(Message::Close(_))) | None | Some(Err(_)) => break,
                        Some(Ok(_)) => continue,
                    },
                    _ = ping.tick() => if write_frame(&mut ws, &ClientFrame::Ping).await.is_err() { break },
                }
            }
        });
        Ok(Self { device, url: url.to_string(), sender: RelaySender { tx: out_tx }, rx: in_rx, task })
    }

    pub fn sender(&self) -> RelaySender {
        self.sender.clone()
    }

    pub async fn send(&self, f: ClientFrame) -> Result<(), ClientError> {
        self.sender.send(f).await
    }

    pub async fn recv(&mut self) -> Option<RelayFrame> {
        self.rx.recv().await
    }

    /// Receives with a timeout; `None` on timeout or close.
    pub async fn recv_timeout(&mut self, d: Duration) -> Option<RelayFrame> {
        tokio::time::timeout(d, self.rx.recv()).await.ok().flatten()
    }

    /// Splits into a sender and the receiving channel, for event loops.
    pub fn into_parts(mut self) -> (RelaySender, mpsc::Receiver<RelayFrame>, JoinHandle<()>) {
        let (_, dummy) = mpsc::channel(1);
        let rx = std::mem::replace(&mut self.rx, dummy);
        let task = std::mem::replace(&mut self.task, tokio::spawn(async {}));
        (self.sender.clone(), rx, task)
    }
}
