//! A plugin's MCP server for tests: one tool, `greet`, that proves it runs
//! from the copy of its plugin folder the daemon prepared for the agent.

use std::io::{BufRead, Write};

use serde_json::{Value, json};

fn main() {
    let stdin = std::io::stdin();
    let mut out = std::io::stdout();
    for line in stdin.lock().lines() {
        let Ok(line) = line else { break };
        let Ok(req) = serde_json::from_str::<Value>(&line) else { continue };
        let Some(id) = req.get("id").cloned() else { continue };
        let result = match req["method"].as_str().unwrap_or_default() {
            "initialize" => json!({
                "protocolVersion": "2025-03-26",
                "capabilities": { "tools": {} },
                "serverInfo": { "name": "greeter", "version": "0" }
            }),
            "tools/list" => json!({ "tools": [{
                "name": "greet",
                "description": "Says hello.",
                "inputSchema": { "type": "object", "properties": { "who": { "type": "string" } } }
            }] }),
            "tools/call" => {
                let who = req.pointer("/params/arguments/who").and_then(Value::as_str).unwrap_or("you");
                let dir = std::env::var("POLYPH_PLUGIN_DIR").unwrap_or_default();
                let marker = std::fs::read_to_string(std::path::Path::new(&dir).join("marker.txt")).unwrap_or_default();
                json!({ "content": [{ "type": "text", "text": format!("hello {who} from {}", marker.trim()) }] })
            }
            _ => json!({}),
        };
        let _ = writeln!(out, "{}", json!({ "jsonrpc": "2.0", "id": id, "result": result }));
        let _ = out.flush();
    }
}
