//! A scripted ACP agent for tests: no model, no API keys. The script (JSON,
//! path in `POLYPH_FAKE_SCRIPT`) has one list of steps per prompt:
//!
//! ```json
//! { "turns": [[
//!   { "message": "Looking at auth" },
//!   { "read": "src/auth.rs" },
//!   { "permission": { "kind": "execute", "title": "Run cargo publish", "command": "cargo publish" } },
//!   { "write": { "path": "src/auth.rs", "content": "fn login() {}\n" } },
//!   { "run": { "command": "sh", "args": ["-c", "echo '3 passed; 0 failed'"] } },
//!   { "raw_write": "/etc/escape.txt" },
//!   { "usage": { "input": 120, "output": 30 } }
//! ]] }
//! ```
//! Results of each step are reported back as agent messages
//! ("permission: allowed", "write: denied: ...") so tests can assert on them.

use std::sync::Arc;

use polyph_agents::rpc;
use serde::Deserialize;
use serde_json::{Value, json};
use tokio::sync::Mutex;

#[derive(Deserialize, Default)]
struct Script {
    turns: Vec<Vec<Value>>,
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    // Script from POLYPH_FAKE_SCRIPT, else `.fake-script.json` in the working directory.
    let path = std::env::var("POLYPH_FAKE_SCRIPT").unwrap_or_else(|_| ".fake-script.json".into());
    let script: Script =
        std::fs::read_to_string(path).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default();
    let (peer, mut incoming, _tasks) = rpc::connect(tokio::io::stdin(), tokio::io::stdout());
    let turn = Arc::new(Mutex::new(0usize));
    let cwd = Arc::new(Mutex::new(String::new()));
    while let Some(req) = incoming.recv().await {
        match req.method.as_str() {
            "initialize" => {
                req.respond(Ok(json!({ "protocolVersion": 1, "agentCapabilities": {}, "authMethods": [] })))
            }
            "session/new" => {
                *cwd.lock().await = req.params.get("cwd").and_then(|c| c.as_str()).unwrap_or_default().to_string();
                req.respond(Ok(json!({ "sessionId": "fake-session" })));
            }
            "session/prompt" => {
                let steps = {
                    let mut t = turn.lock().await;
                    let s = script.turns.get(*t).cloned().unwrap_or_default();
                    *t += 1;
                    s
                };
                let (peer, cwd) = (peer.clone(), cwd.lock().await.clone());
                tokio::spawn(async move {
                    let usage = run_turn(&peer, &cwd, steps).await;
                    let mut resp = json!({ "stopReason": "end_turn" });
                    if let Some(u) = usage {
                        resp["usage"] = u;
                    }
                    req.respond(Ok(resp));
                });
            }
            "session/cancel" => {}
            _ => req.respond(Err((-32601, "not supported".into()))),
        }
    }
}

async fn update(peer: &rpc::Peer, u: Value) {
    let _ = peer.notify("session/update", &json!({ "sessionId": "fake-session", "update": u })).await;
}

async fn say(peer: &rpc::Peer, text: &str) {
    update(
        peer,
        json!({ "sessionUpdate": "agent_message_chunk", "content": { "type": "text", "text": format!("{text}\n") } }),
    )
    .await;
}

