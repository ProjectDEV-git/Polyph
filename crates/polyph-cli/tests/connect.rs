//! `polyph connect` and `polyph start` write one entry per data directory and
//! never replace another home's entry. Every run uses a temporary HOME, never
//! the real one.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::Path;
use std::process::{Command, Stdio};

use serde_json::Value;

fn polyph(user: &Path, home: Option<&Path>, args: &[&str]) -> (bool, String) {
    let mut c = Command::new(env!("CARGO_BIN_EXE_polyph"));
    c.args(["--lang", "en"]);
    if let Some(h) = home {
        c.arg("--home").arg(h);
    }
    let out = c
        .args(args)
        .env("HOME", user)
        .env_remove("USERPROFILE")
        .env_remove("POLYPH_HOME")
        .env_remove("XDG_CONFIG_HOME")
        .env_remove("CODEX_HOME")
        .current_dir(user)
        .stdin(Stdio::null())
        .output()
        .unwrap();
    let text = format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
    (out.status.success(), text)
}

fn servers(user: &Path) -> serde_json::Map<String, Value> {
    let v: Value = serde_json::from_str(&std::fs::read_to_string(user.join(".claude.json")).unwrap()).unwrap();
    v["mcpServers"].as_object().unwrap().clone()
}

#[test]
fn a_temp_home_gets_its_own_entry_and_leaves_the_default_one() {
    let t = tempfile::tempdir().unwrap();
    let user = t.path().join("user");
    std::fs::create_dir_all(&user).unwrap();
    let existing = r#"{"mcpServers":{"polyph":{"type":"stdio","command":"/usr/bin/polyph","args":["mcp","--client","claude-code"]}}}"#;
    std::fs::write(user.join(".claude.json"), existing).unwrap();
    let temp_home = t.path().join("temp-home");

    let (ok, out) = polyph(&user, Some(&temp_home), &["connect", "claude-code", "--yes"]);
    assert!(ok, "{out}");
    let s = servers(&user);
    assert_eq!(s.len(), 2, "{s:?}");
    assert_eq!(s["polyph"]["command"], "/usr/bin/polyph");
    let own = s.keys().find(|k| *k != "polyph").unwrap();
    assert!(own.starts_with("polyph-"), "{own}");
    assert_eq!(s[own]["env"]["POLYPH_HOME"], temp_home.to_str().unwrap());
    // The summary names what it wrote.
    assert!(out.contains(&format!("Wrote: Claude Code ({own})")), "{out}");
}

#[test]
fn yes_does_not_replace_an_entry_for_another_home() {
    let t = tempfile::tempdir().unwrap();
    let user = t.path().join("user");
    std::fs::create_dir_all(&user).unwrap();
    let existing = r#"{"mcpServers":{"polyph":{"type":"stdio","command":"/dev/polyph","args":["mcp"],"env":{"POLYPH_HOME":"/tmp/some-other-home"}}}}"#;
    std::fs::write(user.join(".claude.json"), existing).unwrap();

    // The default home: no --home, no POLYPH_HOME, HOME is the temp user.
    let (ok, out) = polyph(&user, None, &["connect", "claude-code", "--yes"]);
    assert!(ok, "{out}");
    assert_eq!(std::fs::read_to_string(user.join(".claude.json")).unwrap(), existing);
    assert!(out.contains("left the existing \"polyph\" entry alone"), "{out}");
    assert!(out.contains("/tmp/some-other-home"), "{out}");
    assert!(!user.join(".claude.json.polyph-backup").exists());
}

#[test]
fn the_default_home_writes_polyph() {
    let t = tempfile::tempdir().unwrap();
    let user = t.path().join("user");
    std::fs::create_dir_all(&user).unwrap();
    // The default home is under this HOME's config directory on Linux and macOS.
    let default = if cfg!(target_os = "macos") {
        user.join("Library/Application Support/dev.polyph.polyph")
    } else if cfg!(windows) {
        return;
    } else {
        user.join(".config/polyph")
    };
    let (ok, out) = polyph(&user, Some(&default), &["connect", "claude-code", "--yes"]);
    assert!(ok, "{out}");
    let s = servers(&user);
    assert_eq!(s.keys().collect::<Vec<_>>(), ["polyph"]);
    assert!(s["polyph"].get("env").is_none(), "{s:?}");
    assert!(out.contains("Wrote: Claude Code (polyph)"), "{out}");
}
