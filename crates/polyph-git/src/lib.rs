//! Git for Polyph, by running the `git` command (2.38 or newer).
//!
//! - Each agent task gets its own worktree and branch ([`Repo::add_worktree`]).
//! - Agents only write files; the daemon commits for them
//!   ([`Repo::commit_all`]), so sandboxes never need write access to `.git`.
//! - Merges are computed with `git merge-tree --write-tree` and committed with
//!   plumbing ([`Repo::merge`]), so no working copy is touched; conflicted
//!   files are handed to a resolver (see `polyph-merge`).

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::OnceLock;

use serde::Serialize;

#[derive(Debug, thiserror::Error)]
pub enum GitError {
    #[error("git is not installed or could not run: {0}")]
    Spawn(std::io::Error),
    #[error("{0} is not inside a git repository")]
    NotARepo(String),
    #[error("git {cmd} failed: {stderr}")]
    Failed { cmd: String, stderr: String },
    #[error("{0}")]
    Invalid(String),
}

pub type Result<T> = std::result::Result<T, GitError>;

/// Who a commit is attributed to.
#[derive(Debug, Clone)]
pub struct Signature {
    pub name: String,
    pub email: String,
}

impl Signature {
    /// The identity commits made for an agent carry.
    pub fn agent(label: &str, short_id: &str) -> Self {
        let local: String =
            label.chars().map(|c| if c.is_ascii_alphanumeric() { c.to_ascii_lowercase() } else { '-' }).collect();
        Self { name: format!("{label} (Polyph agent)"), email: format!("{local}.{short_id}@agents.polyph.invalid") }
    }
}

/// A conflict the resolver must answer: the three versions of one file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Conflict {
    pub path: String,
    pub base: Option<String>,
    pub ours: Option<String>,
    pub theirs: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum MergeOutcome {
    /// `theirs` was already contained in `ours`.
    UpToDate,
    /// The merge commit, and how each conflicted file was resolved.
    Merged { commit: String, resolved: Vec<String> },
    /// Files the resolver could not resolve; nothing was written.
    Conflicts(Vec<String>),
}

/// A working copy (the main one or a linked worktree).
#[derive(Debug, Clone)]
pub struct Repo {
    root: PathBuf,
    /// For an agent's worktree: the main `.git/worktrees`, which its `.git`
    /// file must point into before every command.
    worktrees: Option<PathBuf>,
}

/// `-c` settings that win over a repository's own config wherever it names
/// a program to run. Agents write their worktree and could write `.git`
/// (SEC-13); the daemon runs git with the person's rights.
fn guard_args() -> &'static [String] {
    static ARGS: OnceLock<Vec<String>> = OnceLock::new();
    ARGS.get_or_init(|| {
        // A file, so `<hooksPath>/<hook>` never exists.
        let hooks = if cfg!(windows) {
            std::env::current_exe().map(|p| p.to_string_lossy().into_owned()).unwrap_or_else(|_| "NUL".into())
        } else {
            "/dev/null".into()
        };
        // shortcut: only the global value is kept (GIT_SSH and system config are not), read those too if someone needs them.
        let ssh = global_config("core.sshCommand").unwrap_or_else(|| "ssh".into());
        let askpass = global_config("core.askPass").unwrap_or_default();
        [
            "core.fsmonitor=".to_string(),
            format!("core.hooksPath={hooks}"),
            format!("core.sshCommand={ssh}"),
            format!("core.askPass={askpass}"),
            "core.pager=cat".into(),
            "protocol.ext.allow=never".into(),
            "commit.gpgSign=false".into(),
        ]
        .into_iter()
        .flat_map(|kv| ["-c".to_string(), kv])
        .collect()
    })
}

/// The person's own setting for `key`, from their global config only.
fn global_config(key: &str) -> Option<String> {
    let out = Command::new("git")
        .args(["config", "--global", "--includes", "--get", key])
        .current_dir(std::env::temp_dir())
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    let v = String::from_utf8_lossy(&out.stdout).trim().to_string();
    (out.status.success() && !v.is_empty()).then_some(v)
}

/// Refuses a worktree whose `.git` file points anywhere but into the main
/// repository's `.git/worktrees`: an agent could aim it at a repository it
/// made, with config and hooks of its own (SEC-13).
fn check_gitdir(root: &Path, worktrees: &Path) -> Result<()> {
    let bad = |why: String| GitError::Invalid(format!("refusing to run git in {}: {why}", root.display()));
    let dot = root.join(".git");
    let m = std::fs::symlink_metadata(&dot).map_err(|e| bad(format!("cannot read .git: {e}")))?;
    if !m.is_file() || m.len() > 4096 {
        return Err(bad(".git is not a worktree's link file".into()));
    }
    let text = std::fs::read_to_string(&dot).map_err(|e| bad(format!("cannot read .git: {e}")))?;
    let dir = text.trim_end().strip_prefix("gitdir: ").ok_or_else(|| bad(".git has no gitdir line".into()))?;
    let dir = canonical(&root.join(dir)).map_err(|e| bad(format!("its gitdir {dir} is missing: {e}")))?;
    if dir.parent() != Some(worktrees) {
        return Err(bad(format!("its gitdir {} is not in {}", dir.display(), worktrees.display())));
    }
    Ok(())
}

fn run(dir: &Path, args: &[&str], env: &[(&str, &str)], stdin: Option<&[u8]>) -> Result<(bool, String, String)> {
    let mut c = Command::new("git");
    c.arg("-C").arg(dir).args(guard_args()).args(args);
    // Never prompt, never use a pager, and keep output stable.
    c.env("GIT_TERMINAL_PROMPT", "0").env("GIT_PAGER", "cat").env("LC_ALL", "C");
    for (k, v) in env {
        c.env(k, v);
    }
    c.stdin(if stdin.is_some() { Stdio::piped() } else { Stdio::null() });
    c.stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = c.spawn().map_err(GitError::Spawn)?;
    if let Some(data) = stdin
        && let Some(mut s) = child.stdin.take()
    {
        use std::io::Write;
        s.write_all(data).map_err(GitError::Spawn)?;
    }
    let out = child.wait_with_output().map_err(GitError::Spawn)?;
    Ok((
        out.status.success(),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    ))
}

