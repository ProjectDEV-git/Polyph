//! Merges that keep what both tasks were trying to do.
//!
//! When git reports a conflicted file, [`resolve`] tries, in order:
//!
//! 1. a line-level three-way merge ([`diff3`]);
//! 2. a symbol-level merge for source files: each function, type or method is
//!    merged on its own, so two tasks that changed or added *different*
//!    symbols (the usual "both appended a function" conflict) both keep their
//!    work; imports and other text outside symbols keep both sides' additions;
//! 3. for prose files, keeping both sides' insertions.
//!
//! What is left is a real disagreement inside one symbol. The daemon then
//! hands the file, with conflict markers ([`with_markers`]), to a resolver
//! agent together with both tasks' intent ([`resolver_prompt`]), and checks
//! its answer with [`has_markers`].

pub mod symbols;

use polyph_git::Conflict;
use serde::Serialize;
use similar::{DiffTag, TextDiff};

use symbols::{Lang, top_level};

/// One side's change to the base: base lines `start..end` replaced by `lines`.
#[derive(Debug, Clone, PartialEq)]
struct Hunk<'a> {
    start: usize,
    end: usize,
    lines: Vec<&'a str>,
}

fn split(s: &str) -> Vec<&str> {
    s.split_inclusive('\n').collect()
}

fn hunks<'a>(base: &str, side: &'a str) -> Vec<Hunk<'a>> {
    let side_lines = split(side);
    let diff = TextDiff::from_lines(base, side);
    let mut out: Vec<Hunk<'a>> = vec![];
    for op in diff.ops() {
        if op.tag() == DiffTag::Equal {
            continue;
        }
        let (o, n) = (op.old_range(), op.new_range());
        let lines: Vec<&'a str> = side_lines[n.start.min(side_lines.len())..n.end.min(side_lines.len())].to_vec();
        match out.last_mut() {
            Some(h) if h.end == o.start => {
                h.end = o.end;
                h.lines.extend(lines);
            }
            _ => out.push(Hunk { start: o.start, end: o.end, lines }),
        }
    }
    out
}

/// Result of a three-way merge.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Merge3 {
    Clean(String),
    /// Text with conflict markers, and the number of conflicts.
    Conflicted(String, usize),
}

/// How [`diff3`] treats two sides inserting different lines at the same place.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Insertions {
    /// A conflict, as git sees it.
    Conflict,
    /// Keep both: ours, then the lines of theirs that ours does not have.
    KeepBoth,
}

