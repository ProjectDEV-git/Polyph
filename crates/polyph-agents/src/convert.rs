//! Turns ACP session updates into Polyph transcript events, keeping per-call
//! state so `tool_call_update`s merge into one structured record.

use std::collections::HashMap;

use polyph_proto::messages::{AgentEventKind, ToolCall, ToolKind, ToolStatus, Usage};
use serde_json::Value;

use crate::wire::{ContentBlock, SessionUpdate, ToolCallContent, ToolCallFields};

/// Output kept per tool call in shared transcripts.
pub const MAX_OUTPUT: usize = 4000;

pub fn tool_kind(kind: Option<&str>, title: &str) -> ToolKind {
    if title.starts_with("mcp__") {
        return ToolKind::Mcp;
    }
    match kind.unwrap_or("other") {
        "read" => ToolKind::Read,
        "edit" => ToolKind::Edit,
        "delete" => ToolKind::Delete,
        "move" => ToolKind::Move,
        "search" => ToolKind::Search,
        "execute" => ToolKind::Execute,
        "think" => ToolKind::Think,
        "fetch" => ToolKind::Fetch,
        _ => ToolKind::Other,
    }
}

fn status(s: Option<&str>) -> Option<ToolStatus> {
    Some(match s? {
        "pending" => ToolStatus::Pending,
        "in_progress" => ToolStatus::Running,
        "completed" => ToolStatus::Completed,
        "failed" => ToolStatus::Failed,
        _ => return None,
    })
}

/// Counts added and removed lines and renders a unified diff.
pub fn diff_stats(path: &str, old: &str, new: &str) -> (u32, u32, String) {
    let d = similar::TextDiff::from_lines(old, new);
    let (mut add, mut del) = (0u32, 0u32);
    for c in d.iter_all_changes() {
        match c.tag() {
            similar::ChangeTag::Insert => add += 1,
            similar::ChangeTag::Delete => del += 1,
            similar::ChangeTag::Equal => {}
        }
    }
    let unified = d.unified_diff().context_radius(3).header(&format!("a/{path}"), &format!("b/{path}")).to_string();
    (add, del, unified)
}

/// Finds "N passed" / "N failed" in common test runner output (cargo, jest,
/// vitest, pytest, go). Returns None when the output does not look like tests.
pub fn parse_test_counts(out: &str) -> Option<(u32, u32)> {
    let mut passed = None;
    let mut failed = None;
    for line in out.lines().rev().take(200) {
        let words: Vec<&str> = line
            .split(|c: char| c.is_whitespace() || c == ',' || c == ';' || c == '(' || c == ')')
            .filter(|w| !w.is_empty())
            .collect();
        for w in words.windows(2) {
            if let Ok(n) = w[0].parse::<u32>() {
                let label = w[1].trim_end_matches(['.', ':']);
                if passed.is_none() && (label == "passed" || label == "passing") {
                    passed = Some(n);
                }
                if failed.is_none() && (label == "failed" || label == "failing") {
                    failed = Some(n);
                }
            }
        }
        if passed.is_some() && failed.is_some() {
            break;
        }
    }
    match (passed, failed) {
        (None, None) => None,
        (p, f) => Some((p.unwrap_or(0), f.unwrap_or(0))),
    }
}

fn str_field(v: &Option<Value>, keys: &[&str]) -> Option<String> {
    let v = v.as_ref()?;
    for k in keys {
        match v.get(*k) {
            Some(Value::String(s)) => return Some(s.clone()),
            Some(Value::Array(a)) => return Some(a.iter().filter_map(|x| x.as_str()).collect::<Vec<_>>().join(" ")),
            _ => {}
        }
    }
    None
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        return s.to_string();
    }
    let mut end = max;
    while !s.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}…", &s[..end])
}

#[derive(Debug, Default)]
pub struct Converter {
    calls: HashMap<String, ToolCall>,
    /// Terminal output known to the client, by terminal id (for execute calls).
    pub terminal_output: HashMap<String, (String, Option<i32>)>,
    message: String,
    thought: String,
}

