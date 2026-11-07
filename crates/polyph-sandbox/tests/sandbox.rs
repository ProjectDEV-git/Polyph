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

/// Bubblewrap and the sandbox helper, or None (with a message) when this
/// machine cannot run them.
#[cfg(target_os = "linux")]
fn bubblewrap() -> Option<Vec<String>> {
    let helper = vec![env!("CARGO_BIN_EXE_polyph-sandbox-exec").to_string()];
    let bwrap = polyph_sandbox::detect(&helper).into_iter().find(|a| a.mechanism == Mechanism::Bubblewrap);
    if !bwrap.as_ref().is_some_and(|a| a.available) {
        eprintln!("skipped: {}", bwrap.map(|a| a.detail).unwrap_or_else(|| "bubblewrap is not installed".into()));
        return None;
    }
    Some(helper)
}

/// A repository at `<base>/repo` with one commit, and a runner for git in it.
#[cfg(target_os = "linux")]
fn repo(base: &std::path::Path) -> std::path::PathBuf {
    let repo = base.join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    for args in [
        &["init", "-q", "-b", "main"][..],
        &["config", "user.name", "Ann"],
        &["config", "user.email", "ann@example.invalid"],
        &["config", "commit.gpgSign", "false"],
        &["commit", "-q", "--allow-empty", "-m", "first"],
    ] {
        git(&repo, args);
    }
    repo
}

#[cfg(target_os = "linux")]
fn git(dir: &std::path::Path, args: &[&str]) -> String {
    let out = std::process::Command::new("git").arg("-C").arg(dir).args(args).output().unwrap();
    assert!(out.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&out.stderr));
    String::from_utf8_lossy(&out.stdout).into_owned()
}

/// Runs `script` with `sh` under bubblewrap in `worktree`, with the git
/// mounts `polyph_git::sandbox_git` gives it (committing on `branch`, if
/// any); returns what it printed.
#[cfg(target_os = "linux")]
async fn run_in_bwrap(
    helper: Vec<String>,
    base: &std::path::Path,
    worktree: &std::path::Path,
    branch: Option<&str>,
    script: &str,
) -> String {
    use std::sync::Arc;

    use polyph_sandbox::proxy::{Allowlist, Decider};

    let g = polyph_git::sandbox_git(worktree, branch);
    std::fs::create_dir_all(base.join("tmp")).unwrap();
    let deny: Decider = Arc::new(|_, _| Box::pin(async { false }));
    let proxy = polyph_sandbox::proxy::start(Allowlist::default(), deny, Some(base.join("run"))).await.unwrap();
    let policy = polyph_sandbox::Policy {
        worktree: worktree.to_path_buf(),
        writable: vec![],
        tmp: Some(base.join("tmp")),
        hidden: vec![],
        read_only: g.read_only,
        git_writable: g.writable,
        proxy_tcp: Some(proxy.tcp),
        proxy_unix: proxy.unix.clone(),
        helper,
        container_image: None,
    };
    let w = polyph_sandbox::wrap(Mechanism::Bubblewrap, &policy, "sh", &["-c".into(), script.into()]).unwrap();
    let out = std::process::Command::new(&w.program)
        .args(&w.args)
        .current_dir(&w.cwd)
        .envs(w.env.iter().cloned())
        .output()
        .unwrap();
    format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr))
}

