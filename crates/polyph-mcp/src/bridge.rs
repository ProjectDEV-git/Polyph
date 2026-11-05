//! Sends tool calls to the daemon through the agent's proxy, as plain HTTP to
//! the internal host. No HTTP library: one request per connection.

use std::future::Future;
use std::pin::Pin;
use std::time::Duration;

use serde_json::{Value, json};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

use crate::server::Backend;
use crate::{CALL_PATH, ENV_PROXY, ENV_TOKEN, ToolOutput};

/// Longest a call may take (ask_agent waits up to 300 s for a reply).
const CALL_TIMEOUT: Duration = Duration::from_secs(330);
const MAX_RESPONSE: usize = 4 * 1024 * 1024;

#[derive(Debug, Clone)]
pub struct HttpBridge {
    /// `host:port` of the proxy.
    pub proxy: String,
    pub token: String,
}

impl HttpBridge {
    pub fn from_env() -> anyhow::Result<Self> {
        let url = std::env::var(ENV_PROXY)
            .or_else(|_| std::env::var("HTTP_PROXY"))
            .map_err(|_| anyhow::anyhow!("{ENV_PROXY} is not set; `polyph mcp` runs only inside a Polyph agent"))?;
        let token = std::env::var(ENV_TOKEN).unwrap_or_default();
        Ok(Self::new(&url, token))
    }

    pub fn new(url: &str, token: String) -> Self {
        let proxy = url.trim_start_matches("http://").trim_end_matches('/').to_string();
        Self { proxy, token }
    }

    async fn post(&self, body: &[u8]) -> anyhow::Result<(u16, Vec<u8>)> {
        let target = format!("http://{}{CALL_PATH}", crate::INTERNAL_HOST);
        http_post(&self.proxy, &target, crate::INTERNAL_HOST, &self.token, body).await
    }
}

/// One HTTP/1.1 POST with `Connection: close`; returns status and body.
async fn http_post(addr: &str, target: &str, host: &str, token: &str, body: &[u8]) -> anyhow::Result<(u16, Vec<u8>)> {
    let mut s = TcpStream::connect(addr).await?;
    let head = format!(
        "POST {target} HTTP/1.1\r\nHost: {host}\r\nAuthorization: Bearer {token}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len(),
    );
    s.write_all(head.as_bytes()).await?;
    s.write_all(body).await?;
    let mut buf = vec![];
    (&mut s).take(MAX_RESPONSE as u64).read_to_end(&mut buf).await?;
    let split = buf
        .windows(4)
        .position(|w| w == b"\r\n\r\n")
        .ok_or_else(|| anyhow::anyhow!("malformed response from the daemon"))?;
    let status =
        std::str::from_utf8(&buf[..split])?.split_whitespace().nth(1).and_then(|s| s.parse().ok()).unwrap_or(0);
    Ok((status, buf[split + 4..].to_vec()))
}

/// For agents the person runs themselves: `polyph mcp` outside a Polyph
/// sandbox reaches the daemon's local API (`POST /rpc`) with the API token,
/// joins the room as a connected agent (`mcp.connect`) and forwards each
/// tool call (`mcp.call`). Reconnects once when the daemon has restarted.
#[derive(Debug)]
pub struct LocalBridge {
    /// The daemon's `daemon.json` (its port) and `api.token`, read on every
    /// call so a restarted daemon on a new port is found again.
    info: std::path::PathBuf,
    token: std::path::PathBuf,
    connect: Value,
    session: tokio::sync::Mutex<Option<String>>,
}

impl LocalBridge {
    /// `client` is how the agent shows in the room (`claude-code`, `codex`...).
    pub fn new(
        info: std::path::PathBuf,
        token: std::path::PathBuf,
        client: &str,
        room: Option<&str>,
        cwd: &std::path::Path,
    ) -> Self {
        let connect = json!({ "client": client, "room": room, "cwd": cwd, "via": "stdio" });
        Self { info, token, connect, session: tokio::sync::Mutex::new(None) }
    }

