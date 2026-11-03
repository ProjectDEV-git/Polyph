//! JSON-RPC 2.0 over newline-delimited JSON, as ACP uses on stdio. Both
//! directions can send requests: the agent asks the client for permission
//! and file access while a prompt is running.

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncWrite, AsyncWriteExt, BufReader};
use tokio::sync::{Mutex, mpsc, oneshot};
use tokio::task::JoinHandle;

/// Longest line accepted from a peer (agents can send large diffs).
pub const MAX_LINE: usize = 16 * 1024 * 1024;

#[derive(Debug, thiserror::Error)]
pub enum RpcError {
    #[error("the agent exited or closed its connection")]
    Closed,
    #[error("the agent returned an error: {message} ({code})")]
    Remote { code: i64, message: String },
    #[error("unexpected response: {0}")]
    Decode(String),
}

/// A request from the peer that we must answer.
#[derive(Debug)]
pub struct Incoming {
    pub method: String,
    pub params: Value,
    reply: Option<oneshot::Sender<Result<Value, (i64, String)>>>,
}

impl Incoming {
    pub fn respond(mut self, r: Result<Value, (i64, String)>) {
        if let Some(tx) = self.reply.take() {
            let _ = tx.send(r);
        }
    }
    pub fn is_notification(&self) -> bool {
        self.reply.is_none()
    }
}

type Pending = Arc<Mutex<HashMap<u64, oneshot::Sender<Result<Value, RpcError>>>>>;

#[derive(Clone)]
pub struct Peer {
    out: mpsc::Sender<String>,
    pending: Pending,
    next: Arc<AtomicU64>,
}

impl std::fmt::Debug for Peer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Peer")
    }
}

impl Peer {
    pub async fn request<P: Serialize, R: DeserializeOwned>(&self, method: &str, params: &P) -> Result<R, RpcError> {
        let id = self.next.fetch_add(1, Ordering::Relaxed);
        let (tx, rx) = oneshot::channel();
        self.pending.lock().await.insert(id, tx);
        let msg = json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params });
        self.out.send(msg.to_string()).await.map_err(|_| RpcError::Closed)?;
        let v = rx.await.map_err(|_| RpcError::Closed)??;
        serde_json::from_value(v).map_err(|e| RpcError::Decode(e.to_string()))
    }

    pub async fn notify<P: Serialize>(&self, method: &str, params: &P) -> Result<(), RpcError> {
        let msg = json!({ "jsonrpc": "2.0", "method": method, "params": params });
        self.out.send(msg.to_string()).await.map_err(|_| RpcError::Closed)
    }
}

/// Handles a notification inline, before the next message is read, so
/// notifications sent before a response are always processed before it.
pub type NotificationHandler =
    Arc<dyn Fn(String, Value) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send>> + Send + Sync>;

/// Starts reading and writing. Incoming requests (and notifications, when no
/// handler is given) arrive on the returned receiver; the caller must answer requests.
pub fn connect<R, W>(read: R, write: W) -> (Peer, mpsc::Receiver<Incoming>, Vec<JoinHandle<()>>)
where
    R: AsyncRead + Unpin + Send + 'static,
    W: AsyncWrite + Unpin + Send + 'static,
{
    connect_with(read, write, None)
}

pub fn connect_with<R, W>(
    read: R,
    write: W,
    on_notify: Option<NotificationHandler>,
) -> (Peer, mpsc::Receiver<Incoming>, Vec<JoinHandle<()>>)
where
    R: AsyncRead + Unpin + Send + 'static,
    W: AsyncWrite + Unpin + Send + 'static,
{
    let (out_tx, mut out_rx) = mpsc::channel::<String>(256);
    let (in_tx, in_rx) = mpsc::channel::<Incoming>(256);
    let pending: Pending = Arc::new(Mutex::new(HashMap::new()));
    let peer = Peer { out: out_tx.clone(), pending: pending.clone(), next: Arc::new(AtomicU64::new(1)) };
    let writer = tokio::spawn(async move {
        let mut w = write;
        while let Some(line) = out_rx.recv().await {
            if w.write_all(line.as_bytes()).await.is_err()
                || w.write_all(b"\n").await.is_err()
                || w.flush().await.is_err()
            {
                break;
            }
        }
    });
    let reader = tokio::spawn(async move {
        let mut r = BufReader::new(read);
        let mut line = String::new();
        loop {
            line.clear();
            match r.read_line(&mut line).await {
                Ok(0) | Err(_) => break,
                Ok(_) => {}
            }
            if line.len() > MAX_LINE {
                continue;
            }
            let Ok(v) = serde_json::from_str::<Value>(line.trim()) else { continue };
            let id = v.get("id").cloned();
            if let Some(method) = v.get("method").and_then(|m| m.as_str()) {
                let params = v.get("params").cloned().unwrap_or(Value::Null);
                if id.as_ref().is_none_or(|i| i.is_null())
                    && let Some(h) = &on_notify
                {
                    h(method.to_string(), params).await;
                    continue;
                }
                let reply = match id {
                    Some(id) if !id.is_null() => {
                        let (tx, rx) = oneshot::channel::<Result<Value, (i64, String)>>();
                        let out = out_tx.clone();
                        tokio::spawn(async move {
                            let resp = match rx.await {
                                Ok(Ok(result)) => json!({ "jsonrpc": "2.0", "id": id, "result": result }),
                                Ok(Err((code, message))) => {
                                    json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } })
                                }
                                Err(_) => {
                                    json!({ "jsonrpc": "2.0", "id": id, "error": { "code": -32603, "message": "request dropped" } })
                                }
                            };
                            let _ = out.send(resp.to_string()).await;
                        });
                        Some(tx)
                    }
                    _ => None,
                };
                if in_tx.send(Incoming { method: method.to_string(), params, reply }).await.is_err() {
                    break;
                }
            } else if let Some(id) = id.and_then(|i| i.as_u64()) {
                let res = match v.get("error") {
                    Some(e) => Err(RpcError::Remote {
                        code: e.get("code").and_then(|c| c.as_i64()).unwrap_or(0),
                        message: e.get("message").and_then(|m| m.as_str()).unwrap_or("").to_string(),
                    }),
                    None => Ok(v.get("result").cloned().unwrap_or(Value::Null)),
                };
                if let Some(tx) = pending.lock().await.remove(&id) {
                    let _ = tx.send(res);
                }
            }
        }
        // Fail everything still waiting.
        for (_, tx) in pending.lock().await.drain() {
            let _ = tx.send(Err(RpcError::Closed));
        }
    });
    (peer, in_rx, vec![writer, reader])
}
