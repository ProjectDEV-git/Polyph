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
#[cfg(any(target_os = "linux", target_os = "macos"))]
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

#[cfg(any(target_os = "linux", target_os = "macos"))]
fn git(dir: &std::path::Path, args: &[&str]) -> String {
    let out = std::process::Command::new("git").arg("-C").arg(dir).args(args).output().unwrap();
    assert!(out.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&out.stderr));
    String::from_utf8_lossy(&out.stdout).into_owned()
}

/// The daemon takes the commits of `branch` from the task worktree at
/// `worktree` into the shared object store.
#[cfg(any(target_os = "linux", target_os = "macos"))]
fn take_in(repo: &std::path::Path, worktree: &std::path::Path, branch: &str) {
    polyph_git::Repo::open(repo).unwrap().open_worktree(worktree).unwrap().import_branch(branch).unwrap();
}

/// A task worktree for `branch` at `<repo>/.polyph/worktrees/<name>`, made
/// by the daemon's own code (with its own object store).
#[cfg(target_os = "linux")]
fn task_worktree(repo: &std::path::Path, name: &str, branch: &str) -> std::path::PathBuf {
    let p = repo.join(".polyph").join("worktrees").join(name);
    polyph_git::Repo::open(repo).unwrap().add_worktree(&p, branch, "main").unwrap();
    p
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
    run_in(Mechanism::Bubblewrap, helper, base, worktree, branch, script).await
}

/// [`run_in_bwrap`] under `mech`.
#[cfg(any(target_os = "linux", target_os = "macos"))]
async fn run_in(
    mech: Mechanism,
    helper: Vec<String>,
    base: &std::path::Path,
    worktree: &std::path::Path,
    branch: Option<&str>,
    script: &str,
) -> String {
    run_with(mech, helper, base, worktree, branch, vec![], &[], script).await
}

