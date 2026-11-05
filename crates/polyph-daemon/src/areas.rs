//! Code areas: who looks after which part of the code, in the same form as
//! a CODEOWNERS file, to guide how work is split between people and agents.
//!
//! Read from `.polyph/AREAS` in the repository, or, when there is none, from
//! the repository's CODEOWNERS (`.github/`, the root or `docs/`). Each line
//! is a pattern and its owners: people's names (with or without `@`) or agent
//! labels. A trailing `# text` is a note shown to agents. As in CODEOWNERS,
//! the last line that matches a path wins.
//!
//! ```text
//! # pattern          owners               # note
//! src/billing/       @ann Ann/claude      # money: every change needs a test
//! *.sql              @bo
//! /docs/             @cy
//! ```
//!
//! Areas are advice, not locks: `next_task` prefers tasks in the caller's
//! areas, `get_context` and `claim_files` say whose area a file is in, and
//! the planner suggests owners. Claims still decide who edits what.

use std::path::Path;

use serde::Serialize;

pub const FILE: &str = ".polyph/AREAS";
const FALLBACKS: &[&str] = &[".github/CODEOWNERS", "CODEOWNERS", "docs/CODEOWNERS"];

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Area {
    pub pattern: String,
    pub owners: Vec<String>,
    pub note: Option<String>,
    #[serde(skip)]
    globs: Vec<String>,
}

impl Area {
    pub fn describe(&self) -> String {
        let mut s = format!("area {}", self.pattern);
        if !self.owners.is_empty() {
            s.push_str(&format!(", looked after by {}", self.owners.join(", ")));
        }
        if let Some(n) = &self.note {
            s.push_str(&format!(" ({n})"));
        }
        s
    }

    /// Whether `who` (a person's name or an agent label) is one of the owners.
    /// An agent counts as its owner's: `Ann/claude` is in Ann's areas.
    pub fn owned_by(&self, who: &str) -> bool {
        let who = who.trim_start_matches('@').to_lowercase();
        let person = who.split('/').next().unwrap_or_default().to_string();
        self.owners.iter().any(|o| {
            let o = o.trim_start_matches('@').to_lowercase();
            o == who || (!o.contains('/') && o == person)
        })
    }

    pub fn matches(&self, path: &str) -> bool {
        self.globs.iter().any(|g| glob(g.as_bytes(), path.as_bytes()))
    }
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct Areas {
    pub source: Option<String>,
    pub areas: Vec<Area>,
}

impl Areas {
    /// The area a repository-relative path belongs to: the last that matches.
    pub fn owner_of(&self, path: &str) -> Option<&Area> {
        let path = path.trim_start_matches("./").trim_start_matches('/');
        self.areas.iter().rev().find(|a| a.matches(path))
    }

    pub fn is_empty(&self) -> bool {
        self.areas.is_empty()
    }
}

/// `*` within a segment, `**` across segments, `?` one character.
fn glob(p: &[u8], s: &[u8]) -> bool {
    match p.first() {
        None => s.is_empty(),
        Some(b'*') if p.get(1) == Some(&b'*') => {
            let rest = &p[2..];
            let rest = rest.strip_prefix(b"/").unwrap_or(rest);
            if rest.is_empty() {
                return true;
            }
            (0..=s.len()).any(|i| (i == 0 || s[i - 1] == b'/') && glob(rest, &s[i..])) || glob(rest, s)
        }
        Some(b'*') => (0..=s.len()).take_while(|&i| i == 0 || s[i - 1] != b'/').any(|i| glob(&p[1..], &s[i..])),
        Some(b'?') => !s.is_empty() && s[0] != b'/' && glob(&p[1..], &s[1..]),
        Some(c) => s.first() == Some(c) && glob(&p[1..], &s[1..]),
    }
}

/// What a CODEOWNERS pattern covers, as globs over repository-relative paths.
fn globs_for(pattern: &str) -> Vec<String> {
    let anchored = pattern.starts_with('/');
    let p = pattern.trim_start_matches('/');
    let dir = p.ends_with('/');
    let p = p.trim_end_matches('/');
    if p.is_empty() {
        return vec!["**".into()];
    }
    let base = if anchored || p.contains('/') { p.to_string() } else { format!("**/{p}") };
    if dir { vec![format!("{base}/**")] } else { vec![base.clone(), format!("{base}/**")] }
}

pub fn parse(text: &str, source: Option<String>) -> Areas {
    let mut areas = vec![];
    for line in text.lines() {
        let (rule, note) = match line.split_once(" #") {
            Some((r, n)) => (r, Some(n.trim().to_string()).filter(|n| !n.is_empty())),
            None => (line, None),
        };
        let rule = rule.trim();
        if rule.is_empty() || rule.starts_with('#') || rule.starts_with('[') {
            continue;
        }
        let mut parts = rule.split_whitespace();
        let Some(pattern) = parts.next() else { continue };
        let owners: Vec<String> = parts.map(str::to_string).collect();
        areas.push(Area { pattern: pattern.to_string(), owners, note, globs: globs_for(pattern) });
    }
    Areas { source, areas }
}

/// The areas of the repository at `worktree`.
pub fn load(worktree: &Path) -> Areas {
    for f in std::iter::once(FILE).chain(FALLBACKS.iter().copied()) {
        if let Ok(text) = std::fs::read_to_string(worktree.join(f)) {
            return parse(&text, Some(f.to_string()));
        }
    }
    Areas::default()
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    #[test]
    fn codeowners_patterns() {
        let a = parse(
            "# comment\n*            @everyone\nsrc/billing/ @ann Ann/claude # money: test everything\n*.sql @bo\n/docs/ @cy\nREADME.md @dee\n",
            None,
        );
        let who = |p: &str| a.owner_of(p).unwrap().owners.join(" ");
        assert_eq!(who("src/billing/charge.rs"), "@ann Ann/claude");
        assert_eq!(who("src/billing/deep/x.rs"), "@ann Ann/claude");
        assert_eq!(who("src/billing/schema.sql"), "@bo", "the last match wins");
        assert_eq!(who("db/x.sql"), "@bo");
        assert_eq!(who("docs/a/b.md"), "@cy");
        assert_eq!(who("sub/docs/a.md"), "@everyone", "/docs/ is anchored");
        assert_eq!(who("README.md"), "@dee");
        assert_eq!(who("pkg/README.md"), "@dee", "an unanchored name matches at any depth");
        assert_eq!(who("src/main.rs"), "@everyone");
        let billing = a.owner_of("src/billing/x.rs").unwrap();
        assert_eq!(billing.note.as_deref(), Some("money: test everything"));
        assert!(billing.owned_by("Ann") && billing.owned_by("ann/codex") && billing.owned_by("Ann/claude"));
        assert!(!billing.owned_by("Bo") && !billing.owned_by("Bo/claude"));
        assert!(billing.describe().contains("looked after by @ann, Ann/claude"));
    }

    #[test]
    fn globs() {
        assert!(glob(b"src/*.rs", b"src/a.rs"));
        assert!(!glob(b"src/*.rs", b"src/a/b.rs"));
        assert!(glob(b"src/**/b.rs", b"src/a/c/b.rs"));
        assert!(glob(b"src/**/b.rs", b"src/b.rs"));
        assert!(glob(b"**", b"anything/at/all"));
        assert!(glob(b"a?c", b"abc") && !glob(b"a?c", b"a/c"));
        assert!(load(Path::new("/nonexistent")).is_empty());
    }
}
