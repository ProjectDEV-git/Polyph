//! Skills: folders with a `SKILL.md` that tell agents how this team does a
//! kind of task (release steps, review checklists, house style). The file
//! starts with front matter:
//!
//! ```text
//! ---
//! name: release
//! description: How we cut a release: version bump, changelog, tag.
//! ---
//! Steps…
//! ```
//!
//! Project skills live in the repository (`.polyph/skills/<name>/SKILL.md`)
//! and travel with it; personal skills live in `<polyph home>/skills/`.
//! Agents list and read them through Polyph's MCP tools. A project skill
//! wins over a personal one with the same name.

use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::{Error, Result, valid_name};

pub const FILE: &str = "SKILL.md";
pub const PROJECT_DIR: &str = ".polyph/skills";
/// Longest skill text served to an agent.
pub const MAX_SKILL_BYTES: usize = 64 * 1024;

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Source {
    Project,
    Personal,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Skill {
    pub name: String,
    pub description: String,
    pub source: Source,
    #[serde(skip)]
    pub dir: PathBuf,
}

/// Front matter `name` and `description`, and the body.
pub fn parse(text: &str) -> Option<(String, String, String)> {
    let rest = text.strip_prefix("---")?.trim_start_matches(['\r', '\n']);
    let end = rest.find("\n---")?;
    let (head, body) = (&rest[..end], &rest[end + 4..]);
    let mut name = None;
    let mut desc = String::new();
    for line in head.lines() {
        if let Some((k, v)) = line.split_once(':') {
            let v = v.trim().trim_matches('"').to_string();
            match k.trim() {
                "name" => name = Some(v),
                "description" => desc = v,
                _ => {}
            }
        }
    }
    Some((name?, desc, body.trim_start_matches(['\r', '\n']).to_string()))
}

/// A skill file; under the agent's `worktree` it must not be a link (SEC-16).
fn read_file(worktree: Option<&Path>, path: &Path) -> std::io::Result<String> {
    match worktree.and_then(|w| path.strip_prefix(w).ok().map(|rel| (w, rel))) {
        Some((w, rel)) => crate::read_text_under(w, rel),
        None => std::fs::read_to_string(path),
    }
}

fn scan(dir: &Path, source: Source, worktree: Option<&Path>) -> Vec<Skill> {
    let Ok(rd) = std::fs::read_dir(dir) else { return vec![] };
    rd.flatten()
        .filter(|e| e.path().join(FILE).is_file())
        .filter_map(|e| {
            let text = read_file(worktree, &e.path().join(FILE)).ok()?;
            let (name, description, _) = parse(&text)?;
            valid_name(&name).then(|| Skill { name, description, source: source.clone(), dir: e.path() })
        })
        .collect()
}

/// Skills for an agent working in `worktree`, with the person's own ones.
pub fn discover(worktree: Option<&Path>, personal: &Path) -> Vec<Skill> {
    let mut out = worktree.map(|w| scan(&w.join(PROJECT_DIR), Source::Project, Some(w))).unwrap_or_default();
    for s in scan(personal, Source::Personal, None) {
        if !out.iter().any(|p| p.name == s.name) {
            out.push(s);
        }
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}

/// One skill's text (front matter removed), capped for the agent.
pub fn read(worktree: Option<&Path>, personal: &Path, name: &str) -> Result<(Skill, String)> {
    let s = discover(worktree, personal)
        .into_iter()
        .find(|s| s.name == name)
        .ok_or_else(|| Error::NotFound(format!("no skill named {name}; list them with list_skills")))?;
    let text = read_file(worktree, &s.dir.join(FILE))?;
    let (_, _, mut body) = parse(&text).ok_or_else(|| Error::Invalid(format!("{name}: bad front matter")))?;
    if body.len() > MAX_SKILL_BYTES {
        let mut cut = MAX_SKILL_BYTES;
        while !body.is_char_boundary(cut) {
            cut -= 1;
        }
        body.truncate(cut);
        body.push_str("\n[…truncated]");
    }
    Ok((s, body))
}

/// Copies a skill folder into a repository's `.polyph/skills/`.
pub fn add_to_project(from: &Path, repo: &Path) -> Result<Skill> {
    let text = std::fs::read_to_string(from.join(FILE))
        .map_err(|_| Error::NotFound(format!("{} has no {FILE}", from.display())))?;
    let (name, description, _) =
        parse(&text).ok_or_else(|| Error::Invalid(format!("{FILE} needs name and description front matter")))?;
    if !valid_name(&name) {
        return Err(Error::Invalid(format!("skill name \"{name}\" must be lower-case letters, digits and dashes")));
    }
    let to = repo.join(PROJECT_DIR).join(&name);
    if to.exists() {
        return Err(Error::Exists(format!("{} already exists", to.display())));
    }
    let mut budget = 2 * 1024 * 1024;
    crate::store::copy_dir(from, &to, &mut budget)?;
    Ok(Skill { name, description, source: Source::Project, dir: to })
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn skill(dir: &Path, name: &str, desc: &str, body: &str) {
        std::fs::create_dir_all(dir.join(name)).unwrap();
        std::fs::write(dir.join(name).join(FILE), format!("---\nname: {name}\ndescription: {desc}\n---\n{body}"))
            .unwrap();
    }

    #[test]
    fn project_skills_win_and_read() {
        let repo = tempfile::tempdir().unwrap();
        let home = tempfile::tempdir().unwrap();
        skill(&repo.path().join(PROJECT_DIR), "release", "How we release", "1. bump\n");
        skill(home.path(), "release", "mine", "x");
        skill(home.path(), "review", "My review list", "- tests\n");
        std::fs::create_dir_all(home.path().join("broken")).unwrap();
        std::fs::write(home.path().join("broken").join(FILE), "no front matter").unwrap();
        let all = discover(Some(repo.path()), home.path());
        let names: Vec<_> = all.iter().map(|s| (s.name.as_str(), s.source.clone())).collect();
        assert_eq!(names, vec![("release", Source::Project), ("review", Source::Personal)]);
        let (_, body) = read(Some(repo.path()), home.path(), "release").unwrap();
        assert_eq!(body, "1. bump\n");
        assert!(read(None, home.path(), "nope").is_err());
        let added = add_to_project(&home.path().join("review"), repo.path()).unwrap();
        assert_eq!(added.source, Source::Project);
        assert!(add_to_project(&home.path().join("review"), repo.path()).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn linked_project_skills_are_skipped() {
        let repo = tempfile::tempdir().unwrap();
        let home = tempfile::tempdir().unwrap();
        skill(home.path(), "secret", "outside the repo", "x");
        std::fs::create_dir_all(repo.path().join(PROJECT_DIR)).unwrap();
        std::os::unix::fs::symlink(home.path().join("secret"), repo.path().join(PROJECT_DIR).join("secret")).unwrap();
        let all = discover(Some(repo.path()), tempfile::tempdir().unwrap().path());
        assert!(all.is_empty(), "{all:?}");
    }
}
