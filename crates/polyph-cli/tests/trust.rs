//! `polyph trust` run from a task worktree trusts the main checkout's
//! polyph.toml, the file the daemon reads, and never the worktree's copy.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::Path;
use std::process::{Command, Stdio};

fn git(dir: &Path, args: &[&str]) {
    let ok = Command::new("git")
        .args(["-c", "user.name=Test", "-c", "user.email=test@polyph.invalid"])
        .args(args)
        .current_dir(dir)
        .status()
        .unwrap()
        .success();
    assert!(ok, "git {args:?} failed");
}

fn polyph(home: &Path, cwd: &Path, args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_polyph"))
        .args(["--lang", "en", "--home"])
        .arg(home)
        .args(args)
        .current_dir(cwd)
        .stdin(Stdio::null())
        .output()
        .unwrap()
}

#[test]
fn trust_from_a_task_worktree_trusts_the_main_checkout() {
    let t = tempfile::tempdir().unwrap();
    let main = t.path().join("main");
    let home = t.path().join("home");
    std::fs::create_dir_all(&main).unwrap();
    git(&main, &["init", "-q", "-b", "main"]);
    git(&main, &["config", "core.autocrlf", "false"]);
    std::fs::write(main.join("polyph.toml"), "checks = [\"cargo test\"]\n").unwrap();
    git(&main, &["add", "-A"]);
    git(&main, &["commit", "-q", "-m", "initial"]);
    let wt = main.join(".polyph/worktrees/task");
    git(&main, &["worktree", "add", "-q", "-b", "task", wt.to_str().unwrap(), "main"]);
    // What an agent might write in its own worktree.
    std::fs::write(wt.join("polyph.toml"), "[sandbox]\nmode = \"off\"\n").unwrap();
    let sub = wt.join("src");
    std::fs::create_dir_all(&sub).unwrap();

    // Without a terminal and without --yes nothing is trusted.
    let out = polyph(&home, &sub, &["trust"]);
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("--yes"), "{}", String::from_utf8_lossy(&out.stderr));
    assert!(!home.join("trusted-projects.json").exists());

    // The review shows the main checkout's settings, not the worktree's.
    let out = polyph(&home, &sub, &["trust", "--yes"]);
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("checks = [\"cargo test\"]"), "{stdout}");
    assert!(!stdout.contains("mode = \"off\""), "{stdout}");

    let record: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(home.join("trusted-projects.json")).unwrap()).unwrap();
    let main_dir = polyph_git::canonical(&main).unwrap();
    let keys: Vec<&String> = record.as_object().unwrap().keys().collect();
    assert_eq!(keys, vec![&main_dir.to_string_lossy().into_owned()]);
}
