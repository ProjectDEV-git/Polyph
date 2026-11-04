//! Polyph's MCP server, given to every agent in ACP `session/new`.
//!
//! The server is a thin stdio process (`polyph mcp`) that runs inside the
//! agent's sandbox. It lists the tools in [`tools`] and forwards each call to
//! the daemon through the agent's own network proxy, at the reserved host
//! `polyph.internal` (see `polyph_sandbox::proxy`). The daemon knows which
//! agent is calling from the proxy the request arrived on, and checks the
//! per-agent token in [`ENV_TOKEN`] as well.
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

#[cfg(test)]
mod tests {
    #[test]
    fn internal_host_matches_the_proxy() {
        assert_eq!(super::INTERNAL_HOST, polyph_sandbox::proxy::INTERNAL_HOST);
    }
}