/// SEC-13: an agent in the person's own checkout (no task) can read the
/// repository with git, but cannot write anything in `.git`: not its
/// settings, hooks or info, not a `commondir` naming a git directory of its
/// own, and not a commit.
#[cfg(target_os = "linux")]
#[tokio::test]
async fn bubblewrap_keeps_the_checkouts_git_directory_read_only() {
    let Some(helper) = bubblewrap() else { return };
    // Not under /tmp, which the sandbox replaces with an empty one.
    let t = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).unwrap();
    let base = t.path().canonicalize().unwrap();
    let repo = repo(&base);
    std::fs::write(repo.join("a.txt"), "one\n").unwrap();
    git(&repo, &["add", "a.txt"]);
    git(&repo, &["commit", "-q", "-m", "second"]);
    let dot = repo.join(".git");
    std::fs::create_dir_all(dot.join("hooks")).unwrap();
    std::fs::create_dir_all(dot.join("info")).unwrap();
    let config = std::fs::read(dot.join("config")).unwrap();
    let evil = base.join("evil");
    std::fs::create_dir_all(&evil).unwrap();

    let script = format!(
        "git config core.fsmonitor /x/evil.sh; echo '[core] fsmonitor = /x/evil.sh' >> .git/config; \
         echo '#!/bin/sh' > .git/hooks/pre-commit; echo '* filter=evil' > .git/info/attributes; \
         echo '{evil}' > .git/commondir; mv .git .old; mv .git/config .git/config.old; \
         echo two > a.txt; git status --porcelain && echo status-ok; git diff --stat && echo diff-ok; \
         echo work > work.txt && git add work.txt && git commit -q -m agent && echo committed",
        evil = evil.display()
    );
    let said = run_in_bwrap(helper, &base, &repo, None, &script).await;

    assert!(said.contains(" M a.txt") && said.contains("status-ok"), "git status must work: {said}");
    assert!(said.contains("a.txt | 2 +-") && said.contains("diff-ok"), "git diff must work: {said}");
    assert!(!said.contains("committed"), "the agent committed in the person's checkout: {said}");
    assert!(said.contains("index.lock': Read-only file system"), "the commit failed for another reason: {said}");
    assert_eq!(git(&repo, &["log", "--format=%s"]).lines().collect::<Vec<_>>(), ["second", "first"]);
    assert_eq!(std::fs::read(dot.join("config")).unwrap(), config, "the agent changed .git/config: {said}");
    assert!(!repo.join(".old").exists(), ".git was renamed: {said}");
    assert!(!dot.join("commondir").exists(), "the agent wrote .git/commondir: {said}");
    assert!(!dot.join("hooks/pre-commit").exists(), "the agent wrote a hook: {said}");
    assert!(!dot.join("info/attributes").exists(), "the agent wrote .git/info: {said}");
}

