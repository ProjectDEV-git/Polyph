//! Shared context for every agent in a room.
//!
//! - The project brief: architecture, conventions and decisions, written by
//!   people and read by every agent. It lives in the room's settings, so it
//!   syncs to everyone; until someone writes one, `.polyph/brief.md` in the
//!   repository stands in for it.
//! - `get_context`: one tool call that hands an agent the brief, its task,
//!   the notes pinned to its files, who holds them and which code area they
//!   belong to, and what was finished lately, cut to fit a size limit.
//! - Search across notes, task cards (with their summaries) and threads.
//! - Recent work: finished tasks with the summary their agent left, so
//!   someone joining late can catch up.

use std::collections::BTreeSet;
use std::path::Path;

use polyph_core::memory::{self, Placement};
use polyph_mcp::ToolOutput;
use polyph_proto::crdt::{Board, CardView};
use polyph_proto::messages::DocName;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::coord::Caller;
use crate::daemon::{ApiError, Shared};
use crate::runtime::{RoomCtx, RoomRuntime, apply};

/// Longest brief accepted, in bytes.
pub const MAX_BRIEF: usize = 32 * 1024;
/// `get_context` sizes, in characters.
pub const DEFAULT_BUDGET: usize = 12_000;
pub const MAX_BUDGET: usize = 60_000;
const MIN_BUDGET: usize = 1_000;
const SETTING: &str = "brief";
/// The brief's stand-in until someone writes one.
pub const BRIEF_FILE: &str = ".polyph/brief.md";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Brief {
    pub text: String,
    /// Who last wrote it (a display name), and when.
    pub by: String,
    pub at: i64,
}

pub fn brief(ctx: &mut RoomCtx) -> Option<Brief> {
    let v = polyph_core::docs::setting(ctx.core.doc(&DocName::Settings), SETTING)?;
    serde_json::from_value(v).ok().filter(|b: &Brief| !b.text.trim().is_empty())
}

/// The room's brief, or the repository's `.polyph/brief.md` when there is none.
pub fn brief_or_file(ctx: &mut RoomCtx, worktree: Option<&Path>) -> Option<(String, String)> {
    if let Some(b) = brief(ctx) {
        return Some((b.text, format!("room brief, last edited by {}", b.by)));
    }
    let text = std::fs::read_to_string(worktree?.join(BRIEF_FILE)).ok()?;
    (!text.trim().is_empty()).then(|| (text, BRIEF_FILE.to_string()))
}

pub async fn set_brief(rt: &RoomRuntime, shared: &Shared, text: &str) -> Result<Brief, ApiError> {
    if text.len() > MAX_BRIEF {
        return Err(ApiError::BadRequest(format!("a brief can be at most {} KB", MAX_BRIEF / 1024)));
    }
    let mut ctx = rt.ctx.lock().await;
    if !ctx.core.role().can_write() {
        return Err(ApiError::Forbidden("viewers cannot change the brief".into()));
    }
    let b = Brief {
        text: polyph_core::redact::redact(text.trim()).into_owned(),
        by: ctx.core.my_profile.display_name.clone(),
        at: shared.clock.now(),
    };
    let v = serde_json::to_value(&b).unwrap_or_default();
    let fx = ctx.core.change_doc(DocName::Settings, |d| polyph_core::docs::set_setting(d, SETTING, &v))?;
    apply(rt, &mut ctx, shared, fx).await;
    drop(ctx);
    crate::feed::log(rt, shared, "brief_updated", None, json!({ "bytes": b.text.len() })).await;
    Ok(b)
}

/// Cuts `s` to at most `max` characters on a line boundary where possible.
fn cut(s: &str, max: usize) -> (String, bool) {
    if s.chars().count() <= max {
        return (s.to_string(), false);
    }
    let mut out: String = s.chars().take(max).collect();
    if let Some(i) = out.rfind('\n')
        && i > max / 2
    {
        out.truncate(i);
    }
    (out, true)
}

/// Sections in priority order, fitted into `budget` characters.
fn fit(sections: Vec<(&str, String)>, budget: usize) -> (String, Vec<String>) {
    let mut out = String::new();
    let mut cut_short = vec![];
    for (name, body) in sections {
        if body.trim().is_empty() {
            continue;
        }
        let head = format!("## {name}\n");
        let used = out.chars().count() + head.chars().count() + 2;
        if used + 40 >= budget {
            cut_short.push(name.to_string());
            continue;
        }
        let (text, was_cut) = cut(&body, budget - used);
        out.push_str(&head);
        out.push_str(text.trim_end());
        if was_cut {
            out.push_str("\n…");
            cut_short.push(name.to_string());
        }
        out.push_str("\n\n");
    }
    (out.trim_end().to_string(), cut_short)
}

