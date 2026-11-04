//! Memory notes: short facts pinned to a file or a line of code, shared
//! through the `Memory` document. A note keeps the text of its line and the
//! lines around it, so it can be found again after the file changes.
//!
//! Layout: `notes` Y.Map note id -> JSON string of [`MemoryNote`].

use serde::{Deserialize, Serialize};
use yrs::{Any, Doc, Map, Out, Transact};

use crate::claims::normalize_path;

pub const NOTES: &str = "notes";
/// Longest note text accepted, in bytes.
pub const MAX_NOTE: usize = 4000;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Anchor {
    /// The pinned line, trimmed.
    pub line: String,
    /// The nearest non-blank lines above and below, trimmed.
    pub before: Option<String>,
    pub after: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemoryNote {
    pub id: String,
    /// Repository-relative path, normalized. A trailing `/` pins a directory.
    pub path: String,
    /// 1-based line when the note was written; None pins the whole file.
    pub line: Option<u32>,
    pub anchor: Option<Anchor>,
    pub text: String,
    /// Member id (hex) of the author, or of the agent's owner.
    pub author: String,
    /// Agent label when an agent wrote it.
    pub agent: Option<String>,
    pub at: i64,
}

/// Where a note points in the current version of its file.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "status", content = "line", rename_all = "snake_case")]
pub enum Placement {
    /// Pinned to the whole file or directory.
    File,
    /// The pinned line is where it was.
    Exact(u32),
    /// The pinned line moved to this line.
    Moved(u32),
    /// The pinned line is gone or the file is missing; the note may be stale.
    Lost,
}

pub fn put_note(doc: &Doc, n: &MemoryNote) {
    let map = doc.get_or_insert_map(NOTES);
    let json = serde_json::to_string(n).unwrap_or_default();
    let mut txn = doc.transact_mut();
    map.insert(&mut txn, n.id.as_str(), json);
}

pub fn remove_note(doc: &Doc, id: &str) -> bool {
    let map = doc.get_or_insert_map(NOTES);
    let mut txn = doc.transact_mut();
    map.remove(&mut txn, id).is_some()
}

pub fn notes(doc: &Doc) -> Vec<MemoryNote> {
    let map = doc.get_or_insert_map(NOTES);
    let txn = doc.transact();
    let mut v: Vec<MemoryNote> = map
        .iter(&txn)
        .filter_map(|(_, o)| match o {
            Out::Any(Any::String(s)) => serde_json::from_str(&s).ok(),
            _ => None,
        })
        .collect();
    v.sort_by(|a, b| a.path.cmp(&b.path).then(a.line.cmp(&b.line)).then(a.at.cmp(&b.at)));
    v
}

/// Notes that apply to `path`: pinned to it, or to a directory containing it.
/// With an empty path, every note.
pub fn notes_for(all: &[MemoryNote], path: &str) -> Vec<MemoryNote> {
    let p = normalize_path(path);
    if p.is_empty() {
        return all.to_vec();
    }
    all.iter()
        .filter(|n| {
            n.path == p
                || (n.path.ends_with('/') && p.starts_with(&n.path))
                || (p.ends_with('/') && n.path.starts_with(&p))
        })
        .cloned()
        .collect()
}

fn neighbor(lines: &[&str], from: usize, up: bool) -> Option<String> {
    let mut i = from;
    loop {
        if up {
            if i == 0 {
                return None;
            }
            i -= 1;
        } else {
            i += 1;
            if i >= lines.len() {
                return None;
            }
        }
        let t = lines[i].trim();
        if !t.is_empty() {
            return Some(t.to_string());
        }
    }
}

/// The anchor for 1-based `line` in `content`, if the line exists.
pub fn make_anchor(content: &str, line: u32) -> Option<Anchor> {
    let lines: Vec<&str> = content.lines().collect();
    let i = (line as usize).checked_sub(1)?;
    let l = lines.get(i)?;
    Some(Anchor { line: l.trim().to_string(), before: neighbor(&lines, i, true), after: neighbor(&lines, i, false) })
}

/// Finds a note's line in the current `content` (None when the file is missing).
pub fn place(note: &MemoryNote, content: Option<&str>) -> Placement {
    let (Some(line), Some(anchor)) = (note.line, note.anchor.as_ref()) else { return Placement::File };
    let Some(content) = content else { return Placement::Lost };
    let lines: Vec<&str> = content.lines().collect();
    let score = |i: usize| -> u32 {
        let mut s = 0;
        if lines[i].trim() == anchor.line {
            s += 4;
        }
        if neighbor(&lines, i, true) == anchor.before {
            s += 1;
        }
        if neighbor(&lines, i, false) == anchor.after {
            s += 1;
        }
        s
    };
    let orig = (line as usize).saturating_sub(1);
    if orig < lines.len() && lines[orig].trim() == anchor.line && (!anchor.line.is_empty() || score(orig) >= 5) {
        return Placement::Exact(line);
    }
    // The line itself must match; neighbors and distance break ties.
    let best = (0..lines.len())
        .filter(|&i| lines[i].trim() == anchor.line)
        .map(|i| (score(i), std::cmp::Reverse(i.abs_diff(orig)), i))
        .filter(|(s, _, _)| !anchor.line.is_empty() || *s >= 5)
        .max();
    match best {
        Some((_, _, i)) if i == orig => Placement::Exact(line),
        Some((_, _, i)) => Placement::Moved(i as u32 + 1),
        None => Placement::Lost,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn note(path: &str, content: &str, line: u32) -> MemoryNote {
        MemoryNote {
            id: "n1".into(),
            path: normalize_path(path),
            line: Some(line),
            anchor: make_anchor(content, line),
            text: "retries must stay idempotent".into(),
            author: "aa".into(),
            agent: None,
            at: 1,
        }
    }

    const V1: &str = "fn a() {}\n\nfn charge() {\n    retry(3);\n}\n";

    #[test]
    fn exact_moved_lost() {
        let n = note("src/pay.rs", V1, 4);
        assert_eq!(n.anchor.as_ref().unwrap().line, "retry(3);");
        assert_eq!(place(&n, Some(V1)), Placement::Exact(4));
        let v2 = format!("// header\n// more\n{V1}");
        assert_eq!(place(&n, Some(&v2)), Placement::Moved(6));
        assert_eq!(place(&n, Some("fn a() {}\n")), Placement::Lost);
        assert_eq!(place(&n, None), Placement::Lost);
    }

    #[test]
    fn neighbors_pick_between_duplicates() {
        let v = "fn a() {\n    retry(3);\n}\n\nfn charge() {\n    retry(3);\n}\n";
        let n = note("p.rs", v, 6);
        let shifted = format!("\n\n{v}");
        assert_eq!(place(&n, Some(&shifted)), Placement::Moved(8));
    }

    #[test]
    fn stored_in_the_memory_doc() {
        let d = Doc::new();
        let n = note("src/pay.rs", V1, 4);
        put_note(&d, &n);
        let mut dir = n.clone();
        dir.id = "n2".into();
        dir.path = "src/".into();
        dir.line = None;
        dir.anchor = None;
        put_note(&d, &dir);
        let all = notes(&d);
        assert_eq!(all.len(), 2);
        assert_eq!(notes_for(&all, "src/pay.rs").len(), 2);
        assert_eq!(notes_for(&all, "lib/x.rs").len(), 0);
        assert_eq!(place(&all[0], None), Placement::File);
        assert!(remove_note(&d, "n1"));
        assert_eq!(notes(&d).len(), 1);
    }
}