/// SEC-13: a task agent commits in its own worktree, but cannot create
/// `.git/commondir`, change its worktree's `commondir` or `gitdir`, or
/// write another worktree's folder or the shared settings.
#[cfg(target_os = "linux")]
#[tokio::test]
async fn bubblewrap_lets_a_task_agent_commit_and_nothing_more() {
    let Some(helper) = bubblewrap() else { return };
    // Not under /tmp, which the sandbox replaces with an empty one.
    let t = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).unwrap();
    let base = t.path().canonicalize().unwrap();
    let repo = repo(&base);
    let wts = repo.join(".polyph").join("worktrees");
    std::fs::create_dir_all(&wts).unwrap();
    let (t1, t2) = (wts.join("t1"), wts.join("t2"));
    git(&repo, &["worktree", "add", "-q", "-b", "polyph/t1", &t1.to_string_lossy(), "main"]);
    git(&repo, &["worktree", "add", "-q", "-b", "polyph/t2", &t2.to_string_lossy(), "main"]);
    let common = repo.join(".git");
    let (own, other) = (common.join("worktrees/t1"), common.join("worktrees/t2"));
    let config = std::fs::read(common.join("config")).unwrap();
    let evil = base.join("evil");
    std::fs::create_dir_all(&evil).unwrap();

    let script = format!(
        "echo work > work.txt && git add work.txt && git commit -q -m agent && echo committed; \
         git status --porcelain && echo status-ok; \
         echo '{evil}' > {common}/commondir; echo '{evil}' > {own}/commondir; rm -f {own}/commondir; \
         echo /x > {own}/gitdir; mv {own} {own}.old; echo '[core] x = 1' > {own}/config.worktree; \
         echo '{evil}' > {other}/commondir; echo '[core] x = 1' > {other}/config.worktree; \
         echo '[core] fsmonitor = /x' >> {common}/config; echo '#!/bin/sh' > {common}/hooks/pre-commit; \
         echo '* filter=evil' > {common}/info/attributes",
        evil = evil.display(),
        common = common.display(),
        own = own.display(),
        other = other.display(),
    );
    let said = run_in_bwrap(helper, &base, &t1, Some("polyph/t1"), &script).await;

    assert!(said.contains("committed"), "a task agent must be able to commit: {said}");
    assert!(said.contains("status-ok"), "{said}");
    assert_eq!(git(&repo, &["log", "--format=%s", "polyph/t1"]).lines().collect::<Vec<_>>(), ["agent", "first"]);
    assert!(!common.join("commondir").exists(), "the agent wrote .git/commondir: {said}");
    assert_eq!(std::fs::read_to_string(own.join("commondir")).unwrap().trim(), "../..", "{said}");
    assert_eq!(std::fs::read_to_string(other.join("commondir")).unwrap().trim(), "../..", "{said}");
    assert!(std::fs::read_to_string(own.join("gitdir")).unwrap().contains("t1"), "{said}");
    assert!(!common.join("worktrees/t1.old").exists(), "the worktree's folder was renamed: {said}");
    assert_eq!(std::fs::read(own.join("config.worktree")).unwrap(), b"", "{said}");
    assert!(!other.join("config.worktree").exists(), "the agent wrote another worktree's settings: {said}");
    assert_eq!(std::fs::read(common.join("config")).unwrap(), config, "the agent changed .git/config: {said}");
    assert!(!common.join("hooks/pre-commit").exists(), "the agent wrote a hook: {said}");
    assert!(!common.join("info/attributes").exists(), "the agent wrote .git/info: {said}");
    // The daemon's own view of the worktree still checks out.
    let main = polyph_git::Repo::open(&repo).unwrap();
    assert!(main.open_worktree(&t1).unwrap().dirty_files().unwrap().is_empty());
}

