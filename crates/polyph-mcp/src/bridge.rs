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
        let mut s = TcpStream::connect(&self.proxy).await?;
        let head = format!(
            "POST http://{host}{CALL_PATH} HTTP/1.1\r\nHost: {host}\r\nAuthorization: Bearer {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            self.token,
            body.len(),
            host = crate::INTERNAL_HOST,
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
