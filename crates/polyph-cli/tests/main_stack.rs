//! The `polyph` binary must not need a large main-thread stack. Windows gives
//! the main thread 1 MiB, and a debug build of the clap parser alone needs
//! more than that, so argument parsing and the command run on a thread with
//! its own stack. This test runs the binary with a 256 KiB main stack, which
//! Unix lets a test set with `ulimit -s`.
#![cfg(unix)]
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::process::{Command, Stdio};

fn polyph_small_stack(cwd: &std::path::Path, args: &[&str]) -> std::process::Output {
    Command::new("sh")
        .arg("-c")
        .arg("ulimit -s 256 && exec \"$0\" \"$@\"")
        .arg(env!("CARGO_BIN_EXE_polyph"))
        .args(args)
        .current_dir(cwd)
        .stdin(Stdio::null())
        .output()
        .unwrap()
}

#[test]
fn parses_and_runs_with_a_small_main_thread_stack() {
    let t = tempfile::tempdir().unwrap();
    let home = t.path().join("home");
    let home = home.to_str().unwrap();

    let out = polyph_small_stack(t.path(), &["--lang", "en", "--version"]);
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success(), "{err}");
    assert!(!err.contains("overflowed"), "{err}");

    // A command that gets as far as `run`: no polyph.toml here, so it fails
    // with its own message, not a crash.
    let out = polyph_small_stack(t.path(), &["--lang", "en", "--home", home, "trust", "--yes"]);
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(!out.status.success());
    assert!(!err.contains("overflowed"), "{err}");
    assert!(err.contains("polyph.toml"), "{err}");
}