impl Repo {
    /// Opens the working copy containing `path`.
    pub fn open(path: &Path) -> Result<Self> {
        let (ok, out, _) = run(path, &["rev-parse", "--show-toplevel"], &[], None)?;
        if !ok {
            return Err(GitError::NotARepo(path.display().to_string()));
        }
        Ok(Self { root: PathBuf::from(out.trim()), worktrees: None })
    }

    /// Opens an agent's worktree at `path`, a linked worktree of this
    /// repository. Every command first checks that its `.git` still points
    /// into this repository's `.git/worktrees`.
    pub fn open_worktree(&self, path: &Path) -> Result<Repo> {
        let common = self.common_dir()?;
        let worktrees = canonical(&common).map_err(GitError::Spawn)?.join("worktrees");
        check_gitdir(path, &worktrees)?;
        let mut r = Repo::open(path)?;
        r.worktrees = Some(worktrees);
        Ok(r)
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    fn exec(&self, args: &[&str], env: &[(&str, &str)], stdin: Option<&[u8]>) -> Result<(bool, String, String)> {
        if let Some(w) = &self.worktrees {
            check_gitdir(&self.root, w)?;
        }
        run(&self.root, args, env, stdin)
    }

    fn git(&self, args: &[&str]) -> Result<String> {
        self.git_env(args, &[], None)
    }

    fn git_env(&self, args: &[&str], env: &[(&str, &str)], stdin: Option<&[u8]>) -> Result<String> {
        let (ok, out, err) = self.exec(args, env, stdin)?;
        if ok {
            Ok(out)
        } else {
            Err(GitError::Failed {
                cmd: args.first().copied().unwrap_or_default().to_string(),
                stderr: err.trim().into(),
            })
        }
    }

    /// The shared `.git` directory (the same for every worktree).
    pub fn common_dir(&self) -> Result<PathBuf> {
        let p = PathBuf::from(self.git(&["rev-parse", "--git-common-dir"])?.trim());
        Ok(if p.is_absolute() { p } else { self.root.join(p) })
    }

    pub fn rev_parse(&self, rev: &str) -> Result<String> {
        Ok(self.git(&["rev-parse", "--verify", "--quiet", &format!("{rev}^{{commit}}")])?.trim().to_string())
    }

    /// The branch checked out here, if any.
    pub fn current_branch(&self) -> Option<String> {
        self.git(&["symbolic-ref", "--quiet", "--short", "HEAD"]).ok().map(|s| s.trim().to_string())
    }

    pub fn branch_exists(&self, branch: &str) -> bool {
        self.git(&["show-ref", "--verify", "--quiet", &format!("refs/heads/{branch}")]).is_ok()
    }

    /// Creates `branch` from `base` and checks it out in a new worktree at `path`.
    /// If the branch exists already, checks it out as it is.
    pub fn add_worktree(&self, path: &Path, branch: &str, base: &str) -> Result<Repo> {
        check_branch_name(branch)?;
        let p = path.to_string_lossy();
        if self.branch_exists(branch) {
            self.git(&["worktree", "add", &p, branch])?;
        } else {
            self.git(&["worktree", "add", "-b", branch, &p, base])?;
        }
        self.open_worktree(path)
    }

    /// Adds a detached worktree at `rev` (for running checks on a merge).
    pub fn add_detached(&self, path: &Path, rev: &str) -> Result<Repo> {
        self.git(&["worktree", "add", "--detach", &path.to_string_lossy(), rev])?;
        self.open_worktree(path)
    }

    pub fn remove_worktree(&self, path: &Path) -> Result<()> {
        self.git(&["worktree", "remove", "--force", &path.to_string_lossy()]).map(|_| ())
    }

    pub fn delete_branch(&self, branch: &str) -> Result<()> {
        self.git(&["branch", "-D", branch]).map(|_| ())
    }

    /// Worktree paths and their branches.
    pub fn worktrees(&self) -> Result<Vec<(PathBuf, Option<String>)>> {
        let out = self.git(&["worktree", "list", "--porcelain"])?;
        let mut v = vec![];
        for block in out.split("\n\n") {
            let mut path = None;
            let mut branch = None;
            for l in block.lines() {
                if let Some(p) = l.strip_prefix("worktree ") {
                    path = Some(PathBuf::from(p));
                } else if let Some(b) = l.strip_prefix("branch refs/heads/") {
                    branch = Some(b.to_string());
                }
            }
            if let Some(p) = path {
                v.push((p, branch));
            }
        }
        Ok(v)
    }

    /// Paths with uncommitted changes (including untracked files).
    pub fn dirty_files(&self) -> Result<Vec<String>> {
        let out = self.git(&["status", "--porcelain=v1", "-z", "--untracked-files=all"])?;
        let mut v = vec![];
        let mut parts = out.split('\0');
        while let Some(e) = parts.next() {
            if e.len() < 4 {
                continue;
            }
            v.push(e[3..].to_string());
            // Renames carry the old path next.
            if e.starts_with('R') || e.starts_with('C') {
                parts.next();
            }
        }
        Ok(v)
    }

    /// Commits every change in the working copy. Returns the new commit, or
    /// None when there was nothing to commit.
    pub fn commit_all(&self, message: &str, who: &Signature) -> Result<Option<String>> {
        if self.dirty_files()?.is_empty() {
            return Ok(None);
        }
        self.git(&["add", "-A"])?;
        let env = sig_env(who);
        let env: Vec<(&str, &str)> = env.iter().map(|(k, v)| (*k, v.as_str())).collect();
        self.git_env(&["commit", "--no-verify", "--quiet", "-F", "-"], &env, Some(message.as_bytes()))?;
        Ok(Some(self.rev_parse("HEAD")?))
    }

    pub fn merge_base(&self, a: &str, b: &str) -> Result<Option<String>> {
        match self.exec(&["merge-base", a, b], &[], None)? {
            (true, out, _) => Ok(Some(out.trim().to_string())),
            (false, _, err) if err.trim().is_empty() => Ok(None),
            (false, _, err) => Err(GitError::Failed { cmd: "merge-base".into(), stderr: err.trim().into() }),
        }
    }

    pub fn is_ancestor(&self, a: &str, b: &str) -> Result<bool> {
        Ok(self.exec(&["merge-base", "--is-ancestor", a, b], &[], None)?.0)
    }

    /// Files changed on `head` since it forked from `base`.
    pub fn changed_files(&self, base: &str, head: &str) -> Result<Vec<String>> {
        let out = self.git(&["diff", "--no-ext-diff", "--no-textconv", "--name-only", "-z", &format!("{base}...{head}")])?;
        Ok(out.split('\0').filter(|s| !s.is_empty()).map(str::to_string).collect())
    }

    /// Unified diff of `head` since it forked from `base`.
    pub fn diff(&self, base: &str, head: &str) -> Result<String> {
        self.git(&["diff", "--no-color", "--no-ext-diff", "--no-textconv", &format!("{base}...{head}")])
    }

    /// Commit subjects on `head` since it forked from `base`, oldest first.
    pub fn log_subjects(&self, base: &str, head: &str) -> Result<Vec<String>> {
        let out = self.git(&["log", "--reverse", "--format=%s", &format!("{base}..{head}")])?;
        Ok(out.lines().map(str::to_string).collect())
    }

    /// File content at a revision, None if it does not exist there.
    pub fn show(&self, rev: &str, path: &str) -> Option<String> {
        self.git(&["show", &format!("{rev}:{path}")]).ok()
    }

    /// Merges `theirs` into `ours` without a working copy. Conflicted files go
    /// to `resolve`, which returns the merged content or None to give up.
    pub fn merge(
        &self,
        ours: &str,
        theirs: &str,
        message: &str,
        who: &Signature,
        resolve: &mut dyn FnMut(&Conflict) -> Option<String>,
    ) -> Result<MergeOutcome> {
        let ours_c = self.rev_parse(ours)?;
        let theirs_c = self.rev_parse(theirs)?;
        if self.is_ancestor(&theirs_c, &ours_c)? {
            return Ok(MergeOutcome::UpToDate);
        }
        let (clean, out, err) =
            self.exec(&["merge-tree", "--write-tree", "--name-only", "--no-messages", &ours_c, &theirs_c], &[], None)?;
        let mut lines = out.lines();
        let tree = lines.next().unwrap_or_default().trim().to_string();
        if tree.len() < 40 {
            return Err(GitError::Failed { cmd: "merge-tree".into(), stderr: err.trim().into() });
        }
        let mut conflicted: Vec<String> = vec![];
        if !clean {
            for l in lines {
                if l.is_empty() {
                    break;
                }
                if !conflicted.iter().any(|c| c == l) {
                    conflicted.push(l.to_string());
                }
            }
        }
        let base = self.merge_base(&ours_c, &theirs_c)?;
        let mut final_tree = tree;
        let mut resolved = vec![];
        if !conflicted.is_empty() {
            let mut failed = vec![];
            let mut blobs = vec![];
            for path in &conflicted {
                let c = Conflict {
                    path: path.clone(),
                    base: base.as_deref().and_then(|b| self.show(b, path)),
                    ours: self.show(&ours_c, path),
                    theirs: self.show(&theirs_c, path),
                };
                match resolve(&c) {
                    Some(text) => {
                        let blob = self.git_env(&["hash-object", "-w", "--stdin"], &[], Some(text.as_bytes()))?;
                        blobs.push((path.clone(), blob.trim().to_string(), self.mode_of(&ours_c, path)));
                    }
                    None => failed.push(path.clone()),
                }
            }
            if !failed.is_empty() {
                return Ok(MergeOutcome::Conflicts(failed));
            }
            final_tree = self.replace_in_tree(&final_tree, &blobs)?;
            resolved = conflicted;
        }
        let env = sig_env(who);
        let env: Vec<(&str, &str)> = env.iter().map(|(k, v)| (*k, v.as_str())).collect();
        let commit = self
            .git_env(
                &["commit-tree", &final_tree, "-p", &ours_c, "-p", &theirs_c, "-F", "-"],
                &env,
                Some(message.as_bytes()),
            )?
            .trim()
            .to_string();
        Ok(MergeOutcome::Merged { commit, resolved })
    }

    fn mode_of(&self, rev: &str, path: &str) -> String {
        self.git(&["ls-tree", rev, "--", path])
            .ok()
            .and_then(|o| o.split_whitespace().next().map(str::to_string))
            .unwrap_or_else(|| "100644".into())
    }

    /// A copy of `tree` with some files replaced, built in a private index.
    fn replace_in_tree(&self, tree: &str, files: &[(String, String, String)]) -> Result<String> {
        let index = self.common_dir()?.join(format!("polyph-merge-index-{}", std::process::id()));
        let idx = index.to_string_lossy().into_owned();
        let env = [("GIT_INDEX_FILE", idx.as_str())];
        let r = (|| {
            self.git_env(&["read-tree", tree], &env, None)?;
            for (path, blob, mode) in files {
                self.git_env(&["update-index", "--add", "--cacheinfo", &format!("{mode},{blob},{path}")], &env, None)?;
            }
            Ok(self.git_env(&["write-tree"], &env, None)?.trim().to_string())
        })();
        let _ = std::fs::remove_file(&index);
        r
    }

    /// Moves `branch` to `commit` if it still points at `old`. When the branch
    /// is checked out in a clean worktree, that worktree is fast-forwarded too.
    pub fn advance_branch(&self, branch: &str, commit: &str, old: &str) -> Result<()> {
        check_branch_name(branch)?;
        for (path, b) in self.worktrees()? {
            if b.as_deref() == Some(branch) {
                let main = canonical(&path).ok() == canonical(&self.main_root()?).ok();
                let wt = if main { Repo::open(&path)? } else { self.open_worktree(&path)? };
                if !wt.dirty_files()?.is_empty() {
                    return Err(GitError::Invalid(format!(
                        "{branch} is checked out with uncommitted changes in {}",
                        path.display()
                    )));
                }
                if wt.rev_parse("HEAD")? != old {
                    return Err(GitError::Invalid(format!("{branch} moved while merging; try again")));
                }
                wt.git(&["merge", "--ff-only", "--quiet", commit])?;
                return Ok(());
            }
        }
        self.git(&["update-ref", &format!("refs/heads/{branch}"), commit, old]).map(|_| ())
    }
}

impl Repo {
    /// Pushes `branch` to `remote` (no force). Uses the person's own git
    /// credentials, so it runs in the daemon, never in an agent's sandbox.
    pub fn push(&self, remote: &str, branch: &str) -> Result<()> {
        check_branch_name(branch)?;
        if remote.starts_with('-') {
            return Err(GitError::Invalid(format!("not a valid remote: {remote}")));
        }
        self.git(&["push", "--quiet", remote, &format!("refs/heads/{branch}:refs/heads/{branch}")]).map(|_| ())
    }

