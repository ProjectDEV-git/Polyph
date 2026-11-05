//! The MCP stdio loop: newline-delimited JSON-RPC 2.0.

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncWrite, AsyncWriteExt, BufReader};
use tokio::sync::mpsc;

use crate::{SERVER_NAME, ToolOutput, tools};

/// Protocol revisions this server speaks, newest first.
pub const VERSIONS: &[&str] = &["2025-06-18", "2025-03-26", "2024-11-05"];

/// Carries tool calls to the daemon.
pub trait Backend: Send + Sync + 'static {
    fn call(&self, tool: String, args: Value) -> Pin<Box<dyn Future<Output = ToolOutput> + Send + '_>>;
    /// The client went away.
    fn close(&self) -> Pin<Box<dyn Future<Output = ()> + Send + '_>> {
        Box::pin(async {})
    }
}

fn error(id: Value, code: i64, message: &str) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } })
}

fn result(id: Value, r: Value) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "result": r })
}

fn tool_result(out: ToolOutput, structured: bool) -> Value {
    let mut r = json!({ "content": [{ "type": "text", "text": out.text }], "isError": out.error });
    if structured && out.data.is_object() {
        r["structuredContent"] = out.data;
    }
    r
}

/// Instructions sent to the client on `initialize`.
pub const INSTRUCTIONS: &str = "You are working in a Polyph room with other people and their agents. \
Call get_context before you start (the project brief, your task and the notes for your files), claim files \
before editing them, ask the holder with ask_agent when a file is taken, and check read_inbox when told you \
have messages. When you are free, next_task gives you the next ready task.";

/// What to do with one incoming message.
#[derive(Debug)]
pub enum Step {
    /// Send this answer.
    Reply(Value),
    /// Run a tool and answer `id` with [`tool_result`].
    Call { id: Value, name: String, args: Value, structured: bool },
    /// A notification or a response: nothing to send.
    Nothing,
}

/// Handles one message; `version` is the negotiated protocol revision and
/// is updated by `initialize`.
pub fn step(msg: &Value, version: &mut &'static str) -> Step {
    let Some(method) = msg.get("method").and_then(|m| m.as_str()) else { return Step::Nothing };
    // Notifications (no id) need no answer.
    let Some(id) = msg.get("id").cloned() else { return Step::Nothing };
    let params = msg.get("params").cloned().unwrap_or(Value::Null);
    match method {
        "initialize" => {
            let asked = params.get("protocolVersion").and_then(|v| v.as_str()).unwrap_or_default();
            *version = VERSIONS.iter().find(|v| **v == asked).copied().unwrap_or(VERSIONS[0]);
            Step::Reply(result(
                id,
                json!({
                    "protocolVersion": *version,
                    "capabilities": { "tools": { "listChanged": false } },
                    "serverInfo": { "name": SERVER_NAME, "version": env!("CARGO_PKG_VERSION") },
                    "instructions": INSTRUCTIONS,
                }),
            ))
        }
        "ping" => Step::Reply(result(id, json!({}))),
        "tools/list" => Step::Reply(result(id, tools::list())),
        "tools/call" => {
            let name = params.get("name").and_then(|n| n.as_str()).unwrap_or_default().to_string();
            let args = params.get("arguments").cloned().unwrap_or_else(|| json!({}));
            if tools::find(&name).is_none() {
                return Step::Reply(error(id, -32602, &format!("unknown tool: {name}")));
            }
            Step::Call { id, name, args, structured: *version >= "2025-06-18" }
        }
        "resources/list" => Step::Reply(result(id, json!({ "resources": [] }))),
        "prompts/list" => Step::Reply(result(id, json!({ "prompts": [] }))),
        _ => Step::Reply(error(id, -32601, "method not found")),
    }
}

/// The answer to a tool call.
pub fn tool_reply(id: Value, out: ToolOutput, structured: bool) -> Value {
    result(id, tool_result(out, structured))
}

/// The answer to a message that could not be parsed.
pub fn parse_error() -> Value {
    error(Value::Null, -32700, "parse error")
}