/// Line-level three-way merge with conflict markers labelled `ours`/`theirs`.
pub fn diff3(base: &str, ours: &str, theirs: &str, labels: (&str, &str), ins: Insertions) -> Merge3 {
    let base_lines = split(base);
    let a = hunks(base, ours);
    let b = hunks(base, theirs);
    let mut out = String::new();
    let mut conflicts = 0;
    let (mut i, mut j, mut pos) = (0usize, 0usize, 0usize);
    let push_base = |out: &mut String, from: usize, to: usize| {
        for l in &base_lines[from.min(base_lines.len())..to.min(base_lines.len())] {
            out.push_str(l);
        }
    };
    let overlaps = |x: &Hunk<'_>, y: &Hunk<'_>| {
        x.start < y.end && y.start < x.end
            || x.start == y.start
            || (x.start == x.end && x.start == y.end && y.start < y.end)
            || (y.start == y.end && y.start == x.end && x.start < x.end)
    };
    while i < a.len() || j < b.len() {
        let next_a = a.get(i);
        let next_b = b.get(j);
        let (take_a, take_b) = match (next_a, next_b) {
            (Some(x), Some(y)) if overlaps(x, y) => (true, true),
            (Some(x), Some(y)) => (x.start <= y.start, x.start > y.start),
            (Some(_), None) => (true, false),
            (None, Some(_)) => (false, true),
            (None, None) => break,
        };
        if take_a && take_b {
            // Grow the conflict region while hunks keep overlapping it.
            let mut start = a[i].start.min(b[j].start);
            let mut end = a[i].end.max(b[j].end);
            let (i0, j0) = (i, j);
            i += 1;
            j += 1;
            loop {
                let grow_a = a.get(i).is_some_and(|h| h.start < end || h.start == start);
                let grow_b = b.get(j).is_some_and(|h| h.start < end || h.start == start);
                if grow_a {
                    end = end.max(a[i].end);
                    i += 1;
                } else if grow_b {
                    end = end.max(b[j].end);
                    j += 1;
                } else {
                    break;
                }
            }
            start = start.min(pos.max(start));
            push_base(&mut out, pos, start);
            let side = |hs: &[Hunk<'_>], from: usize, to: usize| -> String {
                // This side's text for base[start..end].
                let mut s = String::new();
                let mut p = start;
                for h in &hs[from..to] {
                    for l in &base_lines[p.min(base_lines.len())..h.start.min(base_lines.len())] {
                        s.push_str(l);
                    }
                    for l in &h.lines {
                        s.push_str(l);
                    }
                    p = h.end;
                }
                for l in &base_lines[p.min(base_lines.len())..end.min(base_lines.len())] {
                    s.push_str(l);
                }
                s
            };
            let sa = side(&a, i0, i);
            let sb = side(&b, j0, j);
            let only_insertions = start == end;
            if sa == sb {
                out.push_str(&sa);
            } else if only_insertions && ins == Insertions::KeepBoth {
                out.push_str(&sa);
                let have: Vec<&str> = split(&sa);
                for l in split(&sb) {
                    if !have.contains(&l) {
                        out.push_str(l);
                    }
                }
            } else {
                conflicts += 1;
                let nl = |s: &mut String| {
                    if !s.is_empty() && !s.ends_with('\n') {
                        s.push('\n');
                    }
                };
                out.push_str(&format!("<<<<<<< {}\n", labels.0));
                out.push_str(&sa);
                nl(&mut out);
                out.push_str("=======\n");
                out.push_str(&sb);
                nl(&mut out);
                out.push_str(&format!(">>>>>>> {}\n", labels.1));
            }
            pos = end;
        } else {
            let h = if take_a {
                i += 1;
                &a[i - 1]
            } else {
                j += 1;
                &b[j - 1]
            };
            push_base(&mut out, pos, h.start);
            for l in &h.lines {
                out.push_str(l);
            }
            pos = h.end;
        }
    }
    push_base(&mut out, pos, base_lines.len());
    if conflicts == 0 { Merge3::Clean(out) } else { Merge3::Conflicted(out, conflicts) }
}

/// Whether text still has conflict markers.
pub fn has_markers(text: &str) -> bool {
    let mut open = false;
    for l in text.lines() {
        if l.starts_with("<<<<<<< ") || l == "<<<<<<<" {
            open = true;
        } else if open && (l.starts_with(">>>>>>> ") || l == ">>>>>>>") {
            return true;
        }
    }
    false
}

/// A file split into text before the first symbol, one unit per top-level
/// symbol (with the blank lines and comments above it), and text after.
struct Units {
    header: String,
    units: Vec<(String, String)>,
    footer: String,
}

fn units(content: &str, lang: Lang) -> Units {
    let lines = split(content);
    let syms = top_level(content, lang);
    let join = |from: usize, to: usize| -> String { lines[from.min(lines.len())..to.min(lines.len())].concat() };
    let Some(first) = syms.first() else {
        return Units { header: content.to_string(), units: vec![], footer: String::new() };
    };
    // The header ends at the last blank line before the first symbol, so
    // imports stay in the header and the symbol's own comments stay with it.
    let mut header_end = first.start;
    while header_end > 0 && !lines[header_end - 1].trim().is_empty() {
        header_end -= 1;
    }
    if header_end == 0 {
        header_end = first.start;
    }
    let header = join(0, header_end);
    let mut units = vec![];
    let mut seen: Vec<String> = vec![];
    let mut from = header_end;
    for s in &syms {
        let n = seen.iter().filter(|x| **x == s.name).count();
        seen.push(s.name.clone());
        let key = if n == 0 { s.name.clone() } else { format!("{}#{n}", s.name) };
        units.push((key, join(from, s.end + 1)));
        from = s.end + 1;
    }
    Units { header, units, footer: join(from, lines.len()) }
}

