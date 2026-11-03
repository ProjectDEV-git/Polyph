//! Talks to the local daemon over its WebSocket API, starting the daemon
//! first when it is not running.

use std::path::Path;
use std::process::Stdio;
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use polyph_daemon::Paths;
use serde_json::{Value, json};
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;

type Ws = tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

#[derive(Debug)]
pub enum CallError {
    /// The daemon answered with an error message meant for the user.
    Api(String),
    /// The daemon could not be reached.
    Unreachable(String),
}

impl std::fmt::Display for CallError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CallError::Api(m) | CallError::Unreachable(m) => f.write_str(m),
        }
    }
}

pub struct Api {
    ws: Ws,
    next: u64,
}

impl std::fmt::Debug for Api {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Api")
    }
}

pub fn daemon_port(paths: &Paths) -> Option<u16> {
    let s = std::fs::read_to_string(paths.daemon_info()).ok()?;
    let v: Value = serde_json::from_str(&s).ok()?;
    v.get("port")?.as_u64().map(|p| p as u16)
}

pub async fn healthy(port: u16) -> bool {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let Ok(Ok(mut s)) =
        tokio::time::timeout(Duration::from_millis(500), tokio::net::TcpStream::connect(("127.0.0.1", port))).await
    else {
        return false;
    };
    if s.write_all(b"GET /healthz HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n").await.is_err() {
        return false;
    }
    let mut buf = vec![0u8; 256];
    matches!(tokio::time::timeout(Duration::from_millis(500), s.read(&mut buf)).await, Ok(Ok(n)) if n > 0 && buf[..n].starts_with(b"HTTP/1.1 200"))
}

/// Starts `polyph daemon run` detached from this terminal.
pub fn spawn_daemon(home: &Path) -> std::io::Result<()> {
    let exe = std::env::current_exe()?;
    let mut cmd = std::process::Command::new(exe);
    cmd.args(["daemon", "run"])
        .env("POLYPH_HOME", home)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        // Own process group, so closing the terminal does not stop the daemon.
        cmd.process_group(0);
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const DETACHED_PROCESS: u32 = 0x0000_0008;
        const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
        cmd.creation_flags(DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP);
    }
    cmd.spawn().map(|_| ())
}

/// Returns the daemon's port, starting it if needed.
pub async fn ensure_daemon(paths: &Paths, autostart: bool) -> Result<u16, CallError> {
    if let Some(p) = daemon_port(paths)
        && healthy(p).await
    {
        return Ok(p);
    }
    if !autostart {
        return Err(CallError::Unreachable("not running".into()));
    }
    spawn_daemon(&paths.home).map_err(|e| CallError::Unreachable(e.to_string()))?;
    for _ in 0..100 {
        tokio::time::sleep(Duration::from_millis(100)).await;
        if let Some(p) = daemon_port(paths)
            && healthy(p).await
        {
            return Ok(p);
        }
    }
    Err(CallError::Unreachable("the daemon did not start in time".into()))
}

impl Api {
    pub async fn connect(paths: &Paths, autostart: bool) -> Result<Self, CallError> {
        let port = ensure_daemon(paths, autostart).await?;
        let token = std::fs::read_to_string(paths.token()).map_err(|e| CallError::Unreachable(e.to_string()))?;
        let mut req = format!("ws://127.0.0.1:{port}/ws")
            .into_client_request()
            .map_err(|e| CallError::Unreachable(e.to_string()))?;
        let auth =
            format!("Bearer {}", token.trim()).parse().map_err(|_| CallError::Unreachable("bad token".into()))?;
        req.headers_mut().insert("authorization", auth);
        let (ws, _) = tokio_tungstenite::connect_async(req).await.map_err(|e| CallError::Unreachable(e.to_string()))?;
        Ok(Self { ws, next: 1 })
    }

    pub async fn call(&mut self, method: &str, params: Value) -> Result<Value, CallError> {
        self.call_with_events(method, params, |_| {}).await
    }

    /// Calls a method, passing every event that arrives meanwhile to `on_event`.
    pub async fn call_with_events(
        &mut self,
        method: &str,
        params: Value,
        mut on_event: impl FnMut(&Value),
    ) -> Result<Value, CallError> {
        let id = self.next;
        self.next += 1;
        let req = json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params });
        self.ws.send(Message::Text(req.to_string().into())).await.map_err(|e| CallError::Unreachable(e.to_string()))?;
        while let Some(m) = self.ws.next().await {
            let text = match m {
                Ok(Message::Text(t)) => t.to_string(),
                Ok(_) => continue,
                Err(e) => return Err(CallError::Unreachable(e.to_string())),
            };
            let Ok(v) = serde_json::from_str::<Value>(&text) else { continue };
            if v.get("id").and_then(|i| i.as_u64()) == Some(id) {
                if let Some(err) = v.get("error") {
                    return Err(CallError::Api(
                        err.get("message").and_then(|m| m.as_str()).unwrap_or("error").to_string(),
                    ));
                }
                return Ok(v.get("result").cloned().unwrap_or(Value::Null));
            }
            if v.get("method").is_some() {
                on_event(&v);
            }
        }
        Err(CallError::Unreachable("connection closed".into()))
    }

    /// Next notification (`event` or `status`), for `--watch` modes.
    pub async fn next_notification(&mut self) -> Option<Value> {
        while let Some(Ok(m)) = self.ws.next().await {
            if let Message::Text(t) = m
                && let Ok(v) = serde_json::from_str::<Value>(&t)
                && v.get("method").is_some()
            {
                return Some(v);
            }
        }
        None
    }
}
