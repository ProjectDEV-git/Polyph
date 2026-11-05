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
}
