//! Which folder on this device belongs to which room.
//!
//! `polyph host`, `polyph join` and `polyph start` link the folder they run
//! in to the room, so that agents started there (by Polyph or by the person,
//! through `polyph mcp`) land in the right room without a `--room` flag, and
//! so the daemon knows which checkouts to keep fetched (see `sync`).
//!
//! Stored in `folders.json` in the Polyph home; paths are canonical.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Link {
    pub room: String,
    pub path: PathBuf,
}

fn file(home: &Path) -> PathBuf {
    home.join("folders.json")
}

pub fn load(home: &Path) -> Vec<Link> {
    std::fs::read(file(home)).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()
}

fn save(home: &Path, links: &[Link]) -> std::io::Result<()> {
    crate::paths::write_private(&file(home), &serde_json::to_vec_pretty(links).unwrap_or_default())
}

/// The repository root containing `dir`, or `dir` itself outside git.
pub fn root(dir: &Path) -> PathBuf {
    let dir = polyph_git::canonical(dir).unwrap_or_else(|_| dir.to_path_buf());
    let mut d = dir.clone();
    loop {
        if d.join(".git").exists() {
            return d;
        }
        if !d.pop() {
            return dir;
        }
    }
}

/// Links `dir` (its repository root) to `room`, replacing any earlier link
/// of that folder. A room may have several folders (worktrees, clones).
pub fn link(home: &Path, room: &str, dir: &Path) -> std::io::Result<PathBuf> {
    let path = root(dir);
    let mut links = load(home);
    links.retain(|l| l.path != path);
    links.push(Link { room: room.to_string(), path: path.clone() });
    save(home, &links)?;
    Ok(path)
}

/// Removes the link of `dir`, or every link of `room`.
pub fn unlink(home: &Path, room: Option<&str>, dir: Option<&Path>) -> std::io::Result<usize> {
    let mut links = load(home);
    let before = links.len();
    let path = dir.map(root);
    links.retain(|l| !(room.is_some_and(|r| r == l.room) || path.as_ref().is_some_and(|p| *p == l.path)));
    let n = before - links.len();
    if n > 0 {
        save(home, &links)?;
    }
    Ok(n)
}

fn exposed_file(home: &Path) -> PathBuf {
    home.join("agent-writable-git.json")
}

/// Records that an agent ran in the checkout at `root` with a sandbox that
/// does not keep its git config, hooks and info read-only (Landlock,
/// Windows, container mode), so they may be the agent's now (SEC-13).
pub fn mark_git_exposed(home: &Path, root: &Path) -> std::io::Result<()> {
    let root = polyph_git::canonical(root).unwrap_or_else(|_| root.to_path_buf());
    let mut v: Vec<PathBuf> =
        std::fs::read(exposed_file(home)).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default();
    if v.contains(&root) {
        return Ok(());
    }
    v.push(root);
    crate::paths::write_private(&exposed_file(home), &serde_json::to_vec_pretty(&v).unwrap_or_default())
}

fn branches_file(home: &Path) -> PathBuf {
    home.join("task-branches.json")
}

/// Task branches the daemon created, per repository (the main checkout).
type Created = std::collections::BTreeMap<PathBuf, Vec<String>>;

fn created(home: &Path) -> Created {
    std::fs::read(branches_file(home)).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()
}

/// One read-and-write of the record at a time.
static BRANCHES: parking_lot::Mutex<()> = parking_lot::Mutex::new(());

/// Records that the daemon created `branch` in the repository at `root` for
/// a task, so it may give it to a later agent on that task (SEC-13).
pub fn record_task_branch(home: &Path, root: &Path, branch: &str) -> std::io::Result<()> {
    let _g = BRANCHES.lock();
    let root = polyph_git::canonical(root).unwrap_or_else(|_| root.to_path_buf());
    let mut all = created(home);
    let v = all.entry(root).or_default();
    if v.iter().any(|b| b == branch) {
        return Ok(());
    }
    v.push(branch.to_string());
    crate::paths::write_private(&branches_file(home), &serde_json::to_vec_pretty(&all).unwrap_or_default())
}