/// [`run_in`] with `hidden` paths and extra variables in the environment
/// the sandbox is started with (as the daemon's own would be).
#[cfg(any(target_os = "linux", target_os = "macos"))]
#[allow(clippy::too_many_arguments)]
async fn run_with(
    mech: Mechanism,
    helper: Vec<String>,
    base: &std::path::Path,
    worktree: &std::path::Path,
    branch: Option<&str>,
    hidden: Vec<std::path::PathBuf>,
    env: &[(&str, &str)],
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
        hidden,
        read_only: g.read_only,
        git_writable: g.writable,
        proxy_tcp: Some(proxy.tcp),
        proxy_unix: proxy.unix.clone(),
        helper,
        container_image: None,
    };
    let mut w = polyph_sandbox::wrap(mech, &policy, "sh", &["-c".into(), script.into()]).unwrap();
    // As the daemon sets them for a task agent: its own object store.
    w.env.extend(g.env);
    // Started with descriptor 7 open and inheritable, as a daemon started
    // from a shell or service manager may have one (see `INHERITED_FD`).
    let secret = base.join("inherited-fd");
    std::fs::write(&secret, "inherited\n").unwrap();
    let out = std::process::Command::new("sh")
        .arg("-c")
        .arg("exec 7<\"$1\"; shift; exec \"$@\"")
        .arg("sh")
        .arg(&secret)
        .arg(&w.program)
        .args(&w.args)
        .current_dir(&w.cwd)
        .envs(env.iter().copied())
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
    git(&repo, &["worktree", "add", "-q", "-b", "polyph/t1/a", &t1.to_string_lossy(), "main"]);
    git(&repo, &["worktree", "add", "-q", "-b", "polyph/t2/b", &t2.to_string_lossy(), "main"]);
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
    let said = run_in_bwrap(helper, &base, &t1, Some("polyph/t1/a"), &script).await;

    assert!(said.contains("committed"), "a task agent must be able to commit: {said}");
    assert!(said.contains("status-ok"), "{said}");
    // The agent's objects are in its own store until the daemon takes them in.
    take_in(&repo, &t1, "polyph/t1/a");
    assert_eq!(git(&repo, &["log", "--format=%s", "polyph/t1/a"]).lines().collect::<Vec<_>>(), ["agent", "first"]);
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
    git(&repo, &["worktree", "add", "-q", "-b", "polyph/t1/a", &t1.to_string_lossy(), "main"]);
    git(&repo, &["worktree", "add", "-q", "-b", "polyph/t2/b", &t2.to_string_lossy(), "main"]);
    let common = repo.join(".git");
    let main = git(&repo, &["rev-parse", "main"]);
    let main_log = std::fs::read(common.join("logs/refs/heads/main")).unwrap();
    let t2_log = std::fs::read(common.join("logs/refs/heads/polyph/t2/b")).unwrap();
    let outside = base.join("profile");
    std::fs::write(&outside, "export PATH\n").unwrap();

    let script = format!(
        "echo work > work.txt && git add work.txt && git commit -q -m agent && echo committed; \
         c=$(git rev-parse HEAD); \
         git update-ref refs/heads/main $c && echo moved-main; \
         echo $c > {common}/refs/heads/main && echo wrote-main; \
         git update-ref refs/heads/polyph/t2/b $c && echo moved-t2; \
         rm -f {common}/refs/heads/polyph/t2/b && echo removed-t2; \
         git update-ref refs/heads/polyph/t1-other $c && echo made-polyph-other; \
         mkdir {common}/refs/heads/polyph/t3 && echo made-t3-folder; \
         git branch other $c && echo made-other; \
         echo x >> {common}/logs/refs/heads/main && echo wrote-main-log; \
         ln -sf {outside} {common}/logs/refs/heads/polyph/t2/b && echo linked-t2-log; \
         echo {base} > {common}/objects/info/alternates && echo wrote-alternates; \
         rm -rf {common}/objects/info && echo removed-info; \
         ln {outside} {common}/worktrees/t1/hard && echo hard-linked-git; \
         ln {outside} hard && echo hard-linked-worktree; \
         mv {common}/worktrees/t1/objects {common}/worktrees/t1/objects.old && echo moved-objects; true",
        common = common.display(),
        outside = outside.display(),
        base = base.display(),
    );
    let said = run_in_bwrap(helper, &base, &t1, Some("polyph/t1/a"), &script).await;
    assert!(said.contains("committed"), "a task agent must be able to commit on its branch: {said}");
    take_in(&repo, &t1, "polyph/t1/a");
    assert_eq!(git(&repo, &["log", "--format=%s", "polyph/t1/a"]).lines().collect::<Vec<_>>(), ["agent", "first"]);
    for bad in [
        "moved-main",
        "wrote-main",
        "moved-t2",
        "removed-t2",
        "made-polyph-other",
        "made-t3-folder",
        "made-other",
        "wrote-main-log",
        "linked-t2-log",
        "wrote-alternates",
        "removed-info",
        "hard-linked-git",
        "hard-linked-worktree",
        "moved-objects",
    ] {
        assert!(!said.contains(bad), "{bad}: {said}");
    }
    assert!(!common.join("worktrees/t1/objects.old").exists(), "its object store was renamed: {said}");
    assert_eq!(git(&repo, &["rev-parse", "main"]), main, "main moved: {said}");
    assert_eq!(git(&repo, &["rev-parse", "polyph/t2/b"]), main, "another task's branch moved: {said}");
    assert!(!common.join("refs/heads/other").exists(), "{said}");
    assert_eq!(std::fs::read(common.join("logs/refs/heads/main")).unwrap(), main_log, "{said}");
    assert_eq!(std::fs::read(common.join("logs/refs/heads/polyph/t2/b")).unwrap(), t2_log, "{said}");
    assert!(!common.join("refs/heads/polyph/t1-other").exists() && !common.join("refs/heads/polyph/t3").exists());
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
    git(&repo, &["worktree", "add", "-q", "-b", "polyph/t1/a", &t1.to_string_lossy(), "main"]);
    let common = repo.join(".git");
    let outside = base.join("profile");
    std::fs::write(&outside, "export PATH\n").unwrap();

    let script = format!(
        "ln -sf {outside} {common}/logs/refs/heads/polyph/t1/a && echo linked-branch-log; \
         ln -sf {outside} {common}/worktrees/t1/logs/HEAD && echo linked-head-log; echo work > work.txt",
        common = common.display(),
        outside = outside.display(),
    );
    let said = run_in_bwrap(helper, &base, &t1, Some("polyph/t1/a"), &script).await;
    assert!(said.contains("linked-branch-log") && said.contains("linked-head-log"), "{said}");

    let wt = polyph_git::Repo::open(&repo).unwrap().open_worktree(&t1).unwrap();
    let who = polyph_git::Signature::agent("Ann/test", "0");
    let err = wt.commit_task("polyph/t1/a", "agent work", &who).unwrap_err().to_string();
    assert!(err.contains("logs/refs/heads/polyph/t1/a") || err.contains("logs/HEAD"), "{err}");
    assert_eq!(std::fs::read_to_string(&outside).unwrap(), "export PATH\n");
    assert_eq!(git(&repo, &["log", "--format=%s", "polyph/t1/a"]).lines().collect::<Vec<_>>(), ["first"]);
}

