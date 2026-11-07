//! The daemon's own git writes, run in a sandbox (SEC-13).
//!
//! A task agent keeps running while the daemon commits its work, so it can
//! swap a file the daemon's git is about to write (a reflog) for a link to a
//! file of the person's, between the daemon's link check and git's write.
//! Here every git command that writes runs with only the folders it has to
//! write writable: bubblewrap on Linux, Seatbelt on macOS. Everything else,
//! the person's home included, is read-only, so a link leads nowhere git can
//! write. Without either (Landlock, Windows, container mode, or bubblewrap
//! that cannot create namespaces) commands run as before, with the link
//! check alone.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::OnceLock;

/// How this machine confines the daemon's git writes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Confinement {
    Bubblewrap,
    Seatbelt,
    /// No sandbox: only the link check stands between git and a planted link.
    None,
}

/// The confinement used for the daemon's git writes on this machine.
pub fn confinement() -> Confinement {
    static C: OnceLock<Confinement> = OnceLock::new();
    *C.get_or_init(detect)
}

fn detect() -> Confinement {
    if cfg!(target_os = "linux") {
        let works = Command::new("bwrap")
            .args(["--die-with-parent", "--ro-bind", "/", "/", "--dev", "/dev", "true"])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .is_ok_and(|s| s.success());
        if works {
            return Confinement::Bubblewrap;
        }
    }
    if cfg!(target_os = "macos") && Path::new("/usr/bin/sandbox-exec").exists() {
        return Confinement::Seatbelt;
    }
    Confinement::None
}

/// Folders that exist and are not links, with links in their path resolved.
fn real_dirs(paths: &[PathBuf]) -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = vec![];
    for p in paths {
        if std::fs::symlink_metadata(p).is_ok_and(|m| m.is_dir())
            && let Ok(c) = crate::canonical(p)
            && !v.contains(&c)
        {
            v.push(c);
        }
    }
    v
}

/// `c` rebuilt to run under `confinement()` with only `writable` writable,
/// and `read_only` (inside them) read-only again.
pub(crate) fn confine(c: Command, writable: &[PathBuf], read_only: &[PathBuf]) -> Command {
    confine_with(confinement(), c, writable, read_only)
}

pub(crate) fn confine_with(how: Confinement, c: Command, writable: &[PathBuf], read_only: &[PathBuf]) -> Command {
    let writable = real_dirs(writable);
    let read_only: Vec<PathBuf> = read_only
        .iter()
        .filter(|p| std::fs::symlink_metadata(p).is_ok_and(|m| !m.file_type().is_symlink()))
        .filter_map(|p| crate::canonical(p).ok())
        .filter(|p| writable.iter().any(|w| p.starts_with(w) && p != w))
        .collect();
    match how {
        Confinement::None => c,
        Confinement::Bubblewrap => {
            let mut a: Vec<OsString> =
                ["--die-with-parent", "--ro-bind", "/", "/", "--dev", "/dev"].iter().map(OsString::from).collect();
            for w in &writable {
                a.extend([OsString::from("--bind"), w.into(), w.into()]);
            }
            for r in &read_only {
                a.extend([OsString::from("--ro-bind"), r.into(), r.into()]);
            }
            a.push("--".into());
            rebuild(&c, "bwrap", a)
        }
        Confinement::Seatbelt => {
            let profile = seatbelt_profile(&writable, &read_only);
            rebuild(&c, "/usr/bin/sandbox-exec", vec!["-p".into(), profile.into()])
        }
    }
}

/// `prefix` then `c`'s own program and arguments, with its environment and
/// working directory.
fn rebuild(c: &Command, program: &str, prefix: Vec<OsString>) -> Command {
    let mut n = Command::new(program);
    n.args(prefix).arg(c.get_program()).args(c.get_args());
    for (k, v) in c.get_envs() {
        match v {
            Some(v) => n.env(k, v),
            None => n.env_remove(k),
        };
    }
    if let Some(d) = c.get_current_dir() {
        n.current_dir(d);
    }
    n
}

fn sb_quote(p: &Path) -> String {
    p.to_string_lossy().replace('\\', "\\\\").replace('"', "\\\"")
}

/// Seatbelt: writes only to `writable` (and `/dev/null`), never to
/// `read_only`; later rules win.
pub(crate) fn seatbelt_profile(writable: &[PathBuf], read_only: &[PathBuf]) -> String {
    let mut s = String::from("(version 1)\n(allow default)\n(deny file-write*)\n");
    s.push_str("(allow file-write* (literal \"/dev/null\") (regex #\"^/dev/fd/\"))\n");
    for w in writable {
        s.push_str(&format!("(allow file-write* (subpath \"{}\"))\n", sb_quote(w)));
    }
    for r in read_only {
        let f = if r.is_dir() { "subpath" } else { "literal" };
        s.push_str(&format!("(deny file-write* ({f} \"{}\"))\n", sb_quote(r)));
    }
    s
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    #[test]
    fn bubblewrap_binds_only_the_writable_folders() {
        let t = tempfile::tempdir().unwrap();
        let base = crate::canonical(t.path()).unwrap();
        let (w, ro, gone) = (base.join("w"), base.join("w/config"), base.join("missing"));
        std::fs::create_dir_all(&w).unwrap();
        std::fs::write(&ro, "").unwrap();
        let mut c = Command::new("git");
        c.args(["update-ref", "x"]).env("A", "1").env_remove("GIT_DIR").current_dir(&base);
        let n = confine_with(Confinement::Bubblewrap, c, &[w.clone(), gone], &[ro.clone(), base.join("outside")]);
        assert_eq!(n.get_program(), "bwrap");
        let args: Vec<String> = n.get_args().map(|a| a.to_string_lossy().into_owned()).collect();
        let s = |p: &Path| p.to_string_lossy().into_owned();
        let at = |a: &[String]| args.windows(a.len()).position(|x| x == a).unwrap_or_else(|| panic!("{a:?}: {args:?}"));
        let bind = at(&["--bind".into(), s(&w), s(&w)]);
        let ro_at = at(&["--ro-bind".into(), s(&ro), s(&ro)]);
        assert!(at(&["--ro-bind".into(), "/".into(), "/".into()]) < bind && bind < ro_at, "{args:?}");
        assert!(!args.iter().any(|a| a.ends_with("missing") || a.ends_with("outside")), "{args:?}");
        assert_eq!(&args[args.len() - 3..], ["git", "update-ref", "x"]);
        assert!(n.get_envs().any(|(k, v)| k == "A" && v == Some("1".as_ref())));
        assert!(n.get_envs().any(|(k, v)| k == "GIT_DIR" && v.is_none()));
        assert_eq!(n.get_current_dir(), Some(base.as_path()));

        let p = seatbelt_profile(&[w.clone()], &[ro.clone()]);
        let allow = p.find(&format!("(allow file-write* (subpath \"{}\"))", s(&w))).unwrap();
        let deny = p.find(&format!("(deny file-write* (literal \"{}\"))", s(&ro))).unwrap();
        assert!(p.starts_with("(version 1)\n(allow default)\n(deny file-write*)") && allow < deny, "{p}");
    }
}