    async fn rpc(&self, method: &str, params: Value) -> Result<Value, String> {
        let body = json!({ "jsonrpc": "2.0", "id": 1, "method": method, "params": params }).to_string();
        let port = std::fs::read(&self.info)
            .ok()
            .and_then(|b| serde_json::from_slice::<Value>(&b).ok())
            .and_then(|v| v.get("port").and_then(|p| p.as_u64()))
            .ok_or("the Polyph daemon is not running; start it with `polyph daemon start`")?;
        let token = std::fs::read_to_string(&self.token).map_err(|e| format!("cannot read the API token: {e}"))?;
        let addr = format!("127.0.0.1:{port}");
        let (status, b) = http_post(&addr, "/rpc", "127.0.0.1", token.trim(), body.as_bytes())
            .await
            .map_err(|e| format!("could not reach the Polyph daemon: {e}. Run `polyph doctor`."))?;
        if status != 200 {
            return Err(format!("the Polyph daemon refused the call ({status}). Run `polyph doctor`."));
        }
        let v: Value = serde_json::from_slice(&b).map_err(|_| "Polyph returned an unreadable answer".to_string())?;
        match v.get("error") {
            Some(e) => Err(e.get("message").and_then(|m| m.as_str()).unwrap_or("error").to_string()),
            None => Ok(v.get("result").cloned().unwrap_or(Value::Null)),
        }
    }

    /// Joins the room now, so the agent shows up before its first tool call.
    pub async fn connect(&self) -> Result<Value, String> {
        let r = self.rpc("mcp.connect", self.connect.clone()).await?;
        let s = r.get("session").and_then(|s| s.as_str()).ok_or("no session in the answer")?.to_string();
        *self.session.lock().await = Some(s);
        Ok(r)
    }

    async fn session(&self) -> Result<String, String> {
        if let Some(s) = self.session.lock().await.clone() {
            return Ok(s);
        }
        self.connect().await?;
        self.session.lock().await.clone().ok_or_else(|| "not connected".into())
    }

    async fn call_once(&self, tool: &str, args: &Value) -> Result<ToolOutput, String> {
        let session = self.session().await?;
        let r = self.rpc("mcp.call", json!({ "session": session, "tool": tool, "args": args })).await?;
        serde_json::from_value(r).map_err(|_| "Polyph returned an unreadable answer".into())
    }
}

impl Backend for LocalBridge {
    fn call(&self, tool: String, args: Value) -> Pin<Box<dyn Future<Output = ToolOutput> + Send + '_>> {
        Box::pin(async move {
            let first = tokio::time::timeout(CALL_TIMEOUT, self.call_once(&tool, &args)).await;
            let r = match first {
                // The daemon restarted or closed an idle connection: join again.
                Ok(Err(e)) if e.contains("reconnect") => {
                    *self.session.lock().await = None;
                    match tokio::time::timeout(CALL_TIMEOUT, self.call_once(&tool, &args)).await {
                        Ok(r) => r,
                        Err(_) => Err("Polyph did not answer in time".into()),
                    }
                }
                Ok(r) => r,
                Err(_) => Err("Polyph did not answer in time".into()),
            };
            r.unwrap_or_else(ToolOutput::err)
        })
    }

    fn close(&self) -> Pin<Box<dyn Future<Output = ()> + Send + '_>> {
        Box::pin(async move {
            if let Some(s) = self.session.lock().await.take() {
                let _ = self.rpc("mcp.disconnect", json!({ "session": s })).await;
            }
        })
    }
}

impl Backend for HttpBridge {
    fn call(&self, tool: String, args: Value) -> Pin<Box<dyn Future<Output = ToolOutput> + Send + '_>> {
        Box::pin(async move {
            let body = json!({ "tool": tool, "args": args }).to_string();
            match tokio::time::timeout(CALL_TIMEOUT, self.post(body.as_bytes())).await {
                Ok(Ok((200, b))) => serde_json::from_slice(&b)
                    .unwrap_or_else(|_| ToolOutput::err("Polyph returned an unreadable answer")),
                Ok(Ok((status, b))) => {
                    let detail = serde_json::from_slice::<Value>(&b)
                        .ok()
                        .and_then(|v| v.get("error").and_then(|e| e.as_str()).map(str::to_string))
                        .unwrap_or_default();
                    ToolOutput::err(format!("Polyph refused the call ({status}) {detail}"))
                }
                Ok(Err(e)) => ToolOutput::err(format!("could not reach Polyph: {e}")),
                Err(_) => ToolOutput::err("Polyph did not answer in time"),
            }
        })
    }
}
