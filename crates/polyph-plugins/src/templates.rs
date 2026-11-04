//! Project templates: a starting `polyph.toml` and skills for a repository.
//! Built-in ones cover the launch audiences; personal ones are folders in
//! `<polyph home>/templates/<name>/` copied as they are. Applying never
//! overwrites a file that exists.

use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::{Error, Result, valid_name};

#[derive(Clone, Debug, Serialize)]
pub struct Template {
    pub name: String,
    pub description: String,
    pub builtin: bool,
    #[serde(skip)]
    files: Vec<(String, String)>,
    #[serde(skip)]
    dir: Option<PathBuf>,
}

const REVIEW_SKILL: &str = "---
name: review
description: The checklist reviewers and authors use before a task merges.
---
Before you call a task finished:

1. The card's acceptance criteria are met, each one checked by hand or by a test.
2. `checks` in polyph.toml pass on your branch.
3. New behavior has a test; changed behavior has an updated test.
4. No secrets, credentials or personal data in code, logs or fixtures.
5. Release your claims on files you no longer need.
";

const HANDOFF_SKILL: &str = "---
name: handoff
description: How to leave a task so someone else can pick it up.
---
When you stop before a task is done, write in the card's thread:

- what is done, with file paths;
- what is left, as a short list;
- anything surprising you found, and where;
- the exact command that shows the current state (a test, a script).
";

fn builtin() -> Vec<Template> {
    let t = |name: &str, description: &str, toml: &str, skills: &[(&str, &str)]| {
        let mut files = vec![("polyph.toml".to_string(), toml.to_string())];
        for (n, body) in skills {
            files.push((format!("{}/{n}/SKILL.md", crate::skills::PROJECT_DIR), body.to_string()));
        }
        Template { name: name.into(), description: description.into(), builtin: true, files, dir: None }
    };
    vec![
        t(
            "startup",
            "A small product team: agents work freely in their claims, a reviewer agent checks each task, a person approves merges.",
            "# Polyph project settings. See docs in the repository for every key.
checks = []          # commands agents may run freely, e.g. \"cargo test\"

[policy]
preset = \"default\"

[merge]
reviewer = \"claude\"
approval = true
run_checks = true

[claims]
granularity = \"file\"
",
            &[("review", REVIEW_SKILL), ("handoff", HANDOFF_SKILL)],
        ),
        t(
            "hackathon",
            "Move fast for a weekend: agents run anything inside their sandbox, merges need no review.",
            "checks = []

[policy]
preset = \"auto\"

[merge]
approval = false
run_checks = true

[claims]
granularity = \"symbol\"
",
            &[("handoff", HANDOFF_SKILL)],
        ),
        t(
            "open-source",
            "Contributors' agents on one public repository: every action asks first, reviews before merge, pull requests on GitHub.",
            "checks = []

[policy]
preset = \"ask-all\"

[merge]
reviewer = \"claude\"
approval = true
run_checks = true
via = \"github\"

[claims]
granularity = \"file\"
",
            &[("review", REVIEW_SKILL), ("handoff", HANDOFF_SKILL)],
        ),
        t(
            "agency",
            "Client work: careful defaults, a reviewer, and merges a person approves.",
            "checks = []

[policy]
preset = \"default\"

[merge]
reviewer = \"claude\"
approval = true
run_checks = true

[claims]
granularity = \"file\"
",
            &[("review", REVIEW_SKILL), ("handoff", HANDOFF_SKILL)],
        ),
    ]
}

/// Built-in templates, then the person's own (a personal one may not reuse a built-in name).
pub fn list(personal: &Path) -> Vec<Template> {
    let mut out = builtin();
    if let Ok(rd) = std::fs::read_dir(personal) {
        let mut mine: Vec<Template> = rd
            .flatten()
            .filter(|e| e.path().is_dir())
            .filter_map(|e| {
                let name = e.file_name().to_string_lossy().into_owned();
                if !valid_name(&name) || out.iter().any(|t| t.name == name) {
                    return None;
                }
                let description = std::fs::read_to_string(e.path().join("TEMPLATE.md"))
                    .ok()
                    .and_then(|s| s.lines().find(|l| !l.trim().is_empty() && !l.starts_with('#')).map(str::to_string))
                    .unwrap_or_default();
                Some(Template { name, description, builtin: false, files: vec![], dir: Some(e.path()) })
            })
            .collect();
        mine.sort_by(|a, b| a.name.cmp(&b.name));
        out.extend(mine);
    }
    out
}

