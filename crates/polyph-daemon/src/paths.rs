//! Where the daemon keeps its files. `POLYPH_HOME` overrides the platform
//! config directory (tests and portable installs use it).

use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct Paths {
    pub home: PathBuf,
}

impl Paths {
    pub fn from_env() -> anyhow::Result<Self> {
        if let Ok(h) = std::env::var("POLYPH_HOME") {
            return Ok(Self { home: PathBuf::from(h) });
        }
        let dirs = directories::ProjectDirs::from("dev", "polyph", "polyph")
            .ok_or_else(|| anyhow::anyhow!("could not determine a home directory; set POLYPH_HOME"))?;
        Ok(Self { home: dirs.config_dir().to_path_buf() })
    }

    pub fn at(home: impl Into<PathBuf>) -> Self {
        Self { home: home.into() }
    }

    pub fn ensure(&self) -> anyhow::Result<()> {
        std::fs::create_dir_all(self.rooms())?;
        restrict_dir(&self.home);
        Ok(())
    }

    pub fn rooms(&self) -> PathBuf {
        self.home.join("rooms")
    }
    pub fn room_file(&self, room_hex: &str) -> PathBuf {
        self.rooms().join(format!("{room_hex}.room"))
    }
    pub fn config(&self) -> PathBuf {
        self.home.join("config.toml")
    }
    pub fn token(&self) -> PathBuf {
        self.home.join("api.token")
    }
    pub fn daemon_info(&self) -> PathBuf {
        self.home.join("daemon.json")
    }
    pub fn identity(&self) -> PathBuf {
        self.home.join("identity.enc")
    }
    pub fn storage_key(&self) -> PathBuf {
        self.home.join("storage.key")
    }
    pub fn log(&self) -> PathBuf {
        self.home.join("daemon.log")
    }
    /// Project files this person trusted with `polyph trust`.
    pub fn trusted_projects(&self) -> PathBuf {
        self.home.join("trusted-projects.json")
    }
}

#[cfg(unix)]
fn restrict_dir(p: &Path) {
    use std::os::unix::fs::PermissionsExt;
    let _ = std::fs::set_permissions(p, std::fs::Permissions::from_mode(0o700));
}

#[cfg(not(unix))]
fn restrict_dir(_p: &Path) {}

/// Writes a file readable only by the current user (mode 0600 on Unix; on
/// Windows the file inherits the per-user profile directory's ACL, which
/// grants access to the user, SYSTEM and Administrators only).
pub fn write_private(path: &Path, data: &[u8]) -> std::io::Result<()> {
    let tmp = path.with_extension("tmp");
    #[cfg(unix)]
    {
        use std::io::Write;
        use std::os::unix::fs::OpenOptionsExt;
        let mut f = std::fs::OpenOptions::new().write(true).create(true).truncate(true).mode(0o600).open(&tmp)?;
        f.write_all(data)?;
        f.sync_all()?;
    }
    #[cfg(not(unix))]
    {
        std::fs::write(&tmp, data)?;
    }
    std::fs::rename(&tmp, path)
}