    /// Files a merge commit brought into its first parent.
    pub fn merged_files(&self, merge_commit: &str) -> Result<Vec<String>> {
        let out = self.git(&[
            "diff",
            "--no-ext-diff",
            "--no-textconv",
            "--name-only",
            "-z",
            &format!("{merge_commit}^1"),
            merge_commit,
        ])?;
        Ok(out.split('\0').filter(|s| !s.is_empty()).map(str::to_string).collect())
    }

    /// The main working copy's root (the parent of the shared `.git`).
    pub fn main_root(&self) -> Result<PathBuf> {
        let common = self.common_dir()?;
        let common = canonical(&common).unwrap_or(common);
        match (common.file_name().and_then(|n| n.to_str()), common.parent()) {
            (Some(".git"), Some(p)) => Ok(p.to_path_buf()),
            _ => Ok(self.root.clone()),
        }
    }

    /// Fetches `remote` (all branches, pruning gone ones) without ever prompting.
    pub fn fetch(&self, remote: &str) -> Result<()> {
        if remote.starts_with('-') {
            return Err(GitError::Invalid(format!("not a valid remote: {remote}")));
        }
        self.git(&["fetch", "--quiet", "--prune", remote]).map(|_| ())
    }

    /// Files tracked in the index.
    pub fn file_count(&self) -> Result<usize> {
        Ok(self.git(&["ls-files", "-z"])?.split('\0').filter(|s| !s.is_empty()).count())
    }