/// Serves one client until its input closes.
pub async fn serve<R, W>(input: R, output: W, backend: Arc<dyn Backend>) -> anyhow::Result<()>
where
    R: AsyncRead + Unpin + Send + 'static,
    W: AsyncWrite + Unpin + Send + 'static,
{
    let (tx, mut rx) = mpsc::channel::<Value>(64);
    let writer = tokio::spawn(async move {
        let mut output = output;
        while let Some(v) = rx.recv().await {
            let mut line = v.to_string();
            line.push('\n');
            if output.write_all(line.as_bytes()).await.is_err() || output.flush().await.is_err() {
                break;
            }
        }
    });
    let mut lines = BufReader::new(input).lines();
    let mut version = VERSIONS[0];
    while let Some(line) = lines.next_line().await? {
        if line.trim().is_empty() {
            continue;
        }
        let msg: Value = match serde_json::from_str(&line) {
            Ok(v) => v,
            Err(_) => {
                let _ = tx.send(parse_error()).await;
                continue;
            }
        };
        match step(&msg, &mut version) {
            Step::Reply(v) => {
                let _ = tx.send(v).await;
            }
            Step::Call { id, name, args, structured } => {
                let (tx, backend) = (tx.clone(), backend.clone());
                // Calls run concurrently: ask_agent may wait for a reply.
                tokio::spawn(async move {
                    let out = backend.call(name, args).await;
                    let _ = tx.send(tool_reply(id, out, structured)).await;
                });
            }
            Step::Nothing => {}
        }
    }
    drop(tx);
    let _ = writer.await;
    backend.close().await;
    Ok(())
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt};

    struct Echo;
    impl Backend for Echo {
        fn call(&self, tool: String, args: Value) -> Pin<Box<dyn Future<Output = ToolOutput> + Send + '_>> {
            Box::pin(async move { ToolOutput::ok(format!("called {tool}"), json!({ "args": args })) })
        }
    }

    #[tokio::test]
    async fn handshake_list_and_call() {
        let (client, server) = tokio::io::duplex(64 * 1024);
        let (sr, sw) = tokio::io::split(server);
        let task = tokio::spawn(serve(sr, sw, Arc::new(Echo)));
        let (cr, mut cw) = tokio::io::split(client);
        let mut lines = tokio::io::BufReader::new(cr).lines();
        let mut ask = async |v: Value| {
            cw.write_all(format!("{v}\n").as_bytes()).await.unwrap();
        };
        ask(json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-03-26","capabilities":{},"clientInfo":{"name":"t","version":"1"}}})).await;
        let r: Value = serde_json::from_str(&lines.next_line().await.unwrap().unwrap()).unwrap();
        assert_eq!(r["result"]["protocolVersion"], "2025-03-26");
        ask(json!({"jsonrpc":"2.0","method":"notifications/initialized"})).await;
        ask(json!({"jsonrpc":"2.0","id":2,"method":"tools/list"})).await;
        let r: Value = serde_json::from_str(&lines.next_line().await.unwrap().unwrap()).unwrap();
        let names: Vec<&str> =
            r["result"]["tools"].as_array().unwrap().iter().map(|t| t["name"].as_str().unwrap()).collect();
        assert!(names.contains(&"claim_files") && names.contains(&"ask_agent"), "{names:?}");
        ask(json!({"jsonrpc":"2.0","id":"c","method":"tools/call","params":{"name":"list_claims","arguments":{}}}))
            .await;
        let r: Value = serde_json::from_str(&lines.next_line().await.unwrap().unwrap()).unwrap();
        assert_eq!(r["id"], "c");
        assert_eq!(r["result"]["content"][0]["text"], "called list_claims");
        assert!(r["result"].get("structuredContent").is_none(), "2025-03-26 has no structured content");
        ask(json!({"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"rm_rf","arguments":{}}})).await;
        let r: Value = serde_json::from_str(&lines.next_line().await.unwrap().unwrap()).unwrap();
        assert_eq!(r["error"]["code"], -32602);
        // Both halves must go for the server to see end of input.
        drop(lines);
        drop(cw);
        task.await.unwrap().unwrap();
    }

    #[test]
    fn every_schema_is_an_object() {
        for t in tools::TOOLS {
            assert_eq!((t.schema)()["type"], "object", "{}", t.name);
        }
    }
}
