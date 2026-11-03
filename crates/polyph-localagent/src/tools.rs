//! The local agent's tools. Reads that leave the worktree, every write and
//! every command go through ACP client requests, so the daemon's claims,
//! approvals and sandbox apply exactly as for other agents.

use std::path::Path;

use polyph_agents::rpc::Peer;
use serde_json::{Value, json};

pub const NAMES: [&str; 7] = ["read", "write", "edit", "list", "glob", "grep", "shell"];

/// JSON schema for native tool calling.
pub fn schema() -> Value {
    let f = |name: &str, desc: &str, props: Value, req: &[&str]| json!({ "type": "function", "function": { "name": name, "description": desc, "parameters": { "type": "object", "properties": props, "required": req } } });
    json!([
        f("read", "Read a text file in the repository.", json!({ "path": { "type": "string" } }), &["path"]),
        f(
            "write",
            "Create or replace a file.",
            json!({ "path": { "type": "string" }, "content": { "type": "string" } }),
            &["path", "content"]
        ),
        f(
            "edit",
            "Replace an exact snippet in a file. `old` must occur exactly once.",
            json!({ "path": { "type": "string" }, "old": { "type": "string" }, "new": { "type": "string" } }),
            &["path", "old", "new"]
        ),
        f("list", "List a directory.", json!({ "path": { "type": "string" } }), &["path"]),
        f(
            "glob",
            "Find files matching a glob such as src/**/*.rs.",
            json!({ "pattern": { "type": "string" } }),
            &["pattern"]
        ),
        f(
            "grep",
            "Search file contents with a regular expression.",
            json!({ "pattern": { "type": "string" }, "path": { "type": "string" } }),
            &["pattern"]
        ),
        f("shell", "Run a shell command in the repository.", json!({ "command": { "type": "string" } }), &["command"]),
    ])
}

/// Checks arguments before running anything. Returns a message for the model when invalid.
pub fn validate(name: &str, raw: &str) -> Result<Value, String> {
    if !NAMES.contains(&name) {
        return Err(format!("Unknown tool \"{name}\". Available tools: {}.", NAMES.join(", ")));
    }
    let v: Value = serde_json::from_str(raw)
        .map_err(|e| format!("The arguments for {name} are not valid JSON ({e}). Send a JSON object."))?;
    let required: &[&str] = match name {
        "read" | "list" => &["path"],
        "write" => &["path", "content"],
        "edit" => &["path", "old", "new"],
        "glob" | "grep" => &["pattern"],
        "shell" => &["command"],
        _ => &[],
    };
    for r in required {
        if !v.get(*r).is_some_and(|x| x.is_string()) {
            return Err(format!("{name} needs a string field \"{r}\"."));
        }
    }
    Ok(v)
}

/// What a tool did, for the transcript.
#[derive(Debug, Default)]
pub struct Outcome {
    pub text: String,
    pub ok: bool,
    pub kind: &'static str,
    pub title: String,
    pub path: Option<String>,
    pub diff: Option<(String, String)>,
    pub terminal: Option<String>,
}

fn s<'a>(v: &'a Value, k: &str) -> &'a str {
    v.get(k).and_then(|x| x.as_str()).unwrap_or_default()
}

async fn read_via(peer: &Peer, session: &str, path: &str) -> Result<String, String> {
    let r: Result<Value, _> = peer.request("fs/read_text_file", &json!({ "sessionId": session, "path": path })).await;
    r.map(|v| v.get("content").and_then(|c| c.as_str()).unwrap_or_default().to_string()).map_err(|e| e.to_string())
}

const MAX_RESULT: usize = 12_000;

fn clip(s: String) -> String {
    if s.len() <= MAX_RESULT {
        return s;
    }
    let mut end = MAX_RESULT;
    while !s.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}\n[output truncated]", &s[..end])
}