    /// Commits `(ahead, behind)` of `head` relative to `upstream`.
    pub fn ahead_behind(&self, head: &str, upstream: &str) -> Result<(usize, usize)> {
        let out = self.git(&["rev-list", "--left-right", "--count", &format!("{head}...{upstream}")])?;
        let mut n = out.split_whitespace().map(|x| x.parse::<usize>().unwrap_or(0));
        Ok((n.next().unwrap_or(0), n.next().unwrap_or(0)))
    }

    /// The upstream of the checked-out branch (`origin/main`), if set.
    pub fn upstream(&self) -> Option<String> {
        self.git(&["rev-parse", "--abbrev-ref", "--symbolic-full-name", "@{upstream}"])
            .ok()
            .map(|s| s.trim().to_string())
    }

    /// Fetches `remote`, then moves the checked-out branch forward to its
    /// upstream when that loses nothing: the working copy is clean and the
    /// branch has no commits of its own. Otherwise reports and changes nothing.
    pub fn sync(&self, remote: &str) -> Result<SyncReport> {
        self.fetch(remote)?;
        let branch = self.current_branch();
        let upstream = self.upstream();
        let mut r =
            SyncReport { branch, upstream: upstream.clone(), behind: 0, ahead: 0, fast_forwarded: false, held: None };
        let Some(up) = upstream else { return Ok(r) };
        let (ahead, behind) = self.ahead_behind("HEAD", &up)?;
        r.ahead = ahead;
        r.behind = behind;
        if behind == 0 {
            return Ok(r);
        }
        if ahead > 0 {
            r.held = Some("diverged".into());
            return Ok(r);
        }
        if !self.dirty_files()?.is_empty() {
            r.held = Some("dirty".into());
            return Ok(r);
        }
        self.git(&["merge", "--ff-only", "--quiet", &up])?;
        r.fast_forwarded = true;
        Ok(r)
    }

    /// A remote's URL.
    pub fn remote_url(&self, remote: &str) -> Option<String> {
        self.git(&["remote", "get-url", "--", remote]).ok().map(|s| s.trim().to_string())
    }

    /// Pushes a branch to `remote` (no force), for a pull request.
    pub fn push_branch(&self, remote: &str, branch: &str) -> Result<()> {
        self.push(remote, branch)
    }

    /// Which of `paths` (relative to this working copy) git ignores.
    pub fn ignored(&self, paths: &[String]) -> Result<Vec<String>> {
        if paths.is_empty() {
            return Ok(vec![]);
        }
        let input: String = paths.iter().map(|p| format!("{p}\0")).collect();
        // Exit status 1 means "none ignored", so only stdout matters.
        let (_, out, _) = self.exec(&["check-ignore", "-z", "--stdin"], &[], Some(input.as_bytes()))?;
        Ok(out.split('\0').filter(|s| !s.is_empty()).map(str::to_string).collect())
    }