/// Merges one unit; None when both sides changed it in ways that collide.
fn merge_unit(base: Option<&str>, ours: Option<&str>, theirs: Option<&str>, ins: Insertions) -> Option<Option<String>> {
    let own = |s: Option<&str>| s.map(str::to_string);
    if ours == theirs {
        return Some(own(ours));
    }
    if ours == base {
        return Some(own(theirs));
    }
    if theirs == base {
        return Some(own(ours));
    }
    match (base, ours, theirs) {
        (Some(b), Some(o), Some(t)) => match diff3(b, o, t, ("ours", "theirs"), ins) {
            Merge3::Clean(s) => Some(Some(s)),
            Merge3::Conflicted(..) => None,
        },
        // Added on both sides with different content, or deleted on one side
        // and changed on the other: a real disagreement.
        _ => None,
    }
}

fn structural(c: &Conflict, lang: Lang) -> Option<String> {
    let empty = String::new();
    let b = units(c.base.as_ref().unwrap_or(&empty), lang);
    let o = units(c.ours.as_ref()?, lang);
    let t = units(c.theirs.as_ref()?, lang);
    let get = |u: &Units, k: &str| u.units.iter().find(|(key, _)| key == k).map(|(_, v)| v.clone());
    let mut out = String::new();
    out.push_str(&merge_unit(Some(&b.header), Some(&o.header), Some(&t.header), Insertions::KeepBoth)??);
    // Ours' order, with theirs' new units placed after their predecessor.
    let mut order: Vec<String> = o.units.iter().map(|(k, _)| k.clone()).collect();
    for (idx, (k, _)) in t.units.iter().enumerate() {
        if order.contains(k) || get(&b, k).is_some() {
            continue;
        }
        let pred = t.units[..idx].iter().rev().find(|(p, _)| order.contains(p)).map(|(p, _)| p.clone());
        let mut at = match pred {
            Some(p) => order.iter().position(|x| *x == p).map(|x| x + 1).unwrap_or(order.len()),
            None => 0,
        };
        // After anything ours added at the same place: the merged task first.
        while at < order.len() && get(&b, &order[at]).is_none() && get(&t, &order[at]).is_none() {
            at += 1;
        }
        order.insert(at, k.clone());
    }
    // Units theirs kept but ours deleted are decided in merge_unit too.
    for (k, _) in &b.units {
        if !order.contains(k) && get(&t, k).is_some() {
            let bt = (get(&b, k), get(&t, k));
            if bt.0 != bt.1 {
                return None;
            }
        }
    }
    for k in &order {
        let merged =
            merge_unit(get(&b, k).as_deref(), get(&o, k).as_deref(), get(&t, k).as_deref(), Insertions::Conflict)?;
        if let Some(text) = merged {
            if !out.is_empty() && !out.ends_with('\n') {
                out.push('\n');
            }
            out.push_str(&text);
        }
    }
    if !out.is_empty() && !out.ends_with('\n') && !o.footer.is_empty() {
        out.push('\n');
    }
    out.push_str(&merge_unit(Some(&b.footer), Some(&o.footer), Some(&t.footer), Insertions::KeepBoth)??);
    (!has_markers(&out)).then_some(out)
}

/// How a conflicted file was settled.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Strategy {
    Lines,
    Symbols,
    KeepBothInsertions,
}