fn card_text(c: &CardView) -> String {
    let mut t = format!("[{}] {} ({})", &c.id[..8.min(c.id.len())], c.title, c.column);
    if !c.description.trim().is_empty() {
        t.push_str(&format!("\n{}", c.description.trim()));
    }
    if !c.acceptance.is_empty() {
        t.push_str("\nDone when:");
        for a in &c.acceptance {
            t.push_str(&format!("\n- {a}"));
        }
    }
    if !c.files.is_empty() {
        t.push_str(&format!("\nFiles: {}", c.files.join(", ")));
    }
    if let Some(h) = c.handoff.as_deref().filter(|h| !h.trim().is_empty()) {
        t.push_str(&format!("\nHanded over with: {h}"));
    }
    t
}

/// Finished tasks, newest first, with their summaries.
fn recent_cards(cards: &[CardView], limit: usize) -> Vec<&CardView> {
    let mut done: Vec<&CardView> =
        cards.iter().filter(|c| !c.draft && (c.column == "done" || c.column == "review")).collect();
    done.sort_by(|a, b| b.order.partial_cmp(&a.order).unwrap_or(std::cmp::Ordering::Equal));
    // Cards that say what was done first.
    done.sort_by_key(|c| c.summary.is_none());
    done.truncate(limit);
    done
}

fn recent_text(cards: &[&CardView]) -> String {
    let mut t = String::new();
    for c in cards {
        t.push_str(&format!("- {} ({})", c.title, c.column));
        if let Some(a) = &c.assignee {
            t.push_str(&format!(", by {a}"));
        }
        if let Some(s) = c.summary.as_deref().filter(|s| !s.trim().is_empty()) {
            t.push_str(&format!(": {}", s.trim()));
        }
        t.push('\n');
    }
    t
}

/// The `get_context` tool.
pub async fn get_context(
    h: &Caller,
    rt: &RoomRuntime,
    shared: &Shared,
    args: &Value,
) -> Result<ToolOutput, ToolOutput> {
    #[derive(Deserialize)]
    struct A {
        #[serde(default)]
        paths: Vec<String>,
        max_chars: Option<usize>,
    }
    let a: A = serde_json::from_value(args.clone()).map_err(|e| ToolOutput::err(format!("bad arguments: {e}")))?;
    let budget = a.max_chars.unwrap_or(DEFAULT_BUDGET).clamp(MIN_BUDGET, MAX_BUDGET);
    let now = shared.clock.now();
    let task_id = match h.task {
        Some(t) => Some(t.to_hex()),
        None => crate::coord::assigned_card(rt, &h.agent.label).await,
    };
    let mut ctx = rt.ctx.lock().await;
    let room = ctx.core.name.clone();
    let cards = Board::new(ctx.core.doc(&DocName::Board)).all_cards();
    let task = task_id.as_ref().and_then(|t| cards.iter().find(|c| &c.id == t));
    // Files: asked for, the task's, and the ones this agent holds.
    let mut files: BTreeSet<String> = BTreeSet::new();
    for p in &a.paths {
        let rel = crate::coord::rel_path(&h.worktree, p).map_err(ToolOutput::err)?;
        files.insert(rel.split('#').next().unwrap_or_default().to_string());
    }
    if let Some(t) = task {
        files.extend(t.files.iter().cloned());
    }
    let claims = ctx.core.claims.all();
    files.extend(claims.iter().filter(|c| c.owner.id == h.agent.id).map(|c| c.target.path.clone()));
    files.retain(|f| !f.is_empty());

    let you = {
        let mut t = format!("You are {} in the room \"{room}\", working in {}.", h.agent.label, h.worktree.display());
        if h.external {
            t.push_str(" You run in the person's own checkout: commit with git yourself.");
        }
        t
    };
    let task_text = match task {
        Some(t) => card_text(t),
        None => "No task is assigned to you. Ask a person, or call next_task to take the next ready one.".into(),
    };
    let (brief_text, brief_src) = match brief_or_file(&mut ctx, Some(&h.worktree)) {
        Some((t, src)) => (format!("({src})\n{t}"), Some(src)),
        None => (String::new(), None),
    };
    let all_notes = memory::notes(ctx.core.doc(&DocName::Memory));
    let mut notes_text = String::new();
    let mut seen = BTreeSet::new();
    let mut note_count = 0;
    for f in &files {
        for (n, p) in crate::coord::placed_notes(&all_notes, f, &h.worktree) {
            if !seen.insert(n.id.clone()) {
                continue;
            }
            note_count += 1;
            let at = match p {
                Placement::File => n.path.clone(),
                Placement::Exact(l) | Placement::Moved(l) => format!("{}:{l}", n.path),
                Placement::Lost => format!("{} (its line is gone)", n.path),
            };
            notes_text.push_str(&format!("- {at}: {}\n", n.text));
        }
    }
    let mut held_text = String::new();
    for c in claims.iter().filter(|c| c.owner.id != h.agent.id && c.expires_at > now) {
        let p = &c.target.path;
        if files.iter().any(|f| f == p || f.starts_with(p.as_str()) || p.starts_with(&format!("{f}/"))) {
            held_text.push_str(&format!("- {} is held by {} ({})\n", c.target, c.owner.label, c.reason));
        }
    }
    let areas = crate::areas::load(&h.worktree);
    let mut areas_text = String::new();
    for f in &files {
        if let Some(r) = areas.owner_of(f) {
            areas_text.push_str(&format!("- {f}: {}\n", r.describe()));
        }
    }
    let recent = recent_cards(&cards, 8);
    drop(ctx);
    let sections = vec![
        ("You", you),
        ("Your task", task_text),
        ("Project brief", brief_text),
        ("Notes on your files", notes_text),
        ("Held by others", held_text),
        ("Code areas", areas_text),
        ("Recently finished", recent_text(&recent)),
    ];
    let (text, cut_short) = fit(sections, budget);
    let mut text = text;
    if !cut_short.is_empty() {
        text.push_str(&format!(
            "\n\n(Cut to fit {budget} characters: {}. Ask again with a larger max_chars, or use recall and search.)",
            cut_short.join(", ")
        ));
    }
    Ok(ToolOutput::ok(
        text,
        json!({
            "task": task.map(|t| t.id.clone()),
            "files": files,
            "notes": note_count,
            "brief": brief_src,
            "cut": cut_short,
        }),
    ))
}