    /// Adds a line to `.git/info/exclude` unless it is there.
    pub fn exclude(&self, pattern: &str) -> Result<()> {
        let p = self.common_dir()?.join("info").join("exclude");
        let cur = std::fs::read_to_string(&p).unwrap_or_default();
        if cur.lines().any(|l| l.trim() == pattern) {
            return Ok(());
        }
        if let Some(d) = p.parent() {
            std::fs::create_dir_all(d).map_err(GitError::Spawn)?;
        }
        let sep = if cur.is_empty() || cur.ends_with('\n') { "" } else { "\n" };
        std::fs::write(&p, format!("{cur}{sep}{pattern}\n")).map_err(GitError::Spawn)
    }
}

fn sig_env(who: &Signature) -> Vec<(&'static str, String)> {
    vec![
        ("GIT_AUTHOR_NAME", who.name.clone()),
        ("GIT_AUTHOR_EMAIL", who.email.clone()),
        ("GIT_COMMITTER_NAME", who.name.clone()),
        ("GIT_COMMITTER_EMAIL", who.email.clone()),
    ]
}

/// A remote URL safe to share with a room: no user name and password or
/// token (`https://user:ghp_x@host/...`), no query or fragment. SSH URLs
/// keep their login name (`git@host:o/r.git`), which is not a secret. Local
/// paths and `file://` URLs give `None`: they mean nothing on another machine.
pub fn clean_remote(url: &str) -> Option<String> {
    let u = url.trim();
    if u.is_empty() || u.starts_with('-') || u.chars().any(|c| c.is_control() || c.is_whitespace()) {
        return None;
    }
    let u = u.split(['?', '#']).next().unwrap_or(u);
    if let Some((scheme, rest)) = u.split_once("://") {
        let scheme = scheme.to_ascii_lowercase();
        if !["https", "http", "ssh", "git"].contains(&scheme.as_str()) {
            return None;
        }
        let (authority, path) = rest.split_once('/').unwrap_or((rest, ""));
        let (user, host) = match authority.rsplit_once('@') {
            Some((user, host)) => (Some(user), host),
            None => (None, authority),
        };
        if host.is_empty() {
            return None;
        }
        // ssh://git@host/...: keep a plain login name, never a password.
        let host = match user {
            Some(user) if scheme == "ssh" && !user.contains(':') && !user.is_empty() => format!("{user}@{host}"),
            _ => host.to_string(),
        };
        return Some(format!("{scheme}://{host}/{path}"));
    }
    // scp-like: [user[:password]@]host:path (not a Windows drive letter or a local path).
    let head = u.split('/').next().unwrap_or(u);
    let (user, rest) = match head.rfind('@') {
        Some(i) => (Some(&u[..i]), &u[i + 1..]),
        None => (None, u),
    };
    let (host, path) = rest.split_once(':')?;
    if host.len() <= 1 || host.contains('\\') || path.is_empty() {
        return None;
    }
    Some(match user.filter(|u| !u.contains(':') && !u.is_empty()) {
        Some(user) => format!("{user}@{host}:{path}"),
        None => format!("{host}:{path}"),
    })
}

/// Clones `url` into `dir` with the person's own git login. `partial` makes
/// a blobless clone (`--filter=blob:none`): history now, file contents as
/// they are checked out, which is much faster for large repositories.
pub fn clone(url: &str, dir: &Path, branch: Option<&str>, partial: bool) -> Result<Repo> {
    let url = clean_remote(url).ok_or_else(|| GitError::Invalid(format!("not a remote URL: {url}")))?;
    clone_from(&url, dir, branch, partial)
}

/// [`clone`] without the check that `url` is a shareable remote (local
/// paths and `file://` work): for URLs the person typed themselves.
pub fn clone_from(url: &str, dir: &Path, branch: Option<&str>, partial: bool) -> Result<Repo> {
    if url.starts_with('-') {
        return Err(GitError::Invalid(format!("not a remote URL: {url}")));
    }
    let url = url.to_string();
    let parent = dir.parent().unwrap_or(Path::new("."));
    let mut args: Vec<String> = vec!["clone".into(), "--quiet".into()];
    if partial {
        args.push("--filter=blob:none".into());
    }
    if let Some(b) = branch {
        check_branch_name(b)?;
        args.extend(["--branch".into(), b.to_string()]);
    }
    args.extend(["--".into(), url, dir.to_string_lossy().into_owned()]);
    let refs: Vec<&str> = args.iter().map(String::as_str).collect();
    // Unlike other calls, cloning may ask for a password in the terminal.
    let mut c = Command::new("git");
    c.arg("-C").arg(parent).args(guard_args()).args(&refs).env("GIT_PAGER", "cat");
    let status = c.status().map_err(GitError::Spawn)?;
    if !status.success() {
        return Err(GitError::Failed { cmd: "clone".into(), stderr: format!("git clone exited with {status}") });
    }
    Repo::open(dir)
}

/// What [`Repo::sync`] did.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct SyncReport {
    pub branch: Option<String>,
    pub upstream: Option<String>,
    /// Commits the branch had to take in, and that are still only here.
    pub behind: usize,
    pub ahead: usize,
    /// The branch was moved forward to its upstream.
    pub fast_forwarded: bool,
    /// Why it was not moved forward, when it was behind.
    pub held: Option<String>,
}

#[cfg(test)]
mod remote_tests {
    use super::clean_remote;

