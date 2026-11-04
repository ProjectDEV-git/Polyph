//! Plugins installed for one person on one device, in `<polyph home>/plugins/<name>/`.

use std::path::{Path, PathBuf};

use crate::manifest::Manifest;
use crate::{Error, Result, valid_name};

/// Largest plugin folder copied on install.
pub const MAX_PLUGIN_BYTES: u64 = 20 * 1024 * 1024;

#[derive(Clone, Debug)]
pub struct Installed {
    pub manifest: Manifest,
    pub dir: PathBuf,
}

#[derive(Clone, Debug)]
pub struct Store {
    dir: PathBuf,
}

impl Store {
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        Self { dir: dir.into() }
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// Every valid installed plugin, by name. Broken ones are skipped.
    pub fn list(&self) -> Vec<Installed> {
        let Ok(rd) = std::fs::read_dir(&self.dir) else { return vec![] };
        let mut out: Vec<Installed> = rd
            .flatten()
            .filter(|e| e.path().is_dir())
            .filter_map(|e| Manifest::load(&e.path()).ok().map(|m| Installed { manifest: m, dir: e.path() }))
            .collect();
        out.sort_by(|a, b| a.manifest.name.cmp(&b.manifest.name));
        out
    }

    pub fn get(&self, name: &str) -> Option<Installed> {
        self.list().into_iter().find(|p| p.manifest.name == name)
    }

    /// Copies a plugin folder in. Refuses a name already installed unless `replace`.
    pub fn add(&self, from: &Path, replace: bool) -> Result<Installed> {
        let m = Manifest::load(from)?;
        let to = self.dir.join(&m.name);
        if to.exists() {
            if !replace {
                return Err(Error::Exists(format!("plugin {} is already installed; remove it first", m.name)));
            }
            std::fs::remove_dir_all(&to)?;
        }
        let mut budget = MAX_PLUGIN_BYTES;
        std::fs::create_dir_all(&self.dir)?;
        copy_dir(from, &to, &mut budget).inspect_err(|_| {
            let _ = std::fs::remove_dir_all(&to);
        })?;
        Ok(Installed { manifest: m, dir: to })
    }

    pub fn remove(&self, name: &str) -> Result<()> {
        if !valid_name(name) {
            return Err(Error::Invalid(format!("\"{name}\" is not a plugin name")));
        }
        let p = self.dir.join(name);
        if !p.is_dir() {
            return Err(Error::NotFound(format!("plugin {name} is not installed")));
        }
        std::fs::remove_dir_all(p)?;
        Ok(())
    }
}

/// Copies a folder (no symlinks, no `.git`), within a byte budget.
pub fn copy_dir(from: &Path, to: &Path, budget: &mut u64) -> Result<()> {
    std::fs::create_dir_all(to)?;
    for e in std::fs::read_dir(from)? {
        let e = e?;
        let name = e.file_name();
        if name == ".git" || name == "node_modules" {
            continue;
        }
        let ft = e.file_type()?;
        let dst = to.join(&name);
        if ft.is_symlink() {
            continue;
        } else if ft.is_dir() {
            copy_dir(&e.path(), &dst, budget)?;
        } else if ft.is_file() {
            let len = e.metadata()?.len();
            if len > *budget {
                return Err(Error::Invalid(format!("plugin is larger than {} MiB", MAX_PLUGIN_BYTES >> 20)));
            }
            *budget -= len;
            std::fs::copy(e.path(), &dst)?;
        }
    }
    Ok(())
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn add_list_remove() {
        let src = tempfile::tempdir().unwrap();
        std::fs::write(
            src.path().join(crate::manifest::FILE),
            "name = \"notes\"\nversion = \"1\"\n[mcp]\ncommand = \"sh\"\n",
        )
        .unwrap();
        std::fs::create_dir(src.path().join(".git")).unwrap();
        std::fs::write(src.path().join("server.sh"), "echo").unwrap();
        let home = tempfile::tempdir().unwrap();
        let s = Store::new(home.path().join("plugins"));
        s.add(src.path(), false).unwrap();
        assert!(s.add(src.path(), false).is_err());
        s.add(src.path(), true).unwrap();
        let l = s.list();
        assert_eq!(l.len(), 1);
        assert!(l[0].dir.join("server.sh").exists());
        assert!(!l[0].dir.join(".git").exists());
        s.remove("notes").unwrap();
        assert!(s.list().is_empty());
        assert!(s.remove("../x").is_err());
    }
}