#[derive(Debug, Clone, Serialize)]
pub struct Hit {
    /// "note", "card" or "thread".
    pub kind: &'static str,
    pub id: String,
    pub title: String,
    /// The matching text, shortened around the first match.
    pub snippet: String,
    pub score: u32,
    pub at: i64,
}

fn terms(q: &str) -> Vec<String> {
    q.split_whitespace().map(|t| t.to_lowercase()).filter(|t| !t.is_empty()).take(12).collect()
}

/// Score of `title` and `body` for all of `terms`, or `None` when one is missing.
fn score(terms: &[String], title: &str, body: &str) -> Option<u32> {
    let (tl, bl) = (title.to_lowercase(), body.to_lowercase());
    let mut s = 0u32;
    for t in terms {
        let in_title = tl.matches(t.as_str()).count() as u32;
        let in_body = bl.matches(t.as_str()).count() as u32;
        if in_title + in_body == 0 {
            return None;
        }
        s += in_title * 3 + in_body.min(5);
    }
    Some(s)
}

fn snippet(text: &str, terms: &[String]) -> String {
    let lower = text.to_lowercase();
    let at = terms.iter().filter_map(|t| lower.find(t.as_str())).min().unwrap_or(0);
    let chars: Vec<char> = text.chars().collect();
    // Byte offset to char offset (lowercasing can shift lengths; stay safe).
    let ci = text.char_indices().position(|(b, _)| b >= at).unwrap_or(0);
    let start = ci.saturating_sub(60);
    let end = (ci + 140).min(chars.len());
    let mut s: String = chars[start..end].iter().collect::<String>().replace('\n', " ");
    if start > 0 {
        s.insert(0, '…');
    }
    if end < chars.len() {
        s.push('…');
    }
    s
}

/// Searches notes, task cards (title, description, acceptance, summary) and
/// threads (title and posts) for every word of `query`.
pub fn search(ctx: &mut RoomCtx, query: &str, limit: usize) -> Vec<Hit> {
    let terms = terms(query);
    if terms.is_empty() {
        return vec![];
    }
    let mut hits = vec![];
    for n in memory::notes(ctx.core.doc(&DocName::Memory)) {
        if let Some(s) = score(&terms, &n.path, &n.text) {
            hits.push(Hit {
                kind: "note",
                snippet: snippet(&n.text, &terms),
                id: n.id,
                title: n.path,
                score: s,
                at: n.at,
            });
        }
    }
    for c in Board::new(ctx.core.doc(&DocName::Board)).all_cards() {
        let body =
            format!("{}\n{}\n{}", c.description, c.acceptance.join("\n"), c.summary.as_deref().unwrap_or_default());
        if let Some(s) = score(&terms, &c.title, &body) {
            hits.push(Hit {
                kind: "card",
                snippet: snippet(&body, &terms),
                id: c.id,
                title: c.title,
                score: s,
                at: c.created_at,
            });
        }
    }
    let threads = polyph_core::docs::threads(ctx.core.doc(&DocName::Threads));
    let chat = ctx.core.chat();
    for t in threads {
        let posts: Vec<&str> =
            chat.iter().filter(|m| m.thread.as_deref() == Some(t.id.as_str())).map(|m| m.text.as_str()).collect();
        let body = posts.join("\n");
        if let Some(s) = score(&terms, &t.title, &body) {
            hits.push(Hit {
                kind: "thread",
                snippet: snippet(&body, &terms),
                id: t.id,
                title: t.title,
                score: s,
                at: t.created_at,
            });
        }
    }
    hits.sort_by(|a, b| b.score.cmp(&a.score).then(b.at.cmp(&a.at)));
    hits.truncate(limit.clamp(1, 100));
    hits
}

