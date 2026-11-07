//! Polyph's extension points, as plain data and files (no I/O beyond the
//! file system, no daemon types):
//!
//! - [`manifest`]: plugins, folders with a `polyph-plugin.toml` that give
//!   agents an MCP tool server and run hooks on room events. The daemon
//!   runs both inside the agent sandbox, with only the network hosts the
//!   manifest declares.
//! - [`store`]: plugins installed for one person on one device.
//! - [`skills`]: instruction folders (`<name>/SKILL.md`) that agents find
//!   through Polyph's MCP tools; shared with the repository in
//!   `.polyph/skills/`, or personal in the Polyph home.
//! - [`templates`]: starter `polyph.toml` and skills for a repository.

use std::path::{Component, Path};

pub mod manifest;
pub mod skills;
pub mod store;
pub mod templates;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{0}")]
    Invalid(String),
    #[error("{0}")]
    NotFound(String),
    #[error("{0}")]
    Exists(String),
    #[error("{0}")]
    Io(#[from] std::io::Error),
}

pub type Result<T> = std::result::Result<T, Error>;

/// Lower-case letters, digits and dashes, 1 to 40 characters, starting with a letter.
pub fn valid_name(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 40
        && s.starts_with(|c: char| c.is_ascii_lowercase())
        && s.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

/// Largest file the daemon reads from an agent's worktree.
pub const MAX_WORKTREE_READ: u64 = 1024 * 1024;

/// Reads `rel` under `root` (an agent's worktree) without following links.
/// Agents write their worktree, so a link there could make the daemon read
/// the person's own files into a prompt or a room (SEC-16). A symbolic link
/// on any part of `rel`, a path leaving `root`, or a file over `max` bytes is
/// an error.
pub fn read_under(root: &Path, rel: &Path, max: u64) -> std::io::Result<Vec<u8>> {
    use std::io::{Error, ErrorKind, Read};
    let refuse = |why: String| Error::new(ErrorKind::InvalidInput, why);
    let mut path = root.to_path_buf();
    let mut last = None;
    for c in rel.components() {
        let Component::Normal(c) = c else { return Err(refuse(format!("{} leaves the worktree", rel.display()))) };
        path.push(c);
        let m = std::fs::symlink_metadata(&path)?;
        if m.file_type().is_symlink() {
            return Err(refuse(format!("{} is a symbolic link", path.display())));
        }
        last = Some(m);
    }
    let Some(m) = last.filter(|m| m.is_file()) else { return Err(refuse(format!("{} is not a file", path.display()))) };
    let too_big = || Error::new(ErrorKind::FileTooLarge, format!("{} is over {max} bytes", path.display()));
    if m.len() > max {
        return Err(too_big());
    }
    let f = std::fs::File::open(&path)?;
    // A link swapped in after the checks above opens some other file.
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        let o = f.metadata()?;
        if (o.dev(), o.ino()) != (m.dev(), m.ino()) {
            return Err(refuse(format!("{} changed while being read", path.display())));
        }
    }
    // shortcut: Windows has no swap check after the link checks; add one (file IDs) if agents run unsandboxed there.
    let mut buf = vec![];
    f.take(max + 1).read_to_end(&mut buf)?;
    if buf.len() as u64 > max {
        return Err(too_big());
    }
    Ok(buf)
}

/// [`read_under`] as UTF-8 text, at most [`MAX_WORKTREE_READ`] bytes.
pub fn read_text_under(root: &Path, rel: &Path) -> std::io::Result<String> {
    String::from_utf8(read_under(root, rel, MAX_WORKTREE_READ)?)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    #[test]
    fn worktree_reads_refuse_links_and_big_files() {
        let d = tempfile::tempdir().unwrap();
        let (root, outside) = (d.path().join("wt"), d.path().join("secret"));
        std::fs::create_dir_all(root.join(".polyph")).unwrap();
        std::fs::create_dir_all(&outside).unwrap();
        std::fs::write(outside.join("brief.md"), "secret").unwrap();
        std::fs::write(root.join(".polyph/brief.md"), "ok").unwrap();
        assert_eq!(read_text_under(&root, Path::new(".polyph/brief.md")).unwrap(), "ok");
        assert!(read_text_under(&root, Path::new("../secret/brief.md")).is_err());
        assert!(read_text_under(&root, Path::new(".polyph")).is_err(), "a directory");
        std::fs::write(root.join("big"), vec![b'x'; 11]).unwrap();
        let e = read_under(&root, Path::new("big"), 10).unwrap_err();
        assert_eq!(e.kind(), std::io::ErrorKind::FileTooLarge);
        assert_eq!(read_under(&root, Path::new("big"), 11).unwrap().len(), 11);
        #[cfg(unix)]
        {
            use std::os::unix::fs::symlink;
            // A linked file pointing outside.
            std::fs::remove_file(root.join(".polyph/brief.md")).unwrap();
            symlink(outside.join("brief.md"), root.join(".polyph/brief.md")).unwrap();
            assert!(read_text_under(&root, Path::new(".polyph/brief.md")).is_err());
            // A linked parent folder.
            std::fs::remove_dir_all(root.join(".polyph")).unwrap();
            symlink(&outside, root.join(".polyph")).unwrap();
            assert!(read_text_under(&root, Path::new(".polyph/brief.md")).is_err());
        }
    }
}
