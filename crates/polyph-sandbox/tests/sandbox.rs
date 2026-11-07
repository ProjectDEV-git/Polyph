//! Sandbox acceptance: on each platform, every available mechanism must
//! block writes outside the worktree and network access except through the
//! proxy. Mechanisms the machine cannot run are skipped with a message.
#![allow(clippy::unwrap_used)]

use polyph_sandbox::Mechanism;

#[tokio::test]
async fn available_mechanisms_block_escapes() {
    let helper = vec![env!("CARGO_BIN_EXE_polyph-sandbox-exec").to_string()];
    let reports = polyph_sandbox::selftest::run_all(helper).await.unwrap();
    let mut protected = 0;
    for r in &reports {
        if !r.available {
            eprintln!("skipped {:?}: {}", r.mechanism, r.detail);
            continue;
        }
        if r.mechanism == Mechanism::Container {
            eprintln!("skipped container mode: needs an image");
            continue;
        }
        for c in &r.checks {
            // Low integrity level isolates files, not the network: on Windows
            // only the proxy variables steer traffic (see docs/sandbox.md).
            if r.mechanism == Mechanism::WindowsJob && c.key == "sandbox-check-direct-network" {
                eprintln!("Windows: direct network access is not blocked; see docs/sandbox.md");
                continue;
            }
            assert!(c.passed, "{:?}: {} failed", r.mechanism, c.key);
        }
        protected += 1;
    }
    if protected == 0 {
        eprintln!("no sandbox mechanism could run on this machine; nothing was asserted");
    }
}

/// SEC-13: an agent in the person's own checkout can commit, but cannot
/// change the git settings, hooks or info that the person's git runs.
#[cfg(target_os = "linux")]
#[tokio::test]
async fn bubblewrap_keeps_the_checkouts_git_settings_read_only() {
    use std::process::Command;
    use std::sync::Arc;

    use polyph_sandbox::proxy::{Allowlist, Decider};

    let helper = vec![env!("CARGO_BIN_EXE_polyph-sandbox-exec").to_string()];
    let bwrap = polyph_sandbox::detect(&helper).into_iter().find(|a| a.mechanism == Mechanism::Bubblewrap);
    if !bwrap.as_ref().is_some_and(|a| a.available) {
        eprintln!("skipped: {}", bwrap.map(|a| a.detail).unwrap_or_else(|| "bubblewrap is not installed".into()));
        return;
    }
    let t = tempfile::tempdir().unwrap();
    let base = t.path().canonicalize().unwrap();
    let repo = base.join("repo");
    let git = |args: &[&str]| {
        let out = Command::new("git").arg("-C").arg(&repo).args(args).output().unwrap();
        assert!(out.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&out.stderr));
        String::from_utf8_lossy(&out.stdout).into_owned()
    };
    std::fs::create_dir_all(&repo).unwrap();
    git(&["init", "-q"]);
    for (k, v) in [("user.name", "Ann"), ("user.email", "ann@example.invalid"), ("commit.gpgSign", "false")] {
        git(&["config", k, v]);
    }
    git(&["commit", "-q", "--allow-empty", "-m", "first"]);
    let dot = repo.join(".git");
    std::fs::create_dir_all(dot.join("hooks")).unwrap();
    std::fs::create_dir_all(dot.join("info")).unwrap();
    let config = std::fs::read(dot.join("config")).unwrap();
    std::fs::create_dir_all(base.join("tmp")).unwrap();

    let deny: Decider = Arc::new(|_, _| Box::pin(async { false }));
    let proxy = polyph_sandbox::proxy::start(Allowlist::default(), deny, Some(base.join("run"))).await.unwrap();
    let policy = polyph_sandbox::Policy {
        worktree: repo.clone(),
        writable: vec![],
        tmp: Some(base.join("tmp")),
        hidden: vec![],
        read_only: vec![dot.join("config"), dot.join("hooks"), dot.join("info")],
        proxy_tcp: Some(proxy.tcp),
        proxy_unix: proxy.unix.clone(),
        helper: helper.clone(),
        container_image: None,
    };
    let script = "git config core.fsmonitor /x/evil.sh; echo '[core] fsmonitor = /x/evil.sh' >> .git/config; \
                  echo '#!/bin/sh' > .git/hooks/pre-commit; echo '* filter=evil' > .git/info/attributes; \
                  mv .git .old; mv .git/config .git/config.old; \
                  echo work > work.txt && git add work.txt && git commit -q -m agent && echo committed";
    let w = polyph_sandbox::wrap(Mechanism::Bubblewrap, &policy, "sh", &["-c".into(), script.into()]).unwrap();
    let out = Command::new(&w.program).args(&w.args).current_dir(&w.cwd).envs(w.env.iter().cloned()).output().unwrap();
    let said = format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));

    assert!(said.contains("committed"), "the agent must still be able to commit: {said}");
    assert_eq!(git(&["log", "--format=%s"]).lines().collect::<Vec<_>>(), ["agent", "first"]);
    assert_eq!(std::fs::read(dot.join("config")).unwrap(), config, "the agent changed .git/config: {said}");
    assert!(!repo.join(".old").exists(), ".git was renamed: {said}");
    assert!(!dot.join("hooks/pre-commit").exists(), "the agent wrote a hook: {said}");
    assert!(!dot.join("info/attributes").exists(), "the agent wrote .git/info: {said}");
}