pub fn search_tool(ctx: &mut RoomCtx, args: &Value) -> Result<ToolOutput, ToolOutput> {
    #[derive(Deserialize)]
    struct A {
        query: String,
        limit: Option<usize>,
    }
    let a: A = serde_json::from_value(args.clone()).map_err(|e| ToolOutput::err(format!("bad arguments: {e}")))?;
    let hits = search(ctx, &a.query, a.limit.unwrap_or(15));
    if hits.is_empty() {
        return Ok(ToolOutput::ok(format!("Nothing matches \"{}\".", a.query.trim()), json!({ "hits": [] })));
    }
    let mut text = format!("Matches for \"{}\":", a.query.trim());
    for h in &hits {
        text.push_str(&format!("\n- {} [{}] {}: {}", h.kind, &h.id[..8.min(h.id.len())], h.title, h.snippet));
    }
    Ok(ToolOutput::ok(text, json!({ "hits": hits })))
}

/// Finished tasks with their summaries, newest first.
pub fn recent(ctx: &mut RoomCtx, limit: usize) -> Vec<Value> {
    let cards = Board::new(ctx.core.doc(&DocName::Board)).all_cards();
    recent_cards(&cards, limit.clamp(1, 100))
        .into_iter()
        .map(|c| json!({ "id": c.id, "title": c.title, "column": c.column, "assignee": c.assignee, "summary": c.summary, "branch": c.branch }))
        .collect()
}

pub fn recent_tool(ctx: &mut RoomCtx, args: &Value) -> ToolOutput {
    let limit = args.get("limit").and_then(|l| l.as_u64()).unwrap_or(15) as usize;
    let cards = Board::new(ctx.core.doc(&DocName::Board)).all_cards();
    let recent = recent_cards(&cards, limit.clamp(1, 100));
    if recent.is_empty() {
        return ToolOutput::ok("No task has been finished yet.", json!({ "tasks": [] }));
    }
    let open: Vec<&CardView> = cards.iter().filter(|c| c.column == "in_progress" || c.column == "claimed").collect();
    let mut text = format!("Recently finished:\n{}", recent_text(&recent));
    if !open.is_empty() {
        text.push_str("In progress:\n");
        for c in open {
            text.push_str(&format!(
                "- {}{}\n",
                c.title,
                c.assignee.as_ref().map(|a| format!(", {a}")).unwrap_or_default()
            ));
        }
    }
    ToolOutput::ok(
        text.trim_end().to_string(),
        json!({ "tasks": recent.iter().map(|c| c.id.clone()).collect::<Vec<_>>() }),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fitting_keeps_priority_order_and_says_what_was_cut() {
        let long = "line\n".repeat(400);
        let (text, cut) = fit(vec![("A", "first".into()), ("B", long.clone()), ("C", "third".into())], 1_000);
        assert!(text.starts_with("## A\nfirst"));
        assert!(text.chars().count() <= 1_000);
        assert_eq!(cut, vec!["B".to_string(), "C".to_string()]);
        let (text, cut) = fit(vec![("A", "x".into()), ("Empty", " ".into())], 1_000);
        assert_eq!(text, "## A\nx");
        assert!(cut.is_empty());
    }

    #[test]
    fn scoring_needs_every_word() {
        let t = terms("Retry  LOGIC");
        assert_eq!(score(&t, "Retry logic for payments", "").unwrap(), 6);
        assert!(score(&t, "Retry", "nothing else").is_none());
        assert_eq!(score(&t, "x", "retry the logic").unwrap(), 2);
        assert!(snippet(&format!("{}needle{}", "a".repeat(200), "b".repeat(200)), &terms("needle")).contains("needle"));
        assert_eq!(snippet("ไทย needle", &terms("needle")), "ไทย needle");
    }
}