/// Whether the daemon created `branch` in the repository at `root`
/// ([`record_task_branch`]).
pub fn created_task_branch(home: &Path, root: &Path, branch: &str) -> bool {
    let _g = BRANCHES.lock();
    let root = polyph_git::canonical(root).unwrap_or_else(|_| root.to_path_buf());
    created(home).get(&root).is_some_and(|v| v.iter().any(|b| b == branch))
}

/// Opens the checkout at `dir` for a fetch or push. One an agent may have
/// written the git settings of (see [`mark_git_exposed`]) runs with the
/// guarded settings; any other with its own, so per-repository SSH
/// commands, `includeIf` and hooks such as Git LFS's keep working.
pub fn open_for_network(home: &Path, dir: &Path) -> polyph_git::Result<polyph_git::Repo> {
    let r = polyph_git::Repo::open(dir)?;
    let root = polyph_git::canonical(r.root()).unwrap_or_else(|_| r.root().to_path_buf());
    let exposed: Vec<PathBuf> =
        std::fs::read(exposed_file(home)).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default();
    if exposed.contains(&root) { Ok(r) } else { polyph_git::Repo::open_trusted(dir) }
}

/// The room linked to `dir` or to the nearest folder above it.
pub fn room_for(home: &Path, dir: &Path) -> Option<Link> {
    let dir = polyph_git::canonical(dir).unwrap_or_else(|_| dir.to_path_buf());
    load(home).into_iter().filter(|l| dir.starts_with(&l.path)).max_by_key(|l| l.path.components().count())
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    #[test]
    fn link_find_unlink() {
        let home = tempfile::tempdir().unwrap();
        let repo = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(repo.path().join(".git")).unwrap();
        std::fs::create_dir_all(repo.path().join("src/deep")).unwrap();
        let at = link(home.path(), "r1", &repo.path().join("src")).unwrap();
        assert_eq!(at, polyph_git::canonical(repo.path()).unwrap(), "links the repository root");
        assert_eq!(room_for(home.path(), &repo.path().join("src/deep")).unwrap().room, "r1");
        link(home.path(), "r2", repo.path()).unwrap();
        assert_eq!(load(home.path()).len(), 1, "a folder has one room");
        assert_eq!(room_for(home.path(), repo.path()).unwrap().room, "r2");
        assert!(room_for(home.path(), home.path()).is_none());
        assert_eq!(unlink(home.path(), Some("r2"), None).unwrap(), 1);
        assert!(load(home.path()).is_empty());
    }

    /// A checkout an agent could write the git settings of is fetched with
    /// the guarded settings; any other with its own.
    #[cfg(unix)]
    #[test]
    fn exposed_checkouts_fetch_without_their_own_programs() {
        use std::os::unix::fs::PermissionsExt;
        let home = tempfile::tempdir().unwrap();
        let t = tempfile::tempdir().unwrap();
        let git = |dir: &Path, args: &[&str]| {
            let out = std::process::Command::new("git").arg("-C").arg(dir).args(args).output().unwrap();
            assert!(out.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&out.stderr));
        };
        let (origin, repo) = (t.path().join("origin"), t.path().join("repo"));
        git(t.path(), &["init", "-q", "--bare", &origin.to_string_lossy()]);
        git(t.path(), &["init", "-q", &repo.to_string_lossy()]);
        git(&repo, &["remote", "add", "origin", &origin.to_string_lossy()]);
        let (script, ran) = (t.path().join("pack.sh"), t.path().join("ran"));
        std::fs::write(&script, format!("#!/bin/sh\ntouch '{}'\nexit 1\n", ran.display())).unwrap();
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
        git(&repo, &["config", "remote.origin.uploadpack", &script.to_string_lossy()]);

        assert!(open_for_network(home.path(), &repo).unwrap().fetch("origin").is_err());
        assert!(ran.exists(), "a checkout no agent could write keeps its own settings");
        std::fs::remove_file(&ran).unwrap();
        mark_git_exposed(home.path(), &repo).unwrap();
        mark_git_exposed(home.path(), &repo).unwrap();
        open_for_network(home.path(), &repo.join(".git").join("..")).unwrap().fetch("origin").unwrap();
        assert!(!ran.exists(), "an exposed checkout ran its uploadpack");
    }
}
