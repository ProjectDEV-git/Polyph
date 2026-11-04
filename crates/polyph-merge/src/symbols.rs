//! Finds the top-level symbols of a source file (and the members of classes,
//! impls and traits) with their line ranges. Heuristic, without a parser: good
//! enough to tell which functions a change touched and to merge changes to
//! different functions, and it never panics on odd input.

use serde::Serialize;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Lang {
    /// Rust, C, C++, C#, Go, Java, Kotlin, Swift, JavaScript, TypeScript, PHP, Scala.
    Braces,
    /// Python.
    Indent,
}

impl Lang {
    pub fn from_path(path: &str) -> Option<Lang> {
        let ext = path.rsplit_once('.').map(|(_, e)| e.to_ascii_lowercase())?;
        match ext.as_str() {
            "rs" | "c" | "h" | "cc" | "cpp" | "hpp" | "cs" | "go" | "java" | "kt" | "kts" | "swift" | "js" | "jsx"
            | "mjs" | "cjs" | "ts" | "tsx" | "mts" | "php" | "scala" | "dart" => Some(Lang::Braces),
            "py" | "pyi" => Some(Lang::Indent),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Symbol {
    /// Name, qualified by its container (`Account::deposit`).
    pub name: String,
    /// First line (0-based), including doc comments and attributes above it.
    pub start: usize,
    /// Last line (0-based, inclusive).
    pub end: usize,
}

const BRACE_KEYWORDS: &[&str] = &[
    "fn",
    "struct",
    "enum",
    "trait",
    "impl",
    "mod",
    "union",
    "type",
    "const",
    "static",
    "macro_rules!",
    "func",
    "function",
    "class",
    "interface",
    "namespace",
    "object",
    "def",
    "record",
];
const MODIFIERS: &[&str] = &[
    "pub",
    "pub(crate)",
    "pub(super)",
    "async",
    "unsafe",
    "extern",
    "export",
    "default",
    "public",
    "private",
    "protected",
    "static",
    "final",
    "abstract",
    "override",
    "virtual",
    "inline",
    "declare",
    "readonly",
    "open",
    "internal",
    "sealed",
    "data",
    "suspend",
    "const",
    "let",
    "var",
];

fn ident(s: &str) -> String {
    s.chars().take_while(|c| c.is_alphanumeric() || *c == '_' || *c == '$').collect()
}

/// The declared name on a line of a brace language, if the line declares something.
fn brace_decl(line: &str) -> Option<String> {
    let t = line.trim_start();
    if t.is_empty() || t.starts_with("//") || t.starts_with('*') || t.starts_with("/*") || t.starts_with('#') {
        return None;
    }
    let mut words = t.split_whitespace().peekable();
    let mut kw: Option<&str> = None;
    while let Some(w) = words.peek().copied() {
        let w0 = w.split('(').next().unwrap_or(w);
        if BRACE_KEYWORDS.contains(&w0) && !(w0 == "const" && kw.is_none() && t.contains(" fn ")) {
            kw = Some(w0);
            words.next();
            // `impl<T> Trait for Type`: name it after the type.
            if w0 == "impl" {
                let rest: String = t.split_once("impl").map(|x| x.1).unwrap_or_default().to_string();
                let rest = strip_generics(&rest);
                let target = match rest.split_once(" for ") {
                    Some((tr, ty)) => format!("{} for {}", ident(tr.trim()), ident(ty.trim())),
                    None => ident(rest.trim()),
                };
                return (!target.is_empty()).then(|| format!("impl {target}"));
            }
            break;
        }
        if MODIFIERS.contains(&w0) || w0.starts_with("pub(") || w0.starts_with('@') {
            words.next();
            continue;
        }
        break;
    }
    match kw {
        Some(k) => {
            let next = words.next()?;
            // Go methods: `func (r *T) Name(`.
            let next = if k == "func" && next.starts_with('(') {
                t.split_once(')').map(|x| x.1.trim_start()).unwrap_or(next)
            } else {
                next
            };
            let name = ident(next.trim_start_matches('*'));
            (!name.is_empty()).then_some(name)
        }
        None => {
            // `const handler = (...) =>` and `name(args) {` method shorthand in classes.
            let first = ident(t);
            if first.is_empty()
                || ["if", "for", "while", "switch", "return", "else", "match", "catch", "do"].contains(&first.as_str())
            {
                return None;
            }
            let after = &t[first.len()..];
            (after.starts_with('(') && t.trim_end().ends_with('{')).then_some(first)
        }
    }
}

fn strip_generics(s: &str) -> String {
    let mut depth = 0;
    let mut out = String::new();
    for c in s.chars() {
        match c {
            '<' => depth += 1,
            '>' if depth > 0 => depth -= 1,
            _ if depth == 0 => out.push(c),
            _ => {}
        }
    }
    out
}

/// Brace depth change of a line, skipping strings and `//` comments.
fn brace_delta(line: &str, in_block_comment: &mut bool) -> i32 {
    let mut d = 0;
    let b = line.as_bytes();
    let mut i = 0;
    let mut quote: Option<u8> = None;
    while i < b.len() {
        let c = b[i];
        if *in_block_comment {
            if c == b'*' && b.get(i + 1) == Some(&b'/') {
                *in_block_comment = false;
                i += 1;
            }
        } else if let Some(q) = quote {
            if c == b'\\' {
                i += 1;
            } else if c == q {
                quote = None;
            }
        } else {
            match c {
                b'/' if b.get(i + 1) == Some(&b'/') => break,
                b'/' if b.get(i + 1) == Some(&b'*') => {
                    *in_block_comment = true;
                    i += 1;
                }
                b'"' | b'`' => quote = Some(c),
                b'{' => d += 1,
                b'}' => d -= 1,
                _ => {}
            }
        }
        i += 1;
    }
    d
}

fn is_decoration(line: &str) -> bool {
    let t = line.trim_start();
    t.starts_with("///")
        || t.starts_with("//!")
        || t.starts_with("#[")
        || t.starts_with('@')
        || t.starts_with("/**")
        || t.starts_with("* ")
        || t == "*"
        || t.starts_with("*/")
        || t.starts_with("//")
}

/// Pulls a symbol's start up over doc comments and attributes directly above it.
fn with_decorations(lines: &[&str], mut start: usize, floor: usize) -> usize {
    while start > floor && is_decoration(lines[start - 1]) {
        start -= 1;
    }
    start
}

fn brace_symbols(lines: &[&str]) -> Vec<Symbol> {
    let mut out = vec![];
    let mut depth = 0i32;
    let mut in_comment = false;
    // (name, start line, depth at start, container name)
    let mut open: Vec<(String, usize, i32)> = vec![];
    let mut floor = 0;
    for (i, line) in lines.iter().enumerate() {
        let container = open.last().map(|o| o.0.clone());
        let decl = if !in_comment
            && (depth == 0 || (depth == 1 && open.len() == 1 && is_container(&open[0].0, lines[open[0].1])))
        {
            brace_decl(line)
        } else {
            None
        };
        let delta = brace_delta(line, &mut in_comment);
        if let Some(name) = decl {
            let name = match (&container, depth) {
                (Some(c), 1) => format!("{}::{name}", c.trim_start_matches("impl ").split(" for ").last().unwrap_or(c)),
                _ => name,
            };
            let start = with_decorations(lines, i, floor);
            if delta > 0 {
                open.push((name, start, depth));
            } else {
                // One-line item, or a declaration whose body starts on a later line.
                let ends_here = line.trim_end().ends_with(';') || line.trim_end().ends_with('}');
                if ends_here {
                    out.push(Symbol { name, start, end: i });
                    floor = i + 1;
                } else {
                    open.push((name, start, depth));
                }
            }
        }
        depth += delta;
        // Close every symbol whose body ended on this line.
        while let Some((_, _, d)) = open.last() {
            let opened_body = depth > *d;
            let ended = depth <= *d && (delta < 0 || line.trim_end().ends_with(';'));
            if opened_body || !ended {
                break;
            }
            if let Some((name, start, _)) = open.pop() {
                out.push(Symbol { name, start, end: i });
                floor = i + 1;
            }
        }
        if depth < 0 {
            depth = 0;
        }
    }
    out.sort_by_key(|s| (s.start, std::cmp::Reverse(s.end)));
    out
}

fn is_container(name: &str, decl_line: &str) -> bool {
    name.starts_with("impl ")
        || ["class ", "trait ", "interface ", "object ", "namespace ", "mod "].iter().any(|k| decl_line.contains(k))
}

fn indent_of(l: &str) -> usize {
    l.chars().take_while(|c| *c == ' ' || *c == '\t').map(|c| if c == '\t' { 4 } else { 1 }).sum()
}

fn indent_symbols(lines: &[&str]) -> Vec<Symbol> {
    let mut out = vec![];
    let decl = |l: &str| -> Option<String> {
        let t = l.trim_start();
        let rest =
            t.strip_prefix("async def ").or_else(|| t.strip_prefix("def ")).or_else(|| t.strip_prefix("class "))?;
        let n = ident(rest);
        (!n.is_empty()).then_some(n)
    };
    let end_of = |from: usize, ind: usize| -> usize {
        let mut end = from;
        for (j, l) in lines.iter().enumerate().skip(from + 1) {
            if l.trim().is_empty() || l.trim_start().starts_with('#') && indent_of(l) > ind {
                continue;
            }
            if indent_of(l) <= ind {
                break;
            }
            end = j;
        }
        end
    };
    let mut floor = 0;
    let mut i = 0;
    while i < lines.len() {
        let l = lines[i];
        if indent_of(l) == 0
            && let Some(name) = decl(l)
        {
            let mut start = i;
            while start > floor && lines[start - 1].trim_start().starts_with('@') {
                start -= 1;
            }
            let end = end_of(i, 0);
            out.push(Symbol { name: name.clone(), start, end });
            if l.trim_start().starts_with("class ") {
                // Methods one level in.
                let body_ind =
                    lines[i + 1..=end].iter().find(|x| !x.trim().is_empty()).map(|x| indent_of(x)).unwrap_or(4);
                let mut mfloor = i + 1;
                let mut j = i + 1;
                while j <= end {
                    if indent_of(lines[j]) == body_ind
                        && let Some(m) = decl(lines[j])
                    {
                        let mut s = j;
                        while s > mfloor && lines[s - 1].trim_start().starts_with('@') {
                            s -= 1;
                        }
                        let e = end_of(j, body_ind);
                        out.push(Symbol { name: format!("{name}::{m}"), start: s, end: e });
                        mfloor = e + 1;
                        j = e + 1;
                        continue;
                    }
                    j += 1;
                }
            }
            floor = end + 1;
            i = end + 1;
            continue;
        }
        i += 1;
    }
    out
}

/// Symbols of `content`, outermost first in file order. Members of classes,
/// impls and traits follow their container and lie inside its range.
pub fn symbols(content: &str, lang: Lang) -> Vec<Symbol> {
    let lines: Vec<&str> = content.lines().collect();
    match lang {
        Lang::Braces => brace_symbols(&lines),
        Lang::Indent => indent_symbols(&lines),
    }
}

/// Top-level symbols only (no members).
pub fn top_level(content: &str, lang: Lang) -> Vec<Symbol> {
    let all = symbols(content, lang);
    let mut out: Vec<Symbol> = vec![];
    for s in all {
        if out.last().is_none_or(|p| s.start > p.end) {
            out.push(s);
        }
    }
    out
}

/// Names of the innermost symbols whose lines differ between `old` and `new`.
/// None when a change falls outside every symbol (then claim the whole file).
pub fn changed_symbols(old: &str, new: &str, lang: Lang) -> Option<Vec<String>> {
    use similar::{DiffTag, TextDiff};
    let new_syms = symbols(new, lang);
    let old_syms = symbols(old, lang);
    let innermost = |syms: &[Symbol], line: usize| -> Option<String> {
        syms.iter().filter(|s| s.start <= line && line <= s.end).min_by_key(|s| s.end - s.start).map(|s| s.name.clone())
    };
    let diff = TextDiff::from_lines(old, new);
    let mut names: Vec<String> = vec![];
    for op in diff.ops() {
        if op.tag() == DiffTag::Equal {
            continue;
        }
        let (o, n) = (op.old_range(), op.new_range());
        let mut hit = false;
        for l in n.clone() {
            let s = innermost(&new_syms, l)?;
            hit = true;
            if !names.contains(&s) {
                names.push(s);
            }
        }
        for l in o.clone() {
            let s = innermost(&old_syms, l)?;
            hit = true;
            if !names.contains(&s) {
                names.push(s);
            }
        }
        if !hit {
            // Pure insertion of blank space between symbols, or similar.
            return None;
        }
    }
    Some(names)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    const RS: &str = r#"use std::fmt;

/// An account.
#[derive(Debug)]
pub struct Account {
    balance: i64,
}

impl Account {
    /// Adds money.
    pub fn deposit(&mut self, n: i64) {
        if n > 0 { self.balance += n; }
    }

    pub fn balance(&self) -> i64 { self.balance }
}

const LIMIT: i64 = 100;

pub async fn transfer(a: &mut Account, b: &mut Account) -> Result<(), String> {
    let s = "{ not a brace }";
    Ok(())
}
"#;

    #[test]
    fn rust_symbols() {
        let s = symbols(RS, Lang::Braces);
        let names: Vec<(&str, usize, usize)> = s.iter().map(|x| (x.name.as_str(), x.start, x.end)).collect();
        assert_eq!(
            names,
            vec![
                ("Account", 2, 6),
                ("impl Account", 8, 15),
                ("Account::deposit", 9, 12),
                ("Account::balance", 14, 14),
                ("LIMIT", 17, 17),
                ("transfer", 19, 22),
            ]
        );
        assert_eq!(top_level(RS, Lang::Braces).len(), 4);
    }

    #[test]
    fn ts_and_go_and_python() {
        let ts = "export class Cart {\n  add(item: Item) {\n    this.items.push(item);\n  }\n}\n\nexport function total(c: Cart): number {\n  return 0;\n}\n";
        let n: Vec<String> = symbols(ts, Lang::Braces).into_iter().map(|s| s.name).collect();
        assert_eq!(n, vec!["Cart", "Cart::add", "total"]);
        let go = "package main\n\nfunc (s *Server) Start() error {\n\treturn nil\n}\n\nfunc main() {\n}\n";
        let n: Vec<String> = symbols(go, Lang::Braces).into_iter().map(|s| s.name).collect();
        assert_eq!(n, vec!["Start", "main"]);
        let py = "import os\n\n@dataclass\nclass User:\n    name: str\n\n    def greet(self):\n        return 'hi'\n\n\ndef main():\n    pass\n";
        let s = symbols(py, Lang::Indent);
        let n: Vec<(&str, usize, usize)> = s.iter().map(|x| (x.name.as_str(), x.start, x.end)).collect();
        assert_eq!(n, vec![("User", 2, 7), ("User::greet", 6, 7), ("main", 10, 11)]);
    }

    #[test]
    fn which_symbols_changed() {
        let new = RS.replace("self.balance += n;", "self.balance += n * 2;");
        assert_eq!(changed_symbols(RS, &new, Lang::Braces), Some(vec!["Account::deposit".to_string()]));
        let new = RS.replace("use std::fmt;", "use std::fmt::Write;");
        assert_eq!(changed_symbols(RS, &new, Lang::Braces), None);
        assert_eq!(Lang::from_path("src/a.tsx"), Some(Lang::Braces));
        assert_eq!(Lang::from_path("README.md"), None);
    }
}