    #[test]
    fn remotes_lose_their_secrets() {
        let c = |s: &str| clean_remote(s);
        assert_eq!(c("https://github.com/o/r.git").as_deref(), Some("https://github.com/o/r.git"));
        assert_eq!(c("https://bo:ghp_SECRET@github.com/o/r.git").as_deref(), Some("https://github.com/o/r.git"));
        assert_eq!(c("https://x-access-token:abc@github.com/o/r").as_deref(), Some("https://github.com/o/r"));
        assert_eq!(c("https://github.com/o/r.git?token=abc#x").as_deref(), Some("https://github.com/o/r.git"));
        assert_eq!(c("git@github.com:o/r.git").as_deref(), Some("git@github.com:o/r.git"));
        assert_eq!(c("git:pw@github.com:o/r.git").as_deref(), Some("github.com:o/r.git"));
        assert_eq!(c("ssh://git@host:22/o/r").as_deref(), Some("ssh://git@host:22/o/r"));
        assert_eq!(c("ssh://git:pw@host/o/r").as_deref(), Some("ssh://host/o/r"));
        assert_eq!(c("/home/me/repo"), None);
        assert_eq!(c("file:///home/me/repo"), None);
        assert_eq!(c("C:\\repo"), None);
        assert_eq!(c("--upload-pack=x"), None);
        assert_eq!(c("https://a b@h/r"), None);
    }
}

/// `owner/name` from a GitHub remote URL (https, ssh or scp-like).
pub fn github_slug(url: &str) -> Option<String> {
    let u = url.trim().trim_end_matches('/').trim_end_matches(".git");
    let rest = u
        .strip_prefix("git@github.com:")
        .or_else(|| u.strip_prefix("ssh://git@github.com/"))
        .or_else(|| u.split_once("github.com/").map(|(_, r)| r))?;
    let mut parts = rest.split('/');
    let (owner, name) = (parts.next()?, parts.next()?);
    (!owner.is_empty() && !name.is_empty() && parts.next().is_none()).then(|| format!("{owner}/{name}"))
}

/// Rejects names git would read as options or that `check-ref-format` refuses.
pub fn check_branch_name(b: &str) -> Result<()> {
    let bad = b.is_empty()
        || b.starts_with('-')
        || b.starts_with('/')
        || b.ends_with('/')
        || b.ends_with(".lock")
        || b.contains("..")
        || b.contains("@{")
        || b.chars().any(|c| c.is_control() || " ~^:?*[\\".contains(c));
    if bad { Err(GitError::Invalid(format!("not a valid branch name: {b}"))) } else { Ok(()) }
}

/// A branch name for a task: `polyph/<slug>-<short id>`.
pub fn task_branch(title: &str, short_id: &str) -> String {
    let mut slug = String::new();
    for c in title.chars() {
        if c.is_ascii_alphanumeric() {
            slug.push(c.to_ascii_lowercase());
        } else if !slug.ends_with('-') && !slug.is_empty() {
            slug.push('-');
        }
        if slug.len() >= 40 {
            break;
        }
    }
    let slug = slug.trim_end_matches('-');
    if slug.is_empty() { format!("polyph/task-{short_id}") } else { format!("polyph/{slug}-{short_id}") }
}

/// `path` made absolute with links resolved, in the form other programs
/// accept. On Windows `std::fs::canonicalize` returns verbatim paths
/// (`\\?\C:\...`), which git and many tools cannot use; this returns
/// `C:\...` (and `\\server\share\...` for network paths) instead.
pub fn canonical(path: &Path) -> std::io::Result<PathBuf> {
    Ok(simplify_verbatim(path.canonicalize()?))
}