/// Every file under `dir` with its content, for comparing before and after.
#[cfg(target_os = "linux")]
fn snapshot(dir: &std::path::Path) -> Vec<(std::path::PathBuf, Vec<u8>)> {
    let mut v = vec![];
    let mut todo = vec![dir.to_path_buf()];
    while let Some(d) = todo.pop() {
        for e in std::fs::read_dir(&d).unwrap() {
            let p = e.unwrap().path();
            if p.is_dir() {
                todo.push(p);
            } else {
                v.push((p.clone(), std::fs::read(&p).unwrap()));
            }
        }
    }
    v.sort();
    v
}

/// SEC-13: a task agent commits into its worktree's own object store. The
/// shared `objects/` is read-only to it: it cannot replace, add or delete a
/// loose object or a pack there, or commit into it directly. The daemon
/// then takes the agent's commits in.
#[cfg(target_os = "linux")]
#[tokio::test]
async fn bubblewrap_keeps_the_shared_object_store_read_only() {
    let Some(helper) = bubblewrap() else { return };
    let t = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).unwrap();
    let base = t.path().canonicalize().unwrap();
    let repo = repo(&base);
    std::fs::write(repo.join("a.txt"), "one\n").unwrap();
    git(&repo, &["add", "a.txt"]);
    git(&repo, &["commit", "-q", "-m", "second"]);
    git(&repo, &["repack", "-q"]);
    std::fs::write(repo.join("b.txt"), "loose\n").unwrap();
    git(&repo, &["add", "b.txt"]);
    git(&repo, &["commit", "-q", "-m", "third"]);
    let t1 = task_worktree(&repo, "t1", "polyph/t1/a");
    let objects = repo.join(".git").join("objects");
    let before = snapshot(&objects);
    assert!(before.iter().any(|(p, _)| p.to_string_lossy().ends_with(".pack")), "no pack to attack");

    let script = format!(
        "echo work > work.txt && git add work.txt && git commit -q -m agent && echo committed; \
         for f in {o}/??/*; do echo evil > $f && echo replaced-object; rm -f $f && echo removed-object; done; \
         for f in {o}/pack/*; do echo evil > $f && echo replaced-pack; rm -f $f && echo removed-pack; done; \
         echo x > {o}/pack/pack-0000000000000000000000000000000000000000.pack && echo added-pack; \
         mkdir {o}/zz && echo added-folder; \
         env -u GIT_OBJECT_DIRECTORY -u GIT_ALTERNATE_OBJECT_DIRECTORIES git commit -q --allow-empty -m shared \
           && echo committed-shared; \
         git -c gc.auto=0 prune && echo pruned; true",
        o = objects.display(),
    );
    let said = run_in_bwrap(helper, &base, &t1, Some("polyph/t1/a"), &script).await;

    assert!(said.contains("committed"), "a task agent must be able to commit: {said}");
    for bad in [
        "replaced-object",
        "removed-object",
        "replaced-pack",
        "removed-pack",
        "added-pack",
        "added-folder",
        "committed-shared",
    ] {
        assert!(!said.contains(bad), "{bad}: {said}");
    }
    assert!(snapshot(&objects) == before, "the shared object store changed: {said}");
    take_in(&repo, &t1, "polyph/t1/a");
    assert_eq!(
        git(&repo, &["log", "--format=%s", "polyph/t1/a"]).lines().collect::<Vec<_>>(),
        ["agent", "third", "second", "first"]
    );
    assert_eq!(git(&repo, &["show", "polyph/t1/a:work.txt"]), "work\n");
}

/// Seatbelt, or None (with a message) when this Mac cannot run it.
#[cfg(target_os = "macos")]
fn seatbelt() -> bool {
    let sb = polyph_sandbox::detect(&[]).into_iter().find(|a| a.mechanism == Mechanism::Seatbelt);
    if !sb.as_ref().is_some_and(|a| a.available) {
        eprintln!("skipped: {}", sb.map(|a| a.detail).unwrap_or_else(|| "sandbox-exec is missing".into()));
        return false;
    }
    true
}

