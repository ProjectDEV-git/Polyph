//! `polyph hook pre-edit`: run by editor hooks (the Claude Code plugin in
//! `plugins/claude-code`) before an agent edits a file. It reads the hook's
//! JSON on standard input, asks the daemon whether another agent holds the
//! file (`claims.check`), and blocks the edit when one does: exit code 2
//! with the reason on standard error, which Claude Code hands to the model.
//! Free files are claimed for the editing agent's connection.
//!
//! It never stands in the way when Polyph itself cannot answer: no daemon,
//! no room for the folder, or input it does not understand all allow the edit.
//! The message is for the model, so it is in English like the MCP tools'.

use serde_json::{Value, json};

use crate::client::Api;
use crate::{Ctx, call};

/// Exit code that tells Claude Code to block the tool call.
pub const BLOCK: i32 = 2;

/// Paths a tool call is about to write, from a hook's `tool_input`.
pub fn paths(input: &Value) -> Vec<String> {
    let t = &input["tool_input"];
    let mut v: Vec<String> = ["file_path", "notebook_path", "path"]
        .iter()
        .filter_map(|k| t.get(*k).and_then(|p| p.as_str()).map(str::to_string))
        .collect();
    // Tools that edit several files at once.
    if let Some(edits) = t.get("edits").and_then(|e| e.as_array()) {
        v.extend(edits.iter().filter_map(|e| e.get("file_path").and_then(|p| p.as_str()).map(str::to_string)));
    }
    v.sort();
    v.dedup();
    v
}

/// The message for the model when an edit is blocked.
pub fn message(blocked: &[Value]) -> String {
    let parts: Vec<String> = blocked
        .iter()
        .map(|b| {
            format!(
                "{} is claimed by {} ({})",
                b["path"].as_str().unwrap_or_default(),
                b["holder"].as_str().unwrap_or_default(),
                b["reason"].as_str().unwrap_or_default()
            )
        })
        .collect();
    format!(
        "Polyph: not edited, because {}. Ask the holder with the ask_agent tool, wait for them to release it, or work on something else.",
        parts.join("; ")
    )
}

/// Runs the hook; returns the process exit code.
pub async fn pre_edit(ctx: &Ctx, client: &str) -> i32 {
    let mut raw = String::new();
    if std::io::Read::read_to_string(&mut std::io::stdin(), &mut raw).is_err() {
        return 0;
    }
    let Ok(input) = serde_json::from_str::<Value>(&raw) else { return 0 };
    let files = paths(&input);
    if files.is_empty() {
        return 0;
    }
    let cwd = input["cwd"]
        .as_str()
        .map(std::path::PathBuf::from)
        .or_else(|| std::env::current_dir().ok())
        .unwrap_or_default();
    // Never start the daemon from a hook: no daemon means no claims to respect.
    let Ok(mut api) = Api::connect(&ctx.paths, false).await else { return 0 };
    let r =
        call(&mut api, "claims.check", json!({ "cwd": cwd, "paths": files, "client": client, "claim": true })).await;
    match r {
        Ok(v) if v["allowed"] == false => {
            let blocked = v["blocked"].as_array().cloned().unwrap_or_default();
            eprintln!("{}", message(&blocked));
            BLOCK
        }
        _ => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_claude_code_tool_inputs() {
        let edit = json!({ "tool_name": "Edit", "tool_input": { "file_path": "/r/src/a.rs", "old_string": "a", "new_string": "b" } });
        assert_eq!(paths(&edit), vec!["/r/src/a.rs"]);
        let nb = json!({ "tool_name": "NotebookEdit", "tool_input": { "notebook_path": "/r/n.ipynb" } });
        assert_eq!(paths(&nb), vec!["/r/n.ipynb"]);
        let multi =
            json!({ "tool_input": { "edits": [{ "file_path": "b" }, { "file_path": "a" }, { "file_path": "a" }] } });
        assert_eq!(paths(&multi), vec!["a", "b"]);
        assert!(paths(&json!({ "tool_input": { "command": "ls" } })).is_empty());
        let m = message(&[json!({ "path": "src/a.rs", "holder": "Bo/codex", "reason": "refactor" })]);
        assert!(m.contains("src/a.rs is claimed by Bo/codex (refactor)") && m.contains("ask_agent"));
    }
}