/// SEC-13: a task agent moves only its own branch. It cannot move `main`
/// (which the daemon merges into), another task's branch or their reflogs,
/// create a branch outside its own folder, or add `objects/info/alternates`.
#[cfg(target_os = "linux")]
#[tokio::test]
async fn bubblewrap_lets_a_task_agent_move_only_its_own_branch() {
    let Some(helper) = bubblewrap() else { return };
    let t = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).unwrap();
    let base = t.path().canonicalize().unwrap();
    let repo = repo(&base);
    let wts = repo.join(".polyph").join("worktrees");
    std::fs::create_dir_all(&wts).unwrap();
    let (t1, t2) = (wts.join("t1"), wts.join("t2"));
    git(&repo, &["worktree", "add", "-q", "-b", "polyph/t1", &t1.to_string_lossy(), "main"]);
    git(&repo, &["worktree", "add", "-q", "-b", "polyph/t2", &t2.to_string_lossy(), "main"]);
    let common = repo.join(".git");
    let main = git(&repo, &["rev-parse", "main"]);
    let main_log = std::fs::read(common.join("logs/refs/heads/main")).unwrap();
    let t2_log = std::fs::read(common.join("logs/refs/heads/polyph/t2")).unwrap();
    let outside = base.join("profile");
    std::fs::write(&outside, "export PATH\n").unwrap();

    let script = format!(
        "echo work > work.txt && git add work.txt && git commit -q -m agent && echo committed; \
         c=$(git rev-parse HEAD); \
         git update-ref refs/heads/main $c && echo moved-main; \
         echo $c > {common}/refs/heads/main && echo wrote-main; \
         git update-ref refs/heads/polyph/t2 $c && echo moved-t2; \
         rm -f {common}/refs/heads/polyph/t2 && echo removed-t2; \
         git branch other $c && echo made-other; \
         echo x >> {common}/logs/refs/heads/main && echo wrote-main-log; \
         ln -sf {outside} {common}/logs/refs/heads/polyph/t2 && echo linked-t2-log; \
         echo {base} > {common}/objects/info/alternates && echo wrote-alternates; \
         rm -rf {common}/objects/info && echo removed-info; true",
        common = common.display(),
        outside = outside.display(),
        base = base.display(),
    );
    let said = run_in_bwrap(helper, &base, &t1, Some("polyph/t1"), &script).await;

    assert!(said.contains("committed"), "a task agent must be able to commit on its branch: {said}");
    assert_eq!(git(&repo, &["log", "--format=%s", "polyph/t1"]).lines().collect::<Vec<_>>(), ["agent", "first"]);
    for bad in [
        "moved-main",
        "wrote-main",
        "moved-t2",
        "removed-t2",
        "made-other",
        "wrote-main-log",
        "linked-t2-log",
        "wrote-alternates",
        "removed-info",
    ] {
        assert!(!said.contains(bad), "{bad}: {said}");
    }
    assert_eq!(git(&repo, &["rev-parse", "main"]), main, "main moved: {said}");
    assert_eq!(git(&repo, &["rev-parse", "polyph/t2"]), main, "another task's branch moved: {said}");
    assert!(!common.join("refs/heads/other").exists(), "{said}");
    assert_eq!(std::fs::read(common.join("logs/refs/heads/main")).unwrap(), main_log, "{said}");
    assert_eq!(std::fs::read(common.join("logs/refs/heads/polyph/t2")).unwrap(), t2_log, "{said}");
    assert!(!common.join("objects/info/alternates").exists(), "{said}");
    assert_eq!(std::fs::read_to_string(&outside).unwrap(), "export PATH\n");
}

/// SEC-13: a task agent replaces its own branch's reflog, the one file of
/// that kind it can write, with a link to a file outside. The daemon's
/// next commit there is refused and the file stays as it was.
#[cfg(target_os = "linux")]
#[tokio::test]
async fn a_linked_reflog_stops_the_daemons_commit() {
    let Some(helper) = bubblewrap() else { return };
    let t = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).unwrap();
    let base = t.path().canonicalize().unwrap();
    let repo = repo(&base);
    let wts = repo.join(".polyph").join("worktrees");
    std::fs::create_dir_all(&wts).unwrap();
    let t1 = wts.join("t1");
    git(&repo, &["worktree", "add", "-q", "-b", "polyph/t1", &t1.to_string_lossy(), "main"]);
    let common = repo.join(".git");
    let outside = base.join("profile");
    std::fs::write(&outside, "export PATH\n").unwrap();

    let script = format!(
        "ln -sf {outside} {common}/logs/refs/heads/polyph/t1 && echo linked-branch-log; \
         ln -sf {outside} {common}/worktrees/t1/logs/HEAD && echo linked-head-log; echo work > work.txt",
        common = common.display(),
        outside = outside.display(),
    );
    let said = run_in_bwrap(helper, &base, &t1, Some("polyph/t1"), &script).await;
    assert!(said.contains("linked-branch-log") && said.contains("linked-head-log"), "{said}");

    let wt = polyph_git::Repo::open(&repo).unwrap().open_worktree(&t1).unwrap();
    let who = polyph_git::Signature::agent("Ann/test", "0");
    let err = wt.commit_task("polyph/t1", "agent work", &who).unwrap_err().to_string();
    assert!(err.contains("logs/refs/heads/polyph/t1") || err.contains("logs/HEAD"), "{err}");
    assert_eq!(std::fs::read_to_string(&outside).unwrap(), "export PATH\n");
    assert_eq!(git(&repo, &["log", "--format=%s", "polyph/t1"]).lines().collect::<Vec<_>>(), ["first"]);
}