/// SEC-13, as `bubblewrap_keeps_the_checkouts_git_directory_read_only`: an
/// agent in the person's own checkout reads with git but writes nothing in
/// `.git`. First run on the macOS CI runner.
#[cfg(target_os = "macos")]
#[tokio::test]
async fn seatbelt_keeps_the_checkouts_git_directory_read_only() {
    if !seatbelt() {
        return;
    }
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
    let said = run_in(Mechanism::Seatbelt, vec![], &base, &repo, None, &script).await;

    assert!(said.contains(" M a.txt") && said.contains("status-ok"), "git status must work: {said}");
    assert!(said.contains("a.txt | 2 +-") && said.contains("diff-ok"), "git diff must work: {said}");
    assert!(!said.contains("committed"), "the agent committed in the person's checkout: {said}");
    assert!(said.contains("index.lock"), "the commit failed for another reason: {said}");
    assert_eq!(git(&repo, &["log", "--format=%s"]).lines().collect::<Vec<_>>(), ["second", "first"]);
    assert_eq!(std::fs::read(dot.join("config")).unwrap(), config, "the agent changed .git/config: {said}");
    assert!(!repo.join(".old").exists(), ".git was renamed: {said}");
    assert!(!dot.join("commondir").exists(), "the agent wrote .git/commondir: {said}");
    assert!(!dot.join("hooks/pre-commit").exists(), "the agent wrote a hook: {said}");
    assert!(!dot.join("info/attributes").exists(), "the agent wrote .git/info: {said}");
}

/// SEC-13, as `bubblewrap_lets_a_task_agent_move_only_its_own_branch`: a
/// task agent under Seatbelt commits on its own branch, but cannot move
/// `main` or another task's branch, write their reflogs, add
/// `objects/info/alternates`, or hard-link a file from outside into the
/// folders it writes. First run on the macOS CI runner.
#[cfg(target_os = "macos")]
#[tokio::test]
async fn seatbelt_lets_a_task_agent_move_only_its_own_branch() {
    if !seatbelt() {
        return;
    }
    let t = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).unwrap();
    let base = t.path().canonicalize().unwrap();
    let repo = repo(&base);
    let wts = repo.join(".polyph").join("worktrees");
    std::fs::create_dir_all(&wts).unwrap();
    let (t1, t2) = (wts.join("t1"), wts.join("t2"));
    git(&repo, &["worktree", "add", "-q", "-b", "polyph/t1/a", &t1.to_string_lossy(), "main"]);
    git(&repo, &["worktree", "add", "-q", "-b", "polyph/t2/b", &t2.to_string_lossy(), "main"]);
    let common = repo.join(".git");
    let main = git(&repo, &["rev-parse", "main"]);
    let main_log = std::fs::read(common.join("logs/refs/heads/main")).unwrap();
    let t2_log = std::fs::read(common.join("logs/refs/heads/polyph/t2/b")).unwrap();
    let outside = base.join("profile");
    std::fs::write(&outside, "export PATH\n").unwrap();

    let script = format!(
        "echo work > work.txt && git add work.txt && git commit -q -m agent && echo committed; \
         c=$(git rev-parse HEAD); \
         git update-ref refs/heads/main $c && echo moved-main; \
         echo $c > {common}/refs/heads/main && echo wrote-main; \
         git update-ref refs/heads/polyph/t2/b $c && echo moved-t2; \
         rm -f {common}/refs/heads/polyph/t2/b && echo removed-t2; \
         git update-ref refs/heads/polyph/t1-other $c && echo made-polyph-other; \
         git branch other $c && echo made-other; \
         echo x >> {common}/logs/refs/heads/main && echo wrote-main-log; \
         echo {base} > {common}/objects/info/alternates && echo wrote-alternates; \
         ln {outside} {common}/worktrees/t1/hard && echo hard-linked-git; \
         ln {outside} {common}/refs/heads/polyph/t1/hard && echo hard-linked-refs; \
         ln {outside} hard && echo hard-linked-worktree; \
         echo mine > mine.txt && ln mine.txt mine-too.txt && echo linked-inside; \
         mv {common}/worktrees/t1/objects {common}/worktrees/t1/objects.old && echo moved-objects; true",
        common = common.display(),
        outside = outside.display(),
        base = base.display(),
    );
    let said = run_in(Mechanism::Seatbelt, vec![], &base, &t1, Some("polyph/t1/a"), &script).await;

    assert!(said.contains("committed"), "a task agent must be able to commit on its branch: {said}");
    assert!(said.contains("linked-inside"), "a hard link inside the worktree must work: {said}");
    take_in(&repo, &t1, "polyph/t1/a");
    assert_eq!(git(&repo, &["log", "--format=%s", "polyph/t1/a"]).lines().collect::<Vec<_>>(), ["agent", "first"]);
    for bad in [
        "moved-main",
        "wrote-main",
        "moved-t2",
        "removed-t2",
        "made-polyph-other",
        "made-other",
        "wrote-main-log",
        "wrote-alternates",
        "hard-linked-git",
        "hard-linked-refs",
        "hard-linked-worktree",
        "moved-objects",
    ] {
        assert!(!said.contains(bad), "{bad}: {said}");
    }
    assert_eq!(git(&repo, &["rev-parse", "main"]), main, "main moved: {said}");
    assert_eq!(git(&repo, &["rev-parse", "polyph/t2/b"]), main, "another task's branch moved: {said}");
    assert!(!common.join("refs/heads/other").exists(), "{said}");
    assert_eq!(std::fs::read(common.join("logs/refs/heads/main")).unwrap(), main_log, "{said}");
    assert_eq!(std::fs::read(common.join("logs/refs/heads/polyph/t2/b")).unwrap(), t2_log, "{said}");
    assert!(!common.join("objects/info/alternates").exists(), "{said}");
    assert_eq!(std::fs::read_to_string(&outside).unwrap(), "export PATH\n");
}