/// Resolves a conflict without an agent, if both sides' changes can be kept.
pub fn resolve(c: &Conflict) -> Option<(String, Strategy)> {
    let (base, ours, theirs) = (c.base.as_deref().unwrap_or(""), c.ours.as_deref()?, c.theirs.as_deref()?);
    if let Merge3::Clean(s) = diff3(base, ours, theirs, ("ours", "theirs"), Insertions::Conflict) {
        return Some((s, Strategy::Lines));
    }
    if let Some(lang) = Lang::from_path(&c.path) {
        return structural(c, lang).map(|s| (s, Strategy::Symbols));
    }
    let ext = c.path.rsplit_once('.').map(|(_, e)| e.to_ascii_lowercase()).unwrap_or_default();
    if ["md", "markdown", "txt", "rst", "adoc"].contains(&ext.as_str())
        && let Merge3::Clean(s) = diff3(base, ours, theirs, ("ours", "theirs"), Insertions::KeepBoth)
    {
        return Some((s, Strategy::KeepBothInsertions));
    }
    None
}

/// The conflicted file with markers, for a resolver agent.
pub fn with_markers(c: &Conflict, ours_label: &str, theirs_label: &str) -> String {
    let base = c.base.as_deref().unwrap_or("");
    match (c.ours.as_deref(), c.theirs.as_deref()) {
        (Some(o), Some(t)) => match diff3(base, o, t, (ours_label, theirs_label), Insertions::Conflict) {
            Merge3::Clean(s) | Merge3::Conflicted(s, _) => s,
        },
        (Some(o), None) => format!("<<<<<<< {ours_label}\n{o}=======\n>>>>>>> {theirs_label} (deleted this file)\n"),
        (None, Some(t)) => format!("<<<<<<< {ours_label} (deleted this file)\n=======\n{t}>>>>>>> {theirs_label}\n"),
        (None, None) => String::new(),
    }
}

/// What a task set out to do, from its board card.
#[derive(Debug, Clone, Default, Serialize)]
pub struct Intent {
    /// The branch, used as the conflict marker label.
    pub label: String,
    pub title: String,
    pub description: String,
    pub acceptance: Vec<String>,
}

fn intent_text(i: &Intent) -> String {
    let mut s = format!("Branch {}: {}", i.label, if i.title.is_empty() { "(no task card)" } else { &i.title });
    if !i.description.trim().is_empty() {
        s.push_str(&format!("\n{}", i.description.trim()));
    }
    if !i.acceptance.is_empty() {
        s.push_str("\nDone when:");
        for a in &i.acceptance {
            s.push_str(&format!("\n- {a}"));
        }
    }
    s
}

