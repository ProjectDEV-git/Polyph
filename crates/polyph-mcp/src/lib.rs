//! Polyph's MCP server, given to every agent in ACP `session/new`.
//!
//! The server is a thin stdio process (`polyph mcp`) that runs inside the
//! agent's sandbox. It lists the tools in [`tools`] and forwards each call to
//! the daemon through the agent's own network proxy, at the reserved host
//! `polyph.internal` (see `polyph_sandbox::proxy`). The daemon knows which
//! agent is calling from the proxy the request arrived on, and checks the
//! per-agent token in [`ENV_TOKEN`] as well.
//!
//! Agents the person runs themselves (Claude Code, Codex, Cursor...) use the
//! same server and tools: outside a sandbox, `polyph mcp` talks to the local
//! daemon instead ([`bridge::LocalBridge`]), or the agent points at the
//! daemon's `/mcp` web address, which answers with the same [`server::step`].
//!
//! MCP messages are our own lean serde-free JSON handling, for the same
//! reason as the ACP types: the server needs four methods.

pub mod bridge;
pub mod server;
pub mod tools;

/// Proxy URL (`http://host:port`) the server sends tool calls through.
pub const ENV_PROXY: &str = "POLYPH_MCP_PROXY";
/// Per-agent secret the daemon checks on every call.
pub const ENV_TOKEN: &str = "POLYPH_MCP_TOKEN";
/// Name of the server as agents see it (tools appear as `polyph.<tool>` or
/// `mcp__polyph__<tool>` depending on the agent).
pub const SERVER_NAME: &str = "polyph";
/// Must equal `polyph_sandbox::proxy::INTERNAL_HOST` (checked by a test).
pub const INTERNAL_HOST: &str = "polyph.internal";
/// Path on the internal host that receives tool calls.
pub const CALL_PATH: &str = "/mcp/call";

/// What a tool call returns, as the daemon sends it back.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ToolOutput {
    /// Plain text for the model.
    pub text: String,
    /// The same result as structured data.
    #[serde(default)]
    pub data: serde_json::Value,
    #[serde(default)]
    pub error: bool,
}

impl ToolOutput {
    pub fn ok(text: impl Into<String>, data: serde_json::Value) -> Self {
        Self { text: text.into(), data, error: false }
    }

    pub fn err(text: impl Into<String>) -> Self {
        Self { text: text.into(), data: serde_json::Value::Null, error: true }
    }
}

/// Runs the server on stdin/stdout with the daemon bridge from the environment.
pub async fn run_stdio() -> anyhow::Result<()> {
    let backend = bridge::HttpBridge::from_env()?;
    server::serve(tokio::io::stdin(), tokio::io::stdout(), std::sync::Arc::new(backend)).await
}

/// True when this process runs inside an agent Polyph started (its sandbox
/// sets the proxy); otherwise `polyph mcp` serves an agent the person runs.
pub fn inside_polyph() -> bool {
    std::env::var_os(ENV_PROXY).is_some()
}

/// Runs the server on stdin/stdout for an agent the person runs themselves,
/// connected to the local daemon.
pub async fn run_direct(bridge: bridge::LocalBridge) -> anyhow::Result<()> {
    use server::Backend;
    let bridge = std::sync::Arc::new(bridge);
    // Show up in the room at once; if this fails, the first call says why.
    let _ = bridge.connect().await;
    tokio::select! {
        r = server::serve(tokio::io::stdin(), tokio::io::stdout(), bridge.clone()) => r,
        // MCP clients may stop a stdio server with a signal instead of
        // closing stdin. Leave the room either way, or the agent stays listed
        // (and its next start gets a "2" label) until the idle timeout.
        () = shutdown_signal() => {
            let _ = tokio::time::timeout(std::time::Duration::from_secs(2), bridge.close()).await;
            // Handling the signal replaced its default action, which was to
            // exit. Exit here: the runtime would otherwise wait for the
            // blocking stdin read, which nothing can cancel.
            std::process::exit(0)
        }
    }
}

/// SIGTERM, SIGHUP or Ctrl-C; on other platforms Ctrl-C (Ctrl-Break, console close).
async fn shutdown_signal() {
    #[cfg(unix)]
    {
        use tokio::signal::unix::{SignalKind, signal};
        let (Ok(mut term), Ok(mut hup)) = (signal(SignalKind::terminate()), signal(SignalKind::hangup())) else {
            let _ = tokio::signal::ctrl_c().await;
            return;
        };
        tokio::select! {
            _ = term.recv() => {}
            _ = hup.recv() => {}
            _ = tokio::signal::ctrl_c() => {}
        }
    }
    #[cfg(not(unix))]
    {
        let _ = tokio::signal::ctrl_c().await;
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn internal_host_matches_the_proxy() {
        assert_eq!(super::INTERNAL_HOST, polyph_sandbox::proxy::INTERNAL_HOST);
    }
}