/// Variables naming the person's IPC endpoints, as a desktop session sets
/// them; none may reach an agent (SEC-14).
#[cfg(any(target_os = "linux", target_os = "macos"))]
const IPC_VARS: [(&str, &str); 18] = [
    ("DBUS_SESSION_BUS_ADDRESS", "unix:path=/run/user/1000/bus"),
    ("XDG_RUNTIME_DIR", "/run/user/1000"),
    ("SSH_AUTH_SOCK", "/tmp/ssh-polyph/agent.1"),
    ("GPG_AGENT_INFO", "/run/user/1000/gnupg/S.gpg-agent"),
    ("DISPLAY", ":0"),
    ("WAYLAND_DISPLAY", "wayland-0"),
    ("XAUTHORITY", "/home/ann/.Xauthority"),
    ("PULSE_SERVER", "unix:/run/user/1000/pulse/native"),
    ("PIPEWIRE_REMOTE", "pipewire-0"),
    ("KRB5CCNAME", "FILE:/tmp/krb5cc_1000"),
    ("DBUS_STARTER_ADDRESS", "unix:path=/run/user/1000/bus"),
    ("DBUS_SYSTEM_BUS_ADDRESS", "unix:path=/run/dbus/system_bus_socket"),
    ("VSCODE_IPC_HOOK_CLI", "/run/user/1000/vscode-ipc-1.sock"),
    ("KITTY_LISTEN_ON", "unix:/tmp/kitty-1"),
    ("WEZTERM_UNIX_SOCKET", "/run/user/1000/wezterm/gui-sock-1"),
    ("NVIM", "/run/user/1000/nvim.1.0"),
    ("GNOME_KEYRING_CONTROL", "/run/user/1000/keyring"),
    ("NOTIFY_SOCKET", "/run/systemd/notify"),
];