/// The prompt for the resolver agent.
pub fn resolver_prompt(files: &[String], ours: &Intent, theirs: &Intent) -> String {
    format!(
        "Two tasks changed the same code and git cannot merge them. Resolve the conflict markers in: {}.\n\n\
Keep what BOTH tasks were trying to do. Do not drop either side's behavior unless the two genuinely contradict; \
if they do, keep the one that satisfies both tasks' acceptance criteria best and explain why in one sentence.\n\n\
Task already merged (ours):\n{}\n\nTask being merged (theirs):\n{}\n\n\
Edit only those files, remove every conflict marker, and do not run git. Reply with a one-line summary of how you combined them.",
        files.join(", "),
        intent_text(ours),
        intent_text(theirs)
    )
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    fn c(path: &str, base: &str, ours: &str, theirs: &str) -> Conflict {
        Conflict { path: path.into(), base: Some(base.into()), ours: Some(ours.into()), theirs: Some(theirs.into()) }
    }

    #[test]
    fn diff3_basics() {
        let base = "a\nb\nc\n";
        assert_eq!(
            diff3(base, "A\nb\nc\n", "a\nb\nC\n", ("o", "t"), Insertions::Conflict),
            Merge3::Clean("A\nb\nC\n".into())
        );
        let r = diff3(base, "a\nX\nc\n", "a\nY\nc\n", ("o", "t"), Insertions::Conflict);
        assert_eq!(r, Merge3::Conflicted("a\n<<<<<<< o\nX\n=======\nY\n>>>>>>> t\nc\n".into(), 1));
        assert!(has_markers(match &r {
            Merge3::Conflicted(s, _) => s,
            _ => "",
        }));
        // Both appended: a conflict for git, kept both on request.
        let r = diff3(base, "a\nb\nc\nours\n", "a\nb\nc\ntheirs\n", ("o", "t"), Insertions::KeepBoth);
        assert_eq!(r, Merge3::Clean("a\nb\nc\nours\ntheirs\n".into()));
        assert_eq!(diff3(base, base, base, ("o", "t"), Insertions::Conflict), Merge3::Clean(base.into()));
    }

    const BASE: &str = "use std::fmt;\n\npub fn charge(n: i64) -> i64 {\n    n\n}\n";

    #[test]
    fn both_tasks_append_a_function() {
        let ours = format!(
            "use std::fmt;\nuse std::time::Duration;\n\npub fn charge(n: i64) -> i64 {{\n    n\n}}\n\npub fn refund(n: i64) -> i64 {{\n    -n\n}}\n"
        );
        let theirs = format!("{BASE}\npub fn invoice(n: i64) -> String {{\n    format!(\"{{n}}\")\n}}\n");
        let conflict = c("src/pay.rs", BASE, &ours, &theirs);
        assert!(matches!(diff3(BASE, &ours, &theirs, ("o", "t"), Insertions::Conflict), Merge3::Conflicted(..)));
        let (merged, how) = resolve(&conflict).unwrap();
        assert_eq!(how, Strategy::Symbols);
        assert_eq!(
            merged,
            "use std::fmt;\nuse std::time::Duration;\n\npub fn charge(n: i64) -> i64 {\n    n\n}\n\npub fn refund(n: i64) -> i64 {\n    -n\n}\n\npub fn invoice(n: i64) -> String {\n    format!(\"{n}\")\n}\n"
        );
    }

    #[test]
    fn different_functions_changed_and_imports_from_both() {
        let base = "import a\n\ndef f():\n    return 1\n\n\ndef g():\n    return 2\n";
        let ours = "import a\nimport b\n\ndef f():\n    return 10\n\n\ndef g():\n    return 2\n";
        let theirs = "import a\nimport c\n\ndef f():\n    return 1\n\n\ndef g():\n    return 20\n";
        let (m, _) = resolve(&c("x.py", base, ours, theirs)).unwrap();
        assert_eq!(m, "import a\nimport b\nimport c\n\ndef f():\n    return 10\n\n\ndef g():\n    return 20\n");
    }

    #[test]
    fn same_function_both_ways_needs_an_agent() {
        let ours = BASE.replace("    n\n", "    n * 2\n");
        let theirs = BASE.replace("    n\n", "    n + 1\n");
        let conflict = c("src/pay.rs", BASE, &ours, &theirs);
        assert!(resolve(&conflict).is_none());
        let m = with_markers(&conflict, "polyph/double", "polyph/increment");
        assert!(m.contains("<<<<<<< polyph/double\n    n * 2\n=======\n    n + 1\n>>>>>>> polyph/increment\n"), "{m}");
        let p = resolver_prompt(
            &["src/pay.rs".into()],
            &Intent { label: "polyph/double".into(), title: "Double charges".into(), ..Default::default() },
            &Intent {
                label: "polyph/increment".into(),
                title: "Add a fee".into(),
                acceptance: vec!["fee is 1".into()],
                ..Default::default()
            },
        );
        assert!(p.contains("Double charges") && p.contains("- fee is 1") && p.contains("src/pay.rs"));
    }

    #[test]
    fn prose_keeps_both_additions() {
        let base = "# Changes\n\n- one\n";
        let (m, how) =
            resolve(&c("CHANGELOG.md", base, "# Changes\n\n- one\n- two\n", "# Changes\n\n- one\n- three\n")).unwrap();
        assert_eq!((m.as_str(), how), ("# Changes\n\n- one\n- two\n- three\n", Strategy::KeepBothInsertions));
        assert!(resolve(&c("config.json", "{}\n", "{\"a\":1}\n", "{\"b\":2}\n")).is_none());
    }
}
