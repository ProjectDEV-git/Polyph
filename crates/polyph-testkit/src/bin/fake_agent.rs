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
//!   { "usage": { "input": 120, "output": 30 } },
//!   { "mcp": { "tool": "claim_files", "args": { "paths": ["src/a.rs"], "reason": "edit" } } },
//!   { "echo_prompt": true }
//! ]] }
//! ```
//! `mcp` steps call the first MCP server from `session/new`, started the way a
//! real agent starts it, and report "mcp <tool>: <text>". The string `$MSG` in
//! their arguments becomes the first `[message id]` in the current prompt.
//! Results of each step are reported back as agent messages
//! ("permission: allowed", "write: denied: ...") so tests can assert on them.

use std::sync::Arc;

use polyph_agents::rpc;
use serde::Deserialize;
use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, Lines};
use tokio::process::{Child, ChildStdin, ChildStdout};
use tokio::sync::Mutex;

/// A minimal MCP client over a child's stdio.
struct Mcp {
    _child: Child,
    stdin: ChildStdin,
    lines: Lines<BufReader<ChildStdout>>,
    next: u64,
}

impl Mcp {
    async fn start(spec: &Value) -> Option<Mcp> {
        let mut cmd = tokio::process::Command::new(spec.get("command")?.as_str()?);
        for a in spec.get("args").and_then(|a| a.as_array()).into_iter().flatten() {
            cmd.arg(a.as_str()?);
        }
        for e in spec.get("env").and_then(|a| a.as_array()).into_iter().flatten() {
            cmd.env(e.get("name")?.as_str()?, e.get("value")?.as_str()?);
        }
        cmd.stdin(std::process::Stdio::piped()).stdout(std::process::Stdio::piped()).kill_on_drop(true);
        let mut child = cmd.spawn().ok()?;
        let stdin = child.stdin.take()?;
        let lines = BufReader::new(child.stdout.take()?).lines();
        let mut m = Mcp { _child: child, stdin, lines, next: 0 };
        m.request("initialize", json!({ "protocolVersion": "2025-06-18", "capabilities": {}, "clientInfo": { "name": "fake", "version": "0" } })).await?;
        m.send(json!({ "jsonrpc": "2.0", "method": "notifications/initialized" })).await?;
        Some(m)
    }

    async fn send(&mut self, v: Value) -> Option<()> {
        self.stdin.write_all(format!("{v}\n").as_bytes()).await.ok()
    }

    async fn request(&mut self, method: &str, params: Value) -> Option<Value> {
        self.next += 1;
        let id = self.next;
        self.send(json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params })).await?;
        loop {
            let line = self.lines.next_line().await.ok()??;
            let v: Value = serde_json::from_str(&line).ok()?;
            if v.get("id") == Some(&json!(id)) {
                return Some(v);
            }
        }
    }
}

/// The first `[hex]` message id in a prompt.
fn first_message_id(prompt: &str) -> Option<String> {
    prompt.split('[').skip(1).find_map(|rest| {
        let id = rest.split(']').next()?;
        (id.len() >= 8 && id.chars().all(|c| c.is_ascii_hexdigit())).then(|| id.to_string())
    })
}

fn substitute(v: &Value, msg: &str) -> Value {
    match v {
        Value::String(s) => Value::String(s.replace("$MSG", msg)),
        Value::Array(a) => Value::Array(a.iter().map(|x| substitute(x, msg)).collect()),
        Value::Object(o) => Value::Object(o.iter().map(|(k, x)| (k.clone(), substitute(x, msg))).collect()),
        other => other.clone(),
    }
}

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
    let mcp_spec = Arc::new(Mutex::new(Value::Null));
    let mcp: Arc<Mutex<Option<Mcp>>> = Arc::new(Mutex::new(None));
    while let Some(req) = incoming.recv().await {
        match req.method.as_str() {
            "initialize" => {
                req.respond(Ok(json!({ "protocolVersion": 1, "agentCapabilities": {}, "authMethods": [] })))
            }
            "session/new" => {
                *cwd.lock().await = req.params.get("cwd").and_then(|c| c.as_str()).unwrap_or_default().to_string();
                *mcp_spec.lock().await = req.params.pointer("/mcpServers/0").cloned().unwrap_or(Value::Null);
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
                let prompt =
                    req.params.pointer("/prompt/0/text").and_then(|t| t.as_str()).unwrap_or_default().to_string();
                let (mcp, spec) = (mcp.clone(), mcp_spec.lock().await.clone());
                tokio::spawn(async move {
                    let usage = run_turn(&peer, &cwd, steps, &prompt, &mcp, &spec).await;
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

async fn run_turn(
    peer: &rpc::Peer,
    cwd: &str,
    steps: Vec<Value>,
    prompt: &str,
    mcp: &Mutex<Option<Mcp>>,
    spec: &Value,
) -> Option<Value> {
    let mut usage = None;
    for (i, step) in steps.iter().enumerate() {
        let id = format!("call-{i}");
        if let Some(m) = step.get("message").and_then(|m| m.as_str()) {
            say(peer, m).await;
        } else if step.get("echo_prompt").is_some() {
            say(peer, &format!("prompt: {}", prompt.replace('\n', " | "))).await;
        } else if let Some(call) = step.get("mcp") {
            let tool = call.get("tool").and_then(|t| t.as_str()).unwrap_or_default().to_string();
            let args =
                substitute(call.get("args").unwrap_or(&json!({})), &first_message_id(prompt).unwrap_or_default());
            let mut guard = mcp.lock().await;
            if guard.is_none() {
                *guard = Mcp::start(spec).await;
            }
            let text = match guard.as_mut() {
                None => "no mcp server".to_string(),
                Some(m) => match m.request("tools/call", json!({ "name": tool, "arguments": args })).await {
                    Some(r) => r
                        .pointer("/result/content/0/text")
                        .and_then(|t| t.as_str())
                        .map(str::to_string)
                        .unwrap_or_else(|| r.to_string()),
                    None => "mcp server closed".to_string(),
                },
            };
            drop(guard);
            say(peer, &format!("mcp {tool}: {}", text.replace('\n', " | "))).await;
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