/// A script that tries each socket in `sockets` with the helper (printing
/// `<name>=<exit code>`, 3 when refused), and then does an agent's normal
/// work: a connected socket pair, the proxy (through which the Polyph MCP
/// server reaches the daemon), a write in the worktree and `git status`.
#[cfg(any(target_os = "linux", target_os = "macos"))]
fn ipc_script(sockets: &[(&str, String)]) -> String {
    let mut s = format!("H='{}'\n", env!("CARGO_BIN_EXE_polyph-sandbox-exec"));
    for (name, path) in sockets {
        s.push_str(&format!("\"$H\" probe unix '{path}'; echo {name}=$?\n"));
    }
    s.push_str("\"$H\" probe socketpair; echo pair=$?\n");
    s.push_str("\"$H\" probe connect \"${HTTP_PROXY#http://}\"; echo proxy=$?\n");
    let vars: Vec<&str> = IPC_VARS.iter().map(|(k, _)| *k).collect();
    s.push_str(&format!("env | grep -E '^({})=' | sed 's/^/leaked /'\n", vars.join("|")));
    s.push_str("echo work > work.txt && echo wrote\ngit status --porcelain && echo status-ok\n");
    s
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
fn assert_cut_off(said: &str, refused: &[&str]) {
    for name in refused {
        assert!(said.contains(&format!("{name}=3")), "{name} was reachable from the sandbox: {said}");
    }
    for ok in ["pair=0", "proxy=0", "wrote", "?? work.txt", "status-ok"] {
        assert!(said.contains(ok), "the agent's normal work failed ({ok}): {said}");
    }
    assert!(!said.contains("leaked"), "a variable naming host IPC reached the agent: {said}");
}

/// Lines for a script that report a descriptor inherited from outside the
/// sandbox ([`run_with`] starts it with descriptor 7 open) and the Unix
/// pairs that reach other sockets by path: datagram, and raw, which Linux
/// makes a datagram pair (SEC-14).
#[cfg(target_os = "linux")]
const INHERITED_FD: &str = "[ -e /proc/$$/fd/7 ] && echo fd-leaked; ls -l /proc/$$/fd | grep inherited-fd\n\
                            \"$H\" probe socketpair dgram; echo dgrampair=$?\n\
                            \"$H\" probe socketpair raw; echo rawpair=$?\n\
                            \"$H\" probe socketpair seqpacket; echo seqpair=$?\n";

#[cfg(target_os = "linux")]
fn assert_no_inherited_fd(said: &str) {
    assert!(!said.contains("fd-leaked") && !said.contains("inherited-fd"), "an inherited descriptor got in: {said}");
    assert!(said.contains("seqpair=0"), "seqpacket pairs must work: {said}");
}

/// A listening Unix socket at `path`, checked to be reachable from here.
#[cfg(any(target_os = "linux", target_os = "macos"))]
fn listen(path: &std::path::Path) -> std::os::unix::net::UnixListener {
    let l = std::os::unix::net::UnixListener::bind(path).unwrap();
    std::os::unix::net::UnixStream::connect(path).unwrap();
    l
}

/// A listening abstract Unix socket, checked to be reachable from here.
#[cfg(target_os = "linux")]
fn listen_abstract(name: &str) -> std::os::unix::net::UnixListener {
    use std::os::linux::net::SocketAddrExt;
    use std::os::unix::net::{SocketAddr, UnixListener, UnixStream};
    let a = SocketAddr::from_abstract_name(name).unwrap();
    let l = UnixListener::bind_addr(&a).unwrap();
    UnixStream::connect_addr(&a).unwrap();
    l
}

/// SEC-14: under bubblewrap the session bus in `/run/user/<uid>`, a
/// container socket elsewhere and abstract sockets (X11) are out of reach,
/// and the variables naming them are gone; the agent still writes its
/// worktree, runs git and reaches the proxy.
#[cfg(target_os = "linux")]
#[tokio::test]
async fn bubblewrap_cuts_the_agent_off_from_host_ipc() {
    let Some(helper) = bubblewrap() else { return };
    let t = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).unwrap();
    let base = t.path().canonicalize().unwrap();
    let repo = repo(&base);
    let mut sockets: Vec<(&str, String)> = vec![];
    // The session bus, where this machine lets a test make one: in /run as
    // root, or in $XDG_RUNTIME_DIR (/run/user/<uid>).
    let run = std::iter::once(std::path::PathBuf::from("/run"))
        .chain(std::env::var_os("XDG_RUNTIME_DIR").map(std::path::PathBuf::from))
        .find_map(|d| tempfile::Builder::new().prefix("polyph-ipc-").tempdir_in(d).ok());
    let _bus = run.as_ref().map(|d| {
        let p = d.path().join("bus");
        let l = listen(&p);
        sockets.push(("bus", p.to_string_lossy().into_owned()));
        l
    });
    if run.is_none() {
        eprintln!("note: neither /run nor $XDG_RUNTIME_DIR is writable here; the session bus check is skipped");
    }
    // A container engine's socket outside /run, hidden as `host_ipc_paths`
    // hides `~/.colima`, `$DOCKER_HOST` and the like.
    let docker = base.join("docker.sock");
    let _docker = listen(&docker);
    sockets.push(("docker", docker.to_string_lossy().into_owned()));
    let name = format!("polyph-ipc-bwrap-{}", std::process::id());
    let _x11 = listen_abstract(&name);
    sockets.push(("abstract", format!("@{name}")));

    let script = ipc_script(&sockets) + INHERITED_FD;
    let said = run_with(Mechanism::Bubblewrap, helper, &base, &repo, None, vec![docker], &IPC_VARS, &script).await;
    let names: Vec<&str> = sockets.iter().map(|(n, _)| *n).collect();
    assert_cut_off(&said, &names);
    assert_no_inherited_fd(&said);
}

