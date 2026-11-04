//! The TUI talking to a daemon in the same process, for tests.

use std::sync::Arc;

use polyph_daemon::Daemon;
use polyph_tui::conn::{Backend, BoxFut};
use serde_json::Value;
use tokio::sync::mpsc;

#[derive(Debug, Clone)]
pub struct InProcess(pub Arc<Daemon>);

impl Backend for InProcess {
    fn call(&self, method: &str, params: Value) -> BoxFut<'_, Result<Value, String>> {
        let method = method.to_string();
        Box::pin(async move { self.0.call(&method, params).await.map_err(|e| e.to_string()) })
    }
}

/// The daemon's events as the WebSocket delivers them (`{room, event}`).
pub fn events(d: &Arc<Daemon>) -> mpsc::UnboundedReceiver<Value> {
    let mut rx = d.subscribe();
    let (tx, out) = mpsc::unbounded_channel();
    tokio::spawn(async move {
        loop {
            match rx.recv().await {
                Ok(e) => {
                    if tx.send(serde_json::to_value(&e).unwrap_or(Value::Null)).is_err() {
                        return;
                    }
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                Err(_) => return,
            }
        }
    });
    out
}