fn simplify_verbatim(p: PathBuf) -> PathBuf {
    let s = p.to_string_lossy();
    if let Some(rest) = s.strip_prefix(r"\\?\UNC\") {
        return PathBuf::from(format!(r"\\{rest}"));
    }
    if let Some(rest) = s.strip_prefix(r"\\?\") {
        let b = rest.as_bytes();
        if b.len() >= 3 && b[0].is_ascii_alphabetic() && b[1] == b':' && b[2] == b'\\' {
            return PathBuf::from(rest);
        }
    }
    p
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    pub(crate) fn init() -> (tempfile::TempDir, Repo) {
        let d = tempfile::tempdir().unwrap();
        let r = Repo { root: d.path().to_path_buf(), worktrees: None };
        r.git(&["init", "-q", "-b", "main"]).unwrap();
        // Byte-exact merge results: no CRLF conversion on Windows checkouts.
        r.git(&["config", "core.autocrlf", "false"]).unwrap();
        std::fs::write(d.path().join("lib.rs"), "fn a() {}\n").unwrap();
        r.commit_all("initial", &Signature::agent("Ann/test", "0")).unwrap();
        let r = Repo::open(d.path()).unwrap();
        (d, r)
    }

    #[test]
    fn clone_partial_and_sync() {
        let (src, r) = init();
        r.git(&["config", "uploadpack.allowFilter", "true"]).unwrap();
        let bare = tempfile::tempdir().unwrap();
        let bare_path = bare.path().join("origin.git");
        run(src.path(), &["clone", "-q", "--bare", ".", &bare_path.to_string_lossy()], &[], None).unwrap();
        run(&bare_path, &["config", "uploadpack.allowFilter", "true"], &[], None).unwrap();
        let url = format!("file://{}", bare_path.to_string_lossy().replace('\\', "/"));
        let out = tempfile::tempdir().unwrap();
        let mine = clone_from(&url, &out.path().join("mine"), Some("main"), true).unwrap();
        assert_eq!(mine.git(&["config", "remote.origin.promisor"]).unwrap().trim(), "true", "blobless clone");
        // The clone follows the machine's git settings, which on Windows
        // runners check files out with CRLF line endings.
        let text = std::fs::read_to_string(mine.root().join("lib.rs")).unwrap();
        assert_eq!(text.replace("\r\n", "\n"), "fn a() {}\n");
        assert_eq!(mine.file_count().unwrap(), 1);
        // Up to date.
        let rep = mine.sync("origin").unwrap();
        assert_eq!((rep.ahead, rep.behind, rep.fast_forwarded), (0, 0, false));
        assert_eq!(rep.upstream.as_deref(), Some("origin/main"));
        // A teammate pushes: sync moves the branch forward.
        let theirs = clone_from(&url, &out.path().join("theirs"), None, false).unwrap();
        std::fs::write(theirs.root().join("b.rs"), "fn b() {}\n").unwrap();
        theirs.commit_all("add b", &Signature::agent("Bo/test", "1")).unwrap();
        theirs.push("origin", "main").unwrap();
        let rep = mine.sync("origin").unwrap();
        assert!(rep.fast_forwarded && rep.behind == 1, "{rep:?}");
        assert!(mine.root().join("b.rs").exists());
        // Uncommitted work holds it back; so do commits of its own.
        std::fs::write(theirs.root().join("c.rs"), "fn c() {}\n").unwrap();
        theirs.commit_all("add c", &Signature::agent("Bo/test", "1")).unwrap();
        theirs.push("origin", "main").unwrap();
        std::fs::write(mine.root().join("lib.rs"), "fn a() { 1 }\n").unwrap();
        let rep = mine.sync("origin").unwrap();
        assert_eq!(rep.held.as_deref(), Some("dirty"));
        assert!(!mine.root().join("c.rs").exists());
        mine.commit_all("mine", &Signature::agent("Ann/test", "0")).unwrap();
        let rep = mine.sync("origin").unwrap();
        assert_eq!((rep.held.as_deref(), rep.ahead, rep.behind), (Some("diverged"), 1, 1));
        assert!(
            clone("/local/path", &out.path().join("x"), None, false).is_err(),
            "local paths are not shared remotes"
        );
    }

    #[test]
    fn github_slugs() {
        for u in [
            "https://github.com/ProjectDEV-git/Polyph.git",
            "git@github.com:ProjectDEV-git/Polyph.git",
            "ssh://git@github.com/ProjectDEV-git/Polyph",
            "https://x:y@github.com/ProjectDEV-git/Polyph/",
        ] {
            assert_eq!(github_slug(u).as_deref(), Some("ProjectDEV-git/Polyph"), "{u}");
        }
        assert_eq!(github_slug("https://gitlab.com/a/b"), None);
    }

    #[test]
    fn names() {
        assert_eq!(task_branch("Add OAuth login!", "ab12"), "polyph/add-oauth-login-ab12");
        assert_eq!(task_branch("日本語", "ab12"), "polyph/task-ab12");
        assert!(check_branch_name("-rf").is_err() && check_branch_name("a..b").is_err());
        assert_eq!(Signature::agent("Bo/claude", "1f").email, "bo-claude.1f@agents.polyph.invalid");
    }

    #[test]
    fn worktree_commit_and_clean_merge() {
        let (d, r) = init();
        let wt_dir = tempfile::tempdir().unwrap();
        let wt_path = wt_dir.path().join("t1");
        let wt = r.add_worktree(&wt_path, "polyph/t1", "main").unwrap();
        assert_eq!(wt.current_branch().as_deref(), Some("polyph/t1"));
        assert!(wt.commit_all("nothing", &Signature::agent("A", "1")).unwrap().is_none());
        std::fs::write(wt_path.join("new.rs"), "fn n() {}\n").unwrap();
        let c = wt.commit_all("add new.rs", &Signature::agent("A", "1")).unwrap().unwrap();
        assert_eq!(r.changed_files("main", "polyph/t1").unwrap(), vec!["new.rs"]);
        assert!(r.diff("main", "polyph/t1").unwrap().contains("+fn n() {}"));
        // Main moved on in the meantime.
        std::fs::write(d.path().join("other.rs"), "x\n").unwrap();
        r.commit_all("other", &Signature::agent("B", "2")).unwrap();
        let old = r.rev_parse("main").unwrap();
        let out = r.merge("main", &c, "merge t1", &Signature::agent("Polyph", "q"), &mut |_| None).unwrap();
        let MergeOutcome::Merged { commit, resolved } = out else { panic!("{out:?}") };
        assert!(resolved.is_empty());
        // main is checked out and clean in the main working copy: it moves too.
        r.advance_branch("main", &commit, &old).unwrap();
        assert!(d.path().join("new.rs").exists());
        assert_eq!(
            r.merge("main", &c, "again", &Signature::agent("P", "q"), &mut |_| None).unwrap(),
            MergeOutcome::UpToDate
        );
        assert_eq!(r.merged_files(&commit).unwrap(), vec!["new.rs"]);
        assert_eq!(wt.main_root().unwrap(), canonical(d.path()).unwrap());
        r.remove_worktree(&wt_path).unwrap();
        assert_eq!(r.worktrees().unwrap().len(), 1);
        // Push to a bare remote.
        let remote = tempfile::tempdir().unwrap();
        run(remote.path(), &["init", "-q", "--bare"], &[], None).unwrap();
        r.git(&["remote", "add", "origin", &remote.path().to_string_lossy()]).unwrap();
        r.push("origin", "main").unwrap();
        assert!(r.push("--force", "main").is_err());
        r.exclude("/.polyph/").unwrap();
        r.exclude("/.polyph/").unwrap();
        let ex = std::fs::read_to_string(r.common_dir().unwrap().join("info/exclude")).unwrap();
        assert_eq!(ex.matches("/.polyph/").count(), 1);
        assert_eq!(r.ignored(&[".polyph/x".into(), "src/a.rs".into()]).unwrap(), vec![".polyph/x"]);
    }

    #[test]
    fn conflicts_go_to_the_resolver() {
        let (d, r) = init();
        r.git(&["branch", "side"]).unwrap();
        std::fs::write(d.path().join("lib.rs"), "fn a() { 1 }\n").unwrap();
        r.commit_all("ours", &Signature::agent("A", "1")).unwrap();
        r.git(&["checkout", "-q", "side"]).unwrap();
        std::fs::write(d.path().join("lib.rs"), "fn a() { 2 }\n").unwrap();
        r.commit_all("theirs", &Signature::agent("B", "2")).unwrap();
        r.git(&["checkout", "-q", "main"]).unwrap();
        let mut seen = None;
        let out = r
            .merge("main", "side", "m", &Signature::agent("P", "q"), &mut |c| {
                seen = Some(c.clone());
                None
            })
            .unwrap();
        assert_eq!(out, MergeOutcome::Conflicts(vec!["lib.rs".into()]));
        let c = seen.unwrap();
        assert_eq!(
            (c.base.as_deref(), c.ours.as_deref(), c.theirs.as_deref()),
            (Some("fn a() {}\n"), Some("fn a() { 1 }\n"), Some("fn a() { 2 }\n"))
        );
        let out = r
            .merge("main", "side", "m", &Signature::agent("P", "q"), &mut |_| Some("fn a() { 1 + 2 }\n".into()))
            .unwrap();
        let MergeOutcome::Merged { commit, resolved } = out else { panic!() };
        assert_eq!(resolved, vec!["lib.rs"]);
        assert_eq!(r.show(&commit, "lib.rs").unwrap(), "fn a() { 1 + 2 }\n");
        // The working copy was not touched.
        assert_eq!(std::fs::read_to_string(d.path().join("lib.rs")).unwrap(), "fn a() { 1 }\n");
    }

    #[test]
    fn verbatim_paths_are_simplified() {
        assert_eq!(simplify_verbatim(PathBuf::from(r"\\?\C:\w\repo")), PathBuf::from(r"C:\w\repo"));
        assert_eq!(simplify_verbatim(PathBuf::from(r"\\?\UNC\srv\share\x")), PathBuf::from(r"\\srv\share\x"));
        assert_eq!(simplify_verbatim(PathBuf::from(r"\\?\Volume{1}\x")), PathBuf::from(r"\\?\Volume{1}\x"));
        assert_eq!(simplify_verbatim(PathBuf::from("/w/repo")), PathBuf::from("/w/repo"));
    }

    #[cfg(unix)]
    #[test]
    fn repository_config_runs_no_programs() {
        use std::os::unix::fs::PermissionsExt;
        let (d, r) = init();
        let m = tempfile::tempdir().unwrap();
        let marker = m.path().join("ran");
        let script = m.path().join("evil.sh");
        std::fs::write(&script, format!("#!/bin/sh\necho \"$0\" >> '{}'\n", marker.display())).unwrap();
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
        let s = script.to_string_lossy();
        r.git(&["config", "core.fsmonitor", &s]).unwrap();
        r.git(&["config", "diff.external", &s]).unwrap();
        r.git(&["config", "diff.evil.textconv", &s]).unwrap();
        r.git(&["config", "core.pager", &s]).unwrap();
        let hooks = r.common_dir().unwrap().join("hooks");
        std::fs::create_dir_all(&hooks).unwrap();
        for h in ["pre-commit", "commit-msg", "post-commit", "post-index-change", "reference-transaction"] {
            std::fs::copy(&script, hooks.join(h)).unwrap();
        }
        // Plain git does run them, so the test can see the difference.
        Command::new("git").arg("-C").arg(d.path()).args(["status", "--porcelain"]).output().unwrap();
        assert!(marker.exists(), "plain git status ran no fsmonitor");
        std::fs::remove_file(&marker).unwrap();

        std::fs::write(d.path().join(".gitattributes"), "*.rs diff=evil\n").unwrap();
        std::fs::write(d.path().join("lib.rs"), "fn a() { 1 }\n").unwrap();
        assert!(!r.dirty_files().unwrap().is_empty());
        r.commit_all("change", &Signature::agent("A", "1")).unwrap().unwrap();
        assert!(r.diff("HEAD~1", "HEAD").unwrap().contains("+fn a() { 1 }"));
        assert_eq!(r.changed_files("HEAD~1", "HEAD").unwrap().len(), 2);
        let wt_dir = tempfile::tempdir().unwrap();
        let wt = r.add_worktree(&wt_dir.path().join("t1"), "polyph/t1", "main").unwrap();
        std::fs::write(wt.root().join("n.rs"), "x\n").unwrap();
        wt.commit_all("n", &Signature::agent("A", "1")).unwrap().unwrap();
        assert!(!marker.exists(), "ran: {}", std::fs::read_to_string(&marker).unwrap_or_default());
    }

    #[test]
    fn forged_gitdir_is_refused() {
        let (_d, r) = init();
        let wt_dir = tempfile::tempdir().unwrap();
        let p = wt_dir.path().join("t1");
        let wt = r.add_worktree(&p, "polyph/t1", "main").unwrap();
        assert!(wt.dirty_files().unwrap().is_empty());
        // The agent points its `.git` at a repository of its own.
        let evil = wt_dir.path().join("evil.git");
        run(wt_dir.path(), &["init", "-q", "--bare", &evil.to_string_lossy()], &[], None).unwrap();
        std::fs::write(p.join(".git"), format!("gitdir: {}\n", evil.display())).unwrap();
        let e = r.open_worktree(&p).unwrap_err().to_string();
        assert!(e.contains("refusing to run git"), "{e}");
        assert!(wt.dirty_files().is_err(), "an open worktree checks again");
        // A `.git` folder instead of the link file.
        std::fs::remove_file(p.join(".git")).unwrap();
        std::fs::create_dir(p.join(".git")).unwrap();
        assert!(r.open_worktree(&p).is_err());
        assert!(r.open_worktree(r.root()).is_err(), "the main working copy is not a linked worktree");
    }
}