/// SEC-14: in Landlock mode the seccomp filter refuses Unix sockets, path
/// and abstract alike (and from Landlock ABI 6 the scope refuses abstract
/// ones too), and datagram and raw pairs; the variables and inherited
/// descriptors are gone, and the agent's normal work runs.
#[cfg(target_os = "linux")]
#[tokio::test]
async fn landlock_cuts_the_agent_off_from_host_ipc() {
    let helper = vec![env!("CARGO_BIN_EXE_polyph-sandbox-exec").to_string()];
    let ll = polyph_sandbox::detect(&helper).into_iter().find(|a| a.mechanism == Mechanism::Landlock);
    if !ll.as_ref().is_some_and(|a| a.available) {
        eprintln!("skipped: {}", ll.map(|a| a.detail).unwrap_or_else(|| "no Landlock".into()));
        return;
    }
    assert!(ll.as_ref().is_some_and(|a| a.detail.contains("seccomp blocks")), "{ll:?}");
    let t = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).unwrap();
    let base = t.path().canonicalize().unwrap();
    let repo = repo(&base);
    let bus = base.join("bus");
    let _bus = listen(&bus);
    let name = format!("polyph-ipc-landlock-{}", std::process::id());
    let _x11 = listen_abstract(&name);
    let sockets = [("bus", bus.to_string_lossy().into_owned()), ("abstract", format!("@{name}"))];

    let script = ipc_script(&sockets) + INHERITED_FD;
    let said = run_with(Mechanism::Landlock, helper, &base, &repo, None, vec![], &IPC_VARS, &script).await;
    assert_cut_off(&said, &["bus", "abstract", "dgrampair", "rawpair"]);
    assert_no_inherited_fd(&said);
}

/// SEC-14 under Seatbelt: `open` and `osascript` do not run (nor a copy of
/// `open`, which LaunchServices would serve outside the profile), a Unix
/// socket outside is refused and the variables are gone; the agent still
/// writes its worktree, runs git and cargo, and uses a socket in its own
/// temp folder. Runs on the macOS CI runner.
#[cfg(target_os = "macos")]
#[tokio::test]
async fn seatbelt_cuts_the_agent_off_from_host_ipc() {
    if !seatbelt() {
        return;
    }
    let t = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).unwrap();
    let base = t.path().canonicalize().unwrap();
    let repo = repo(&base);
    let bus = base.join("bus");
    let _bus = listen(&bus);
    std::fs::create_dir_all(base.join("tmp")).unwrap();
    let own = base.join("tmp").join("own.sock");
    let _own = listen(&own);
    let escaped = base.join("escaped");
    let command = repo.join("x.command");
    std::fs::write(&command, format!("#!/bin/sh\ntouch '{}'\n", escaped.display())).unwrap();
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&command, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    let mut script = ipc_script(&[("bus", bus.to_string_lossy().into_owned())]);
    script.push_str(&format!(
        "\"$H\" probe unix '{own}'; echo own=$?\n\
         /usr/bin/open -a Terminal x.command; echo open=$?\n\
         cp /usr/bin/open ./o && ./o -a Terminal x.command; echo open-copy=$?\n\
         /usr/bin/osascript -e 'return 1'; echo osascript=$?\n\
         '{cargo}' --version && echo cargo-ok\n",
        own = own.display(),
        cargo = env!("CARGO"),
    ));
    let said = run_with(Mechanism::Seatbelt, vec![], &base, &repo, None, vec![], &IPC_VARS, &script).await;
    assert_cut_off(&said, &["bus"]);
    assert!(said.contains("own=0"), "a socket in the agent's own temp folder must work: {said}");
    assert!(said.contains("cargo-ok"), "cargo must run: {said}");
    for bad in ["open=0", "open-copy=0", "osascript=0"] {
        assert!(!said.contains(bad), "{bad}: {said}");
    }
    std::thread::sleep(std::time::Duration::from_secs(3));
    assert!(!escaped.exists(), "a script ran outside the sandbox through LaunchServices: {said}");
}