impl Converter {
    fn merge(&mut self, f: &ToolCallFields) -> ToolCall {
        let entry = self.calls.entry(f.tool_call_id.clone()).or_insert_with(|| ToolCall {
            id: f.tool_call_id.clone(),
            kind: ToolKind::Other,
            title: String::new(),
            status: ToolStatus::Pending,
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
        });
        if let Some(t) = &f.title {
            entry.title = t.clone();
        }
        // Updates often carry only a new title; keep the kind unless one is given.
        if f.kind.is_some() || entry.title.starts_with("mcp__") {
            entry.kind = tool_kind(f.kind.as_deref(), &entry.title);
        }
        if let Some(s) = status(f.status.as_deref()) {
            entry.status = s;
        }
        if let Some(locs) = &f.locations {
            for l in locs {
                if !entry.paths.contains(&l.path) {
                    entry.paths.push(l.path.clone());
                }
            }
        }
        if let Some(cmd) = str_field(&f.raw_input, &["command", "cmd"]) {
            entry.command = Some(cmd);
        }
        if let Some(q) = str_field(&f.raw_input, &["pattern", "query", "regex"]) {
            entry.query = Some(q);
        }
        if let Some(p) = str_field(&f.raw_input, &["file_path", "path", "filePath"])
            && !entry.paths.contains(&p)
        {
            entry.paths.push(p);
        }
        if let Some(Value::Number(n)) =
            f.raw_output.as_ref().and_then(|o| o.get("numMatches").or_else(|| o.get("matches")))
        {
            entry.matches = n.as_u64().map(|v| v as u32);
        }
        if let Some(code) = f
            .raw_output
            .as_ref()
            .and_then(|o| o.get("exitCode").or_else(|| o.get("exit_code")))
            .and_then(|c| c.as_i64())
        {
            entry.exit_code = Some(code as i32);
        }
        let mut output = None;
        for c in f.content.iter().flatten() {
            match c {
                ToolCallContent::Diff { path, old_text, new_text } => {
                    let (a, d, u) = diff_stats(path, old_text.as_deref().unwrap_or(""), new_text);
                    entry.lines_added = Some(entry.lines_added.unwrap_or(0) + a);
                    entry.lines_removed = Some(entry.lines_removed.unwrap_or(0) + d);
                    entry.diff = Some(match entry.diff.take() {
                        Some(prev) => prev + &u,
                        None => u,
                    });
                    if !entry.paths.contains(path) {
                        entry.paths.push(path.clone());
                    }
                }
                ToolCallContent::Content { content: ContentBlock::Text { text } } => output = Some(text.clone()),
                ToolCallContent::Terminal { terminal_id } => {
                    if let Some((out, code)) = self.terminal_output.get(terminal_id) {
                        output = Some(out.clone());
                        if code.is_some() {
                            entry.exit_code = *code;
                        }
                    }
                }
                _ => {}
            }
        }
        if let Some(raw) =
            f.raw_output.as_ref().and_then(|o| o.get("stdout").or_else(|| o.get("output"))).and_then(|s| s.as_str())
        {
            output = Some(raw.to_string());
        }
        if let Some(out) = output {
            if entry.kind == ToolKind::Execute
                && let Some((p, fl)) = parse_test_counts(&out)
            {
                entry.tests_passed = Some(p);
                entry.tests_failed = Some(fl);
            }
            entry.output = Some(truncate(&out, MAX_OUTPUT));
        }
        entry.clone()
    }

    /// Converts one update. Message and thought chunks are buffered until
    /// [`flush`](Self::flush) so the transcript gets whole paragraphs.
    pub fn update(&mut self, u: &SessionUpdate) -> Vec<AgentEventKind> {
        match u {
            SessionUpdate::AgentMessageChunk { content } => {
                if let Some(t) = content.text() {
                    self.message.push_str(t);
                }
                vec![]
            }
            SessionUpdate::AgentThoughtChunk { content } => {
                if let Some(t) = content.text() {
                    self.thought.push_str(t);
                }
                vec![]
            }
            SessionUpdate::ToolCall(f) | SessionUpdate::ToolCallUpdate(f) => {
                let mut out = self.flush();
                let call = self.merge(f);
                out.push(AgentEventKind::Tool(Box::new(call)));
                out
            }
            SessionUpdate::UsageUpdate { cost, .. } => {
                let _ = cost;
                vec![]
            }
            _ => vec![],
        }
    }

    pub fn flush(&mut self) -> Vec<AgentEventKind> {
        let mut out = vec![];
        if !self.thought.trim().is_empty() {
            out.push(AgentEventKind::Thought { text: std::mem::take(&mut self.thought) });
        }
        self.thought.clear();
        if !self.message.trim().is_empty() {
            out.push(AgentEventKind::Message { text: std::mem::take(&mut self.message) });
        }
        self.message.clear();
        out
    }

    pub fn usage(u: &crate::wire::TokenUsage, model: Option<String>, subscription: bool) -> Usage {
        Usage {
            input_tokens: Some(u.input_tokens),
            output_tokens: Some(u.output_tokens),
            cache_read_tokens: u.cached_read_tokens,
            cache_write_tokens: u.cached_write_tokens,
            model,
            subscription,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wire::Location;

    #[test]
    fn test_counts() {
        assert_eq!(parse_test_counts("test result: FAILED. 48 passed; 2 failed; 0 ignored"), Some((48, 2)));
        assert_eq!(parse_test_counts("Tests:       2 failed, 48 passed, 50 total"), Some((48, 2)));
        assert_eq!(parse_test_counts("===== 3 failed, 10 passed in 1.2s ====="), Some((10, 3)));
        assert_eq!(parse_test_counts("Compiling foo"), None);
    }

    #[test]
    fn merges_tool_call_updates() {
        let mut c = Converter::default();
        let start = ToolCallFields {
            tool_call_id: "t1".into(),
            title: Some("Edit src/auth.rs".into()),
            kind: Some("edit".into()),
            status: Some("pending".into()),
            locations: Some(vec![Location { path: "src/auth.rs".into(), line: Some(3) }]),
            ..Default::default()
        };
        c.update(&SessionUpdate::ToolCall(start));
        let done = ToolCallFields {
            tool_call_id: "t1".into(),
            status: Some("completed".into()),
            content: Some(vec![ToolCallContent::Diff {
                path: "src/auth.rs".into(),
                old_text: Some("a\nb\n".into()),
                new_text: "a\nc\nd\n".into(),
            }]),
            ..Default::default()
        };
        let ev = c.update(&SessionUpdate::ToolCallUpdate(done));
        let AgentEventKind::Tool(t) = &ev[0] else { panic!("expected tool") };
        assert_eq!(t.kind, ToolKind::Edit);
        assert_eq!(t.status, ToolStatus::Completed);
        assert_eq!((t.lines_added, t.lines_removed), (Some(2), Some(1)));
        assert!(t.diff.as_deref().unwrap().contains("+c"));
    }
}
