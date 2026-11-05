//! Keys for agents that reach Polyph over its local web address
//! (`http://127.0.0.1:<port>/mcp`) instead of starting `polyph mcp`.
//!
//! `polyph connect --http` asks for one per client and writes it into that
//! client's MCP config as an `Authorization: Bearer` header. A key can only
//! call Polyph's agent tools (never the rest of the local API), names the
//! client it was made for, and remembers the folder it was made in, which
//! picks the room and is where paths are relative to. `polyph disconnect`
//! revokes it.
//!
//! Stored in `connect-keys.json` (private to the user, like `api.token`).

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Key {
    pub key: String,
    pub client: String,
    /// The folder `polyph connect` ran in (its repository root).
    pub dir: Option<PathBuf>,
    pub created: i64,
}

fn file(home: &Path) -> PathBuf {
    home.join("connect-keys.json")
}

pub fn load(home: &Path) -> Vec<Key> {
    std::fs::read(file(home)).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()
}

fn save(home: &Path, keys: &[Key]) -> std::io::Result<()> {
    crate::paths::write_private(&file(home), &serde_json::to_vec_pretty(keys).unwrap_or_default())
}

/// The key for `client` in `dir`, made if there is none yet.
pub fn issue(home: &Path, client: &str, dir: Option<&Path>, now: i64) -> anyhow::Result<Key> {
    let dir = dir.map(crate::folders::root);
    let mut keys = load(home);
    if let Some(k) = keys.iter().find(|k| k.client == client && k.dir == dir) {
        return Ok(k.clone());
    }
    let mut b = [0u8; 32];
    getrandom::getrandom(&mut b).map_err(|_| anyhow::anyhow!("no system randomness"))?;
    let k = Key { key: format!("pk_{}", hex::encode(b)), client: client.to_string(), dir, created: now };
    keys.push(k.clone());
    save(home, &keys)?;
    Ok(k)
}

/// Revokes the keys of `client` (in `dir`, when given). Returns how many.
pub fn revoke(home: &Path, client: &str, dir: Option<&Path>) -> std::io::Result<usize> {
    let dir = dir.map(crate::folders::root);
    let mut keys = load(home);
    let before = keys.len();
    keys.retain(|k| !(k.client == client && (dir.is_none() || k.dir == dir)));
    let n = before - keys.len();
    if n > 0 {
        save(home, &keys)?;
    }
    Ok(n)
}

/// The key record for a presented secret (constant-time comparison).
pub fn find(home: &Path, presented: &str) -> Option<Key> {
    load(home).into_iter().find(|k| crate::api::ct_eq(k.key.as_bytes(), presented.as_bytes()))
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    #[test]
    fn issue_reuse_find_revoke() {
        let home = tempfile::tempdir().unwrap();
        let a = issue(home.path(), "cursor", None, 1).unwrap();
        assert!(a.key.starts_with("pk_") && a.key.len() > 60);
        assert_eq!(issue(home.path(), "cursor", None, 2).unwrap(), a, "one key per client and folder");
        let b = issue(home.path(), "zed", None, 3).unwrap();
        assert_ne!(a.key, b.key);
        assert_eq!(find(home.path(), &a.key).unwrap().client, "cursor");
        assert!(find(home.path(), "pk_wrong").is_none());
        assert_eq!(revoke(home.path(), "cursor", None).unwrap(), 1);
        assert!(find(home.path(), &a.key).is_none());
        assert!(find(home.path(), &b.key).is_some());
    }
}
