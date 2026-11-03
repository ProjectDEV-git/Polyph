//! One-line summaries of tool calls, built from structured data (never a
//! model) so they are instant and free. Consecutive calls of the same kind
//! are grouped. Each line is a translation key plus arguments; surfaces
//! render it with their localizer (`locales/*/tools.ftl`).

use polyph_proto::messages::{ToolCall, ToolKind};
use serde::Serialize;
use serde_json::{Value, json};

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct SummaryLine {
    pub key: &'static str,
    pub args: Value,
    /// Ids of the calls this line covers, for expanding on click or keypress.
    pub calls: Vec<String>,
}

const MAX_NAMES: usize = 3;

fn file_list(paths: &[String]) -> String {
    let mut names: Vec<&str> = paths.iter().map(|p| p.as_str()).collect();
    names.dedup();
    let more = names.len().saturating_sub(MAX_NAMES);
    let mut s = names.into_iter().take(MAX_NAMES).collect::<Vec<_>>().join(", ");
    if more > 0 {
        s.push_str(", …");
    }
    s
}

fn short_command(c: &str) -> String {
    let c = c.trim();
    if c.chars().count() > 60 { format!("{}…", c.chars().take(59).collect::<String>()) } else { c.to_string() }
}

/// Groups consecutive calls of the same kind (edits to different files stay
/// one line per file, as in Claude Code's own display) and summarizes them.
pub fn summarize(calls: &[ToolCall]) -> Vec<SummaryLine> {
    let mut out: Vec<SummaryLine> = vec![];
    let mut i = 0;
    while i < calls.len() {
        let c = &calls[i];
        let group_kinds = matches!(c.kind, ToolKind::Read | ToolKind::Search | ToolKind::Think | ToolKind::Fetch);
        let mut j = i + 1;
        if group_kinds {
            while j < calls.len() && calls[j].kind == c.kind {
                j += 1;
            }
        }
        let group = &calls[i..j];
        let ids: Vec<String> = group.iter().map(|c| c.id.clone()).collect();
        let line = match c.kind {
            ToolKind::Read => {
                let paths: Vec<String> = group.iter().flat_map(|c| c.paths.clone()).collect();
                SummaryLine {
                    key: "tool-read",
                    args: json!({ "count": paths.len().max(group.len()), "files": file_list(&paths) }),
                    calls: ids,
                }
            }
            ToolKind::Search => {
                let q = c.query.clone().unwrap_or_else(|| c.title.clone());
                let matches: Option<u32> = group.iter().map(|c| c.matches).sum();
                match (group.len(), matches) {
                    (1, Some(m)) => {
                        SummaryLine { key: "tool-search", args: json!({ "query": q, "matches": m }), calls: ids }
                    }
                    (1, None) => SummaryLine { key: "tool-search-plain", args: json!({ "query": q }), calls: ids },
                    (n, _) => SummaryLine { key: "tool-search-many", args: json!({ "count": n }), calls: ids },
                }
            }
            ToolKind::Edit => {
                let file = c.paths.first().cloned().unwrap_or_else(|| c.title.clone());
                SummaryLine {
                    key: "tool-edit",
                    args: json!({ "file": file, "added": c.lines_added.unwrap_or(0), "removed": c.lines_removed.unwrap_or(0) }),
                    calls: ids,
                }
            }
            ToolKind::Delete => {
                SummaryLine { key: "tool-delete", args: json!({ "files": file_list(&c.paths) }), calls: ids }
            }
            ToolKind::Move => {
                SummaryLine { key: "tool-move", args: json!({ "files": file_list(&c.paths) }), calls: ids }
            }
            ToolKind::Execute => {
                let cmd = short_command(c.command.as_deref().unwrap_or(&c.title));
                match (c.tests_passed, c.tests_failed, c.exit_code) {
                    (Some(p), Some(f), _) => SummaryLine {
                        key: "tool-run-tests",
                        args: json!({ "command": cmd, "passed": p, "failed": f }),
                        calls: ids,
                    },
                    (_, _, Some(code)) if code != 0 => SummaryLine {
                        key: "tool-run-failed",
                        args: json!({ "command": cmd, "code": code }),
                        calls: ids,
                    },
                    _ => SummaryLine { key: "tool-run", args: json!({ "command": cmd }), calls: ids },
                }
            }
            ToolKind::Think => SummaryLine { key: "tool-think", args: json!({}), calls: ids },
            ToolKind::Fetch => SummaryLine {
                key: "tool-fetch",
                args: json!({ "count": group.len(), "target": c.title.clone() }),
                calls: ids,
            },
            ToolKind::Mcp => SummaryLine {
                key: "tool-mcp",
                args: json!({ "tool": c.title.trim_start_matches("mcp__").replace("__", " ") }),
                calls: ids,
            },
            ToolKind::Other => SummaryLine { key: "tool-other", args: json!({ "title": c.title.clone() }), calls: ids },
        };
        out.push(line);
        i = j;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use polyph_proto::messages::ToolStatus;

    fn call(id: &str, kind: ToolKind) -> ToolCall {
        ToolCall {
            id: id.into(),
            kind,
            title: String::new(),
            status: ToolStatus::Completed,
            paths: vec![],
            command: None,
            query: None,
            lines_added: None,
            lines_removed: None,
            matches: None,
            tests_passed: None,
            tests_failed: None,
            exit_code: None,
            diff: None,
            output: None,
        }
    }

    #[test]
    fn groups_reads_and_summarizes_runs() {
        let mut r1 = call("1", ToolKind::Read);
        r1.paths = vec!["src/auth.rs".into()];
        let mut r2 = call("2", ToolKind::Read);
        r2.paths = vec!["src/db.rs".into(), "src/lib.rs".into()];
        let mut e = call("3", ToolKind::Edit);
        e.paths = vec!["src/auth.rs".into()];
        e.lines_added = Some(24);
        e.lines_removed = Some(6);
        let mut t = call("4", ToolKind::Execute);
        t.command = Some("cargo test".into());
        t.tests_passed = Some(48);
        t.tests_failed = Some(2);
        let lines = summarize(&[r1, r2, e, t]);
        assert_eq!(lines.len(), 3);
        assert_eq!(lines[0].key, "tool-read");
        assert_eq!(lines[0].args["count"], 3);
        assert_eq!(lines[0].args["files"], "src/auth.rs, src/db.rs, src/lib.rs");
        assert_eq!(lines[0].calls, vec!["1", "2"]);
        assert_eq!(lines[1].args, json!({ "file": "src/auth.rs", "added": 24, "removed": 6 }));
        assert_eq!(lines[2].key, "tool-run-tests");
        assert_eq!(lines[2].args["failed"], 2);
    }
}