pub async fn run(peer: &Peer, session: &str, cwd: &Path, name: &str, args: &Value) -> Outcome {
    match name {
        "read" => {
            let path = s(args, "path").to_string();
            match read_via(peer, session, &path).await {
                Ok(t) => Outcome {
                    text: clip(t),
                    ok: true,
                    kind: "read",
                    title: format!("Read {path}"),
                    path: Some(path),
                    ..Default::default()
                },
                Err(e) => Outcome {
                    text: format!("Error: {e}"),
                    kind: "read",
                    title: format!("Read {path}"),
                    path: Some(path),
                    ..Default::default()
                },
            }
        }
        "write" | "edit" => {
            let path = s(args, "path").to_string();
            let old_file = read_via(peer, session, &path).await.unwrap_or_default();
            let new_file = if name == "write" {
                s(args, "content").to_string()
            } else {
                let (old, new) = (s(args, "old"), s(args, "new"));
                match old_file.matches(old).count() {
                    1 => old_file.replacen(old, new, 1),
                    0 => {
                        return Outcome {
                            text: format!(
                                "Error: the text to replace was not found in {path}. Read the file and copy the snippet exactly."
                            ),
                            kind: "edit",
                            title: format!("Edit {path}"),
                            path: Some(path),
                            ..Default::default()
                        };
                    }
                    n => {
                        return Outcome {
                            text: format!(
                                "Error: the text to replace occurs {n} times in {path}. Include more surrounding lines so it is unique."
                            ),
                            kind: "edit",
                            title: format!("Edit {path}"),
                            path: Some(path),
                            ..Default::default()
                        };
                    }
                }
            };
            let r: Result<Value, _> = peer
                .request("fs/write_text_file", &json!({ "sessionId": session, "path": path, "content": new_file }))
                .await;
            match r {
                Ok(_) => Outcome {
                    text: format!("Wrote {path}."),
                    ok: true,
                    kind: "edit",
                    title: format!("Edit {path}"),
                    path: Some(path),
                    diff: Some((old_file, new_file)),
                    ..Default::default()
                },
                Err(e) => Outcome {
                    text: format!("Error: {e}"),
                    kind: "edit",
                    title: format!("Edit {path}"),
                    path: Some(path),
                    ..Default::default()
                },
            }
        }
        "list" => {
            let p = cwd.join(s(args, "path"));
            let mut names: Vec<String> = std::fs::read_dir(&p)
                .map(|r| {
                    r.flatten()
                        .map(|e| {
                            format!("{}{}", e.file_name().to_string_lossy(), if e.path().is_dir() { "/" } else { "" })
                        })
                        .collect()
                })
                .unwrap_or_default();
            names.sort();
            Outcome {
                text: clip(names.join("\n")),
                ok: true,
                kind: "read",
                title: format!("List {}", s(args, "path")),
                ..Default::default()
            }
        }
        "glob" => {
            let pat = s(args, "pattern");
            let text = match globset::Glob::new(pat) {
                Ok(g) => {
                    let m = g.compile_matcher();
                    let mut hits: Vec<String> = walkdir::WalkDir::new(cwd)
                        .into_iter()
                        .filter_entry(|e| !e.file_name().to_string_lossy().starts_with('.') || e.depth() == 0)
                        .flatten()
                        .filter(|e| e.file_type().is_file())
                        .filter_map(|e| e.path().strip_prefix(cwd).ok().map(|p| p.to_string_lossy().into_owned()))
                        .filter(|p| m.is_match(p))
                        .take(500)
                        .collect();
                    hits.sort();
                    hits.join("\n")
                }
                Err(e) => format!("Error: {e}"),
            };
            Outcome { text: clip(text), ok: true, kind: "search", title: format!("Glob {pat}"), ..Default::default() }
        }
        "grep" => {
            let pat = s(args, "pattern");
            let base = cwd.join(s(args, "path"));
            let text = match regex::Regex::new(pat) {
                Ok(re) => {
                    let mut out = vec![];
                    for e in walkdir::WalkDir::new(&base)
                        .into_iter()
                        .filter_entry(|e| !e.file_name().to_string_lossy().starts_with('.') || e.depth() == 0)
                        .flatten()
                    {
                        if !e.file_type().is_file() {
                            continue;
                        }
                        let Ok(t) = std::fs::read_to_string(e.path()) else { continue };
                        for (n, l) in t.lines().enumerate() {
                            if re.is_match(l) {
                                let rel = e
                                    .path()
                                    .strip_prefix(cwd)
                                    .map(|p| p.to_string_lossy().into_owned())
                                    .unwrap_or_default();
                                out.push(format!("{rel}:{}: {}", n + 1, l.trim()));
                                if out.len() >= 200 {
                                    break;
                                }
                            }
                        }
                    }
                    out.join("\n")
                }
                Err(e) => format!("Error: {e}"),
            };
            Outcome { text: clip(text), ok: true, kind: "search", title: format!("Search {pat}"), ..Default::default() }
        }
        "shell" => {
            let command = s(args, "command").to_string();
            let created: Result<Value, _> = peer
                .request("terminal/create", &json!({ "sessionId": session, "command": "sh", "args": ["-c", command] }))
                .await;
            let tid = match created {
                Ok(v) => v.get("terminalId").and_then(|t| t.as_str()).unwrap_or_default().to_string(),
                Err(e) => {
                    return Outcome {
                        text: format!("Error: {e}"),
                        kind: "execute",
                        title: command,
                        ..Default::default()
                    };
                }
            };
            let exit: Value = peer
                .request("terminal/wait_for_exit", &json!({ "sessionId": session, "terminalId": tid }))
                .await
                .unwrap_or(Value::Null);
            let out: Value = peer
                .request("terminal/output", &json!({ "sessionId": session, "terminalId": tid }))
                .await
                .unwrap_or(Value::Null);
            let code = exit.get("exitCode").and_then(|c| c.as_i64());
            let text = format!(
                "{}\n[exit code {}]",
                out.get("output").and_then(|o| o.as_str()).unwrap_or_default(),
                code.map(|c| c.to_string()).unwrap_or_else(|| "unknown".into())
            );
            let _ =
                peer.request::<_, Value>("terminal/release", &json!({ "sessionId": session, "terminalId": tid })).await;
            Outcome {
                text: clip(text),
                ok: code == Some(0),
                kind: "execute",
                title: command,
                terminal: Some(tid),
                ..Default::default()
            }
        }
        _ => Outcome { text: format!("Unknown tool {name}"), ..Default::default() },
    }
}