fn files_of(t: &Template) -> Result<Vec<(String, Vec<u8>)>> {
    if let Some(dir) = &t.dir {
        let mut out = vec![];
        walk(dir, dir, &mut out)?;
        out.retain(|(p, _)| p != "TEMPLATE.md");
        return Ok(out);
    }
    Ok(t.files.iter().map(|(p, s)| (p.clone(), s.clone().into_bytes())).collect())
}

fn walk(root: &Path, dir: &Path, out: &mut Vec<(String, Vec<u8>)>) -> Result<()> {
    for e in std::fs::read_dir(dir)? {
        let e = e?;
        let ft = e.file_type()?;
        if ft.is_dir() && e.file_name() != ".git" {
            walk(root, &e.path(), out)?;
        } else if ft.is_file() {
            let rel = e.path().strip_prefix(root).map_err(|_| Error::Invalid("bad path".into()))?.to_path_buf();
            let rel = rel.components().map(|c| c.as_os_str().to_string_lossy()).collect::<Vec<_>>().join("/");
            out.push((rel, std::fs::read(e.path())?));
        }
    }
    Ok(())
}

#[derive(Debug, Default, Serialize)]
pub struct Applied {
    pub written: Vec<String>,
    /// Files that already existed and were left alone.
    pub kept: Vec<String>,
}

/// Writes a template's files into `repo`, keeping files that exist.
pub fn apply(personal: &Path, name: &str, repo: &Path) -> Result<Applied> {
    let t = list(personal)
        .into_iter()
        .find(|t| t.name == name)
        .ok_or_else(|| Error::NotFound(format!("no template named {name}; see `polyph template list`")))?;
    let mut out = Applied::default();
    for (rel, body) in files_of(&t)? {
        let p = repo.join(&rel);
        if p.exists() {
            out.kept.push(rel);
            continue;
        }
        if let Some(parent) = p.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&p, body)?;
        out.written.push(rel);
    }
    Ok(out)
}

/// The files a template would write, for tests and previews.
pub fn preview(personal: &Path, name: &str) -> Result<Vec<(String, String)>> {
    let t = list(personal).into_iter().find(|t| t.name == name).ok_or_else(|| Error::NotFound(name.to_string()))?;
    Ok(files_of(&t)?.into_iter().map(|(p, b)| (p, String::from_utf8_lossy(&b).into_owned())).collect())
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn applies_without_overwriting() {
        let home = tempfile::tempdir().unwrap();
        let repo = tempfile::tempdir().unwrap();
        std::fs::write(repo.path().join("polyph.toml"), "# mine\n").unwrap();
        let a = apply(home.path(), "startup", repo.path()).unwrap();
        assert_eq!(a.kept, vec!["polyph.toml"]);
        assert!(a.written.contains(&".polyph/skills/review/SKILL.md".to_string()));
        assert_eq!(std::fs::read_to_string(repo.path().join("polyph.toml")).unwrap(), "# mine\n");
        // The written skills are real skills.
        let skills = crate::skills::discover(Some(repo.path()), home.path());
        assert_eq!(skills.iter().map(|s| s.name.as_str()).collect::<Vec<_>>(), vec!["handoff", "review"]);
        assert!(apply(home.path(), "nope", repo.path()).is_err());
    }

    #[test]
    fn personal_templates() {
        let home = tempfile::tempdir().unwrap();
        let t = home.path().join("ours");
        std::fs::create_dir_all(t.join("docs")).unwrap();
        std::fs::write(t.join("TEMPLATE.md"), "# Ours\nOur house setup.\n").unwrap();
        std::fs::write(t.join("docs/README.md"), "hello").unwrap();
        std::fs::create_dir_all(home.path().join("startup")).unwrap();
        let all = list(home.path());
        let ours = all.iter().find(|t| t.name == "ours").unwrap();
        assert_eq!(ours.description, "Our house setup.");
        assert_eq!(all.iter().filter(|t| t.name == "startup").count(), 1, "built-ins win");
        let repo = tempfile::tempdir().unwrap();
        let a = apply(home.path(), "ours", repo.path()).unwrap();
        assert_eq!(a.written, vec!["docs/README.md"]);
    }
}