async fn run_turn(peer: &rpc::Peer, cwd: &str, steps: Vec<Value>) -> Option<Value> {
    let mut usage = None;
    for (i, step) in steps.iter().enumerate() {
        let id = format!("call-{i}");
        if let Some(m) = step.get("message").and_then(|m| m.as_str()) {
            say(peer, m).await;
        } else if let Some(t) = step.get("thought").and_then(|m| m.as_str()) {
            update(peer, json!({ "sessionUpdate": "agent_thought_chunk", "content": { "type": "text", "text": t } }))
                .await;
        } else if let Some(path) = step.get("read").and_then(|p| p.as_str()) {
            update(peer, json!({ "sessionUpdate": "tool_call", "toolCallId": id, "title": format!("Read {path}"), "kind": "read", "status": "in_progress", "locations": [{ "path": path }] })).await;
            let r: Result<Value, _> =
                peer.request("fs/read_text_file", &json!({ "sessionId": "fake-session", "path": path })).await;
            let ok = r.is_ok();
            update(peer, json!({ "sessionUpdate": "tool_call_update", "toolCallId": id, "status": if ok { "completed" } else { "failed" } })).await;
            say(peer, &format!("read: {}", if ok { "ok" } else { "denied" })).await;
        } else if let Some(p) = step.get("permission") {
            let tool = json!({ "toolCallId": id, "title": p.get("title").cloned().unwrap_or(json!("action")), "kind": p.get("kind").cloned().unwrap_or(json!("other")), "rawInput": { "command": p.get("command").cloned().unwrap_or(Value::Null) } });
            let r: Result<Value, _> = peer
                .request(
                    "session/request_permission",
                    &json!({ "sessionId": "fake-session", "toolCall": tool, "options": [{ "optionId": "yes", "name": "Allow", "kind": "allow_once" }, { "optionId": "no", "name": "Reject", "kind": "reject_once" }] }),
                )
                .await;
            let allowed = r
                .ok()
                .and_then(|v| v.pointer("/outcome/optionId").and_then(|o| o.as_str()).map(|o| o == "yes"))
                .unwrap_or(false);
            say(peer, &format!("permission: {}", if allowed { "allowed" } else { "denied" })).await;
        } else if let Some(w) = step.get("write") {
            let path = w.get("path").and_then(|p| p.as_str()).unwrap_or_default().to_string();
            let content = w.get("content").and_then(|c| c.as_str()).unwrap_or_default().to_string();
            let old = std::fs::read_to_string(std::path::Path::new(cwd).join(&path)).unwrap_or_default();
            update(peer, json!({ "sessionUpdate": "tool_call", "toolCallId": id, "title": format!("Edit {path}"), "kind": "edit", "status": "in_progress", "locations": [{ "path": path }] })).await;
            let r: Result<Value, rpc::RpcError> = peer
                .request(
                    "fs/write_text_file",
                    &json!({ "sessionId": "fake-session", "path": path, "content": content }),
                )
                .await;
            match r {
                Ok(_) => {
                    update(peer, json!({ "sessionUpdate": "tool_call_update", "toolCallId": id, "status": "completed", "content": [{ "type": "diff", "path": path, "oldText": old, "newText": content }] })).await;
                    say(peer, "write: ok").await;
                }
                Err(e) => {
                    update(peer, json!({ "sessionUpdate": "tool_call_update", "toolCallId": id, "status": "failed" }))
                        .await;
                    say(peer, &format!("write: denied: {e}")).await;
                }
            }
        } else if let Some(run) = step.get("run") {
            let command = run.get("command").and_then(|c| c.as_str()).unwrap_or("true").to_string();
            let args: Vec<String> = run
                .get("args")
                .and_then(|a| a.as_array())
                .map(|a| a.iter().filter_map(|x| x.as_str().map(str::to_string)).collect())
                .unwrap_or_default();
            update(peer, json!({ "sessionUpdate": "tool_call", "toolCallId": id, "title": command, "kind": "execute", "status": "in_progress", "rawInput": { "command": format!("{command} {}", args.join(" ")) } })).await;
            let created: Result<Value, _> = peer
                .request("terminal/create", &json!({ "sessionId": "fake-session", "command": command, "args": args }))
                .await;
            match created.ok().and_then(|v| v.get("terminalId").and_then(|t| t.as_str()).map(str::to_string)) {
                Some(tid) => {
                    let exit: Value = peer
                        .request("terminal/wait_for_exit", &json!({ "sessionId": "fake-session", "terminalId": tid }))
                        .await
                        .unwrap_or(Value::Null);
                    let out: Value = peer
                        .request("terminal/output", &json!({ "sessionId": "fake-session", "terminalId": tid }))
                        .await
                        .unwrap_or(Value::Null);
                    let text = out.get("output").and_then(|o| o.as_str()).unwrap_or_default().to_string();
                    update(peer, json!({ "sessionUpdate": "tool_call_update", "toolCallId": id, "status": "completed", "content": [{ "type": "terminal", "terminalId": tid }], "rawOutput": { "exitCode": exit.get("exitCode").cloned().unwrap_or(Value::Null), "stdout": text } })).await;
                    say(peer, &format!("run: exit {}", exit.get("exitCode").and_then(|c| c.as_i64()).unwrap_or(-1)))
                        .await;
                }
                None => {
                    update(peer, json!({ "sessionUpdate": "tool_call_update", "toolCallId": id, "status": "failed" }))
                        .await;
                    say(peer, "run: denied").await;
                }
            }
        } else if let Some(path) = step.get("raw_write").and_then(|p| p.as_str()) {
            // Bypasses ACP entirely; only the sandbox can stop this.
            let ok = std::fs::write(path, b"escaped").is_ok();
            say(peer, &format!("raw_write: {}", if ok { "ok" } else { "blocked" })).await;
        } else if let Some(u) = step.get("usage") {
            let i = u.get("input").and_then(|v| v.as_u64()).unwrap_or(0);
            let o = u.get("output").and_then(|v| v.as_u64()).unwrap_or(0);
            usage = Some(json!({ "totalTokens": i + o, "inputTokens": i, "outputTokens": o }));
        }
    }
    usage
}
