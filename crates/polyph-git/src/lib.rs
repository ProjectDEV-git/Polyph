//! Git for Polyph, by running the `git` command (2.38 or newer).
//!
//! - Each agent task gets its own worktree and branch ([`Repo::add_worktree`]).
//! - The daemon commits an agent's work for it ([`Repo::commit_task`]). A
//!   task agent may also commit in its own worktree, into an object store of
//!   its own; the sandbox keeps the rest of `.git` read-only
//!   ([`sandbox_git`]), and the daemon copies the task's commits into the
//!   shared store with git checking every object ([`Repo::import_branch`]).
//! - The daemon's own git writes run in a sandbox where only the folders
//!   they write are writable ([`confine`]).
//! - Merges are computed with `git merge-tree --write-tree` and committed with
//!   plumbing ([`Repo::merge`]), so no working copy is touched; conflicted
//!   files are handed to a resolver (see `polyph-merge`).

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::OnceLock;

use serde::Serialize;

pub mod confine;

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
    /// Whose configuration `fetch` and `push` use. Local commands always
    /// run with [`guard_args`].
    network: Trust,
}

/// Whose configuration a git command may use to run programs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Trust {
    /// Only the person wrote this repository's config, hooks and info: used
    /// as they are (per-repository `core.sshCommand`, `includeIf`, hooks
    /// such as Git LFS's pre-push).
    Person,
    /// An agent could have written them: run with [`guard_args`].
    Agent,
}

/// `-c` settings that win over a repository's own config wherever it names
/// a program to run. Agents write their worktree and, without a task, the
/// `.git` of the person's own checkout (SEC-13); the daemon runs git with
/// the person's rights. Settings git keeps the first value of
/// (`core.gitProxy`, `remote.*.uploadpack`) are handled where the command
/// is built; settings under names no list can cover (`filter.*`,
/// `merge.*.driver`) are not handled here: see [`sandbox_git`] and
/// `docs/security.md`.
fn guard_args() -> &'static [String] {
    static ARGS: OnceLock<Vec<String>> = OnceLock::new();
    ARGS.get_or_init(|| {
        guard_settings(&|k| std::env::var_os(k), &person_config)
            .into_iter()
            .flat_map(|kv| ["-c".to_string(), kv])
            .collect()
    })
}

/// The settings behind [`guard_args`], from the daemon's environment
/// (`env`) and the person's system and global config (`config`, which
/// returns `git config` output lines for the given arguments).
fn guard_settings(
    env: &dyn Fn(&str) -> Option<std::ffi::OsString>,
    config: &dyn Fn(&[&str]) -> Vec<String>,
) -> Vec<String> {
    // A file, so `<hooksPath>/<hook>` never exists.
    let hooks = if cfg!(windows) {
        std::env::current_exe().map(|p| p.to_string_lossy().into_owned()).unwrap_or_else(|_| "NUL".into())
    } else {
        "/dev/null".into()
    };
    let mut v = vec![
        "core.fsmonitor=".to_string(),
        format!("core.hooksPath={hooks}"),
        "core.pager=cat".into(),
        "protocol.ext.allow=never".into(),
        "commit.gpgSign=false".into(),
    ];
    // GIT_SSH_COMMAND wins over any config, so it needs no override.
    if env("GIT_SSH_COMMAND").is_none() {
        let ssh = config(&["--get-all", "core.sshCommand"]).pop().or_else(|| {
            // GIT_SSH ranks below core.sshCommand, so carry it over (quoted:
            // core.sshCommand goes through the shell, GIT_SSH does not).
            env("GIT_SSH").map(|p| format!("'{}'", p.to_string_lossy().replace('\'', r"'\''")))
        });
        v.push(format!("core.sshCommand={}", ssh.unwrap_or_else(|| "ssh".into())));
    }
    // GIT_ASKPASS wins over any config. Otherwise keep the person's value,
    // or SSH_ASKPASS, which git falls back to when core.askPass is unset.
    // With neither, the empty value behaves as unset for the person and
    // still replaces the repository's.
    if env("GIT_ASKPASS").is_none() {
        let askpass = config(&["--get-all", "core.askPass"])
            .pop()
            .or_else(|| env("SSH_ASKPASS").map(|p| p.to_string_lossy().into_owned()))
            .unwrap_or_default();
        v.push(format!("core.askPass={askpass}"));
    }
    // An empty helper clears every helper read so far, the repository's
    // included; then the person's own come back in their order.
    v.push("credential.helper=".into());
    for line in config(&["--get-regexp", r"^credential\..*helper$"]) {
        let (key, value) = line.split_once(' ').unwrap_or((line.as_str(), ""));
        if key.ends_with(".helper") && !key.contains('=') {
            v.push(format!("{key}={value}"));
        }
    }
    v
}

/// `git config <args>` over the person's system and global files, in the
/// order git reads them. Never a repository's.
fn person_config(args: &[&str]) -> Vec<String> {
    let mut v = vec![];
    for scope in ["--system", "--global"] {
        let Ok(out) = Command::new("git")
            .args(["config", scope, "--includes"])
            .args(args)
            .current_dir(std::env::temp_dir())
            .env_remove("GIT_DIR")
            .env_remove("GIT_CONFIG")
            .stdin(Stdio::null())
            .stderr(Stdio::null())
            .output()
        else {
            continue;
        };
        if out.status.success() {
            v.extend(String::from_utf8_lossy(&out.stdout).lines().map(str::to_string));
        }
    }
    v
}

/// A `git` command in `dir`. Under [`Trust::Agent`] it carries
/// [`guard_args`] and no `core.gitProxy`: git takes the first value it reads
/// for that key, so a `-c` cannot replace the repository's, but an empty
/// `GIT_PROXY_COMMAND` makes git skip the setting.
fn git_command(dir: &Path, trust: Trust) -> Command {
    let mut c = Command::new("git");
    c.arg("-C").arg(dir);
    // Only the directories named here count, never the daemon's own.
    for k in ["GIT_DIR", "GIT_COMMON_DIR", "GIT_WORK_TREE", "GIT_INDEX_FILE", "GIT_OBJECT_DIRECTORY"] {
        c.env_remove(k);
    }
    if trust == Trust::Agent {
        c.args(guard_args());
        if std::env::var_os("GIT_PROXY_COMMAND").is_none() {
            c.env("GIT_PROXY_COMMAND", "");
        }
    }
    // Never prompt, never use a pager, and keep output stable.
    c.env("GIT_TERMINAL_PROMPT", "0").env("GIT_PAGER", "cat").env("LC_ALL", "C");
    c
}

/// Reads a small git pointer file (`.git`, `commondir`): a regular file of
/// at most 4 KiB, never through a link.
fn read_pointer(p: &Path) -> std::io::Result<String> {
    let m = std::fs::symlink_metadata(p)?;
    if !m.is_file() || m.len() > 4096 {
        return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "not a small regular file"));
    }
    std::fs::read_to_string(p)
}

/// The git directory and the shared git directory of the working copy at
/// `root`, checked so git takes its config, hooks and info only from where
/// they belong (SEC-13). Refused: a `commondir` file in a main git
/// directory (git would read the settings of the folder it names instead),
/// and a linked worktree whose `commondir` names anything but the git
/// directory two levels above its own folder in `worktrees/`.
fn git_dirs(root: &Path) -> Result<(PathBuf, PathBuf)> {
    let bad = |why: String| GitError::Invalid(format!("refusing to run git in {}: {why}", root.display()));
    let dot = root.join(".git");
    let m = std::fs::metadata(&dot).map_err(|e| bad(format!("cannot read .git: {e}")))?;
    let gitdir = if m.is_dir() {
        canonical(&dot).map_err(|e| bad(format!("cannot read .git: {e}")))?
    } else {
        let text = read_pointer(&dot).map_err(|e| bad(format!("cannot read .git: {e}")))?;
        let dir = text.trim_end().strip_prefix("gitdir: ").ok_or_else(|| bad(".git has no gitdir line".into()))?;
        canonical(&root.join(dir)).map_err(|e| bad(format!("its gitdir {dir} is missing: {e}")))?
    };
    let commondir = gitdir.join("commondir");
    if std::fs::symlink_metadata(&commondir).is_err() {
        return Ok((gitdir.clone(), gitdir));
    }
    if m.is_dir() {
        return Err(bad(format!("{} exists; a main git directory must not name another", commondir.display())));
    }
    let text = read_pointer(&commondir).map_err(|e| bad(format!("cannot read {}: {e}", commondir.display())))?;
    let common = canonical(&gitdir.join(text.trim_end()))
        .map_err(|e| bad(format!("{} names a missing folder: {e}", commondir.display())))?;
    let expected = gitdir.parent().filter(|w| w.file_name() == Some("worktrees".as_ref())).and_then(Path::parent);
    if expected != Some(common.as_path()) {
        return Err(bad(format!(
            "{} names {}, not the repository it belongs to",
            commondir.display(),
            common.display()
        )));
    }
    if std::fs::symlink_metadata(common.join("commondir")).is_ok() {
        return Err(bad(format!(
            "{} has a commondir file; a main git directory must not name another",
            common.display()
        )));
    }
    Ok((gitdir, common))
}

/// Refuses a worktree whose `.git` file points anywhere but into the main
/// repository's `.git/worktrees`: an agent could aim it at a repository it
/// made, with config and hooks of its own (SEC-13). Returns the verified
/// git directory and shared git directory, which commands then name
/// explicitly.
fn check_gitdir(root: &Path, worktrees: &Path) -> Result<(PathBuf, PathBuf)> {
    let bad = |why: String| GitError::Invalid(format!("refusing to run git in {}: {why}", root.display()));
    let m = std::fs::symlink_metadata(root.join(".git")).map_err(|e| bad(format!("cannot read .git: {e}")))?;
    if !m.is_file() {
        return Err(bad(".git is not a worktree's link file".into()));
    }
    let (dir, common) = git_dirs(root)?;
    if dir.parent() != Some(worktrees) || worktrees.parent() != Some(common.as_path()) {
        return Err(bad(format!("its gitdir {} is not in {}", dir.display(), worktrees.display())));
    }
    Ok((dir, common))
}

/// Git subcommands that can write the git directory (refs, reflogs,
/// objects, the index). Before any of them the daemon runs [`check_links`].
const WRITES: &[&str] = &[
    "add",
    "branch",
    "checkout",
    "cherry-pick",
    "commit",
    "commit-tree",
    "fetch",
    "gc",
    "hash-object",
    "merge",
    "pack-refs",
    "prune",
    "pull",
    "push",
    "read-tree",
    "rebase",
    "reflog",
    "repack",
    "reset",
    "revert",
    "stash",
    "switch",
    "tag",
    "update-index",
    "update-ref",
    "worktree",
    "write-tree",
];

/// Refuses a git directory where git, run by the daemon as the person, would
/// write through a link an agent planted (SEC-13): git appends to reflogs
/// with a plain open, so `logs/HEAD` linked to `~/.profile` adds a line the
/// agent chose there. Walks `logs/`, `refs/`, `worktrees/` and
/// `objects/info/` of the shared git directory, and the worktree's own git
/// directory, and refuses any symbolic link and (on Unix, outside `objects/`,
/// whose files `git clone --local` hard-links) any file with more than one
/// hard link, naming the path. `info/` too, which the daemon writes itself.
fn check_links(gitdir: &Path, common: &Path) -> Result<()> {
    fn walk(p: &Path, depth: usize, nlink: bool) -> std::result::Result<(), PathBuf> {
        let Ok(m) = std::fs::symlink_metadata(p) else { return Ok(()) };
        if m.file_type().is_symlink() {
            return Err(p.to_path_buf());
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            if nlink && m.is_file() && m.nlink() > 1 {
                return Err(p.to_path_buf());
            }
        }
        #[cfg(not(unix))]
        let _ = nlink;
        // Deeper than any ref name git accepts here: refuse rather than recurse.
        if m.is_dir() && depth > 64 {
            return Err(p.to_path_buf());
        }
        if m.is_dir() {
            for e in std::fs::read_dir(p).map_err(|_| p.to_path_buf())? {
                walk(&e.map_err(|_| p.to_path_buf())?.path(), depth + 1, nlink)?;
            }
        }
        Ok(())
    }
    let mut roots: Vec<PathBuf> =
        ["logs", "refs", "worktrees", "objects/info", "info"].iter().map(|d| common.join(d)).collect();
    if gitdir != common {
        roots.push(gitdir.to_path_buf());
    }
    for r in &roots {
        walk(r, 0, !r.starts_with(common.join("objects"))).map_err(|p| {
            GitError::Invalid(format!(
                "refusing to run git in {}: {} is a link (symbolic or hard), and git would write through it; \
                 remove it and check what it points at",
                common.display(),
                p.display()
            ))
        })?;
    }
    Ok(())
}

/// Refuses `info/alternates` (and `http-alternates`) in the object store
/// `objects` of the repository at `common`: they make git read objects from
/// another repository, and the daemon never creates them (SEC-13). Checked
/// before every command, in the shared store and a task's own.
fn check_alternates(objects: &Path, common: &Path) -> Result<()> {
    for f in ["alternates", "http-alternates"] {
        let p = objects.join("info").join(f);
        if std::fs::symlink_metadata(&p).is_ok() {
            return Err(GitError::Invalid(format!(
                "refusing to run git in {}: {} makes git read objects from another repository, and Polyph did \
                 not create it; remove it (`git repack -a -d` first if the repository needs those objects)",
                common.display(),
                p.display()
            )));
        }
    }
    Ok(())
}

/// Subcommands that reach the network with the person's credentials: they
/// run outside the daemon's git sandbox ([`confine`]). Their writes go to
/// `refs/remotes/`, `FETCH_HEAD` and `objects/`, none of which an agent
/// writes.
const NETWORK: &[&str] = &["fetch", "pull", "push"];

/// `GIT_ALTERNATE_OBJECT_DIRECTORIES` naming `dir` alone, quoted when it
/// holds the list separator or starts with a quote.
fn alternates_value(dir: &Path) -> std::ffi::OsString {
    let s = dir.to_string_lossy();
    let sep = if cfg!(windows) { ';' } else { ':' };
    if s.contains(sep) || s.starts_with('"') {
        format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\"")).into()
    } else {
        dir.as_os_str().to_os_string()
    }
}

/// The git subcommand in `args`, past any `-c key=value`.
fn subcommand<'a>(args: &[&'a str]) -> &'a str {
    let mut it = args.iter();
    while let Some(a) = it.next() {
        if *a == "-c" {
            it.next();
        } else if !a.starts_with('-') {
            return a;
        }
    }
    ""
}

fn run(dir: &Path, args: &[&str], env: &[(&str, &str)], stdin: Option<&[u8]>) -> Result<(bool, String, String)> {
    let mut c = git_command(dir, Trust::Agent);
    c.args(args);
    output(c, env, stdin)
}

fn output(mut c: Command, env: &[(&str, &str)], stdin: Option<&[u8]>) -> Result<(bool, String, String)> {
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
        Ok(Self { root: PathBuf::from(out.trim()), worktrees: None, network: Trust::Agent })
    }

    /// Opens the working copy containing `path` for a `fetch` or `push` that
    /// uses the repository's own configuration and hooks as they are. Only
    /// for repositories no agent could write the config, hooks or info of
    /// (see [`sandbox_git`]).
    pub fn open_trusted(path: &Path) -> Result<Self> {
        Ok(Self { network: Trust::Person, ..Self::open(path)? })
    }

    /// Opens an agent's worktree at `path`, a linked worktree of this
    /// repository. Every command first checks that its `.git` still points
    /// into this repository's `.git/worktrees`, then names that git
    /// directory itself, so git never reads the `.git` file the agent can
    /// rewrite.
    pub fn open_worktree(&self, path: &Path) -> Result<Repo> {
        let worktrees = self.common_dir()?.join("worktrees");
        let root = canonical(path).map_err(GitError::Spawn)?;
        check_gitdir(&root, &worktrees)?;
        Ok(Repo { root, worktrees: Some(worktrees), network: Trust::Agent })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// The verified git directory and shared git directory (see
    /// [`git_dirs`] and [`check_gitdir`]).
    fn dirs(&self) -> Result<(PathBuf, PathBuf)> {
        match &self.worktrees {
            Some(w) => check_gitdir(&self.root, w),
            None => git_dirs(&self.root),
        }
    }

    /// The command for `args`, with the verified git directory named on the
    /// command line and the shared one in `GIT_COMMON_DIR`, so git never
    /// takes either from a file an agent could write.
    fn command(&self, trust: Trust, args: &[&str]) -> Result<Command> {
        self.command_in(trust, args, &[])
    }

    /// [`Repo::command`]; a command that writes runs in the daemon's git
    /// sandbox ([`confine`]) with [`Repo::write_scope`] and `extra` writable.
    /// In a task worktree with an object store of its own, git reads and
    /// writes objects there, with the shared store as an alternate it only
    /// reads (SEC-13).
    fn command_in(&self, trust: Trust, args: &[&str], extra: &[PathBuf]) -> Result<Command> {
        let mut c = git_command(&self.root, trust);
        let (dir, common) = self.dirs()?;
        check_alternates(&common.join("objects"), &common)?;
        let own = self.own_objects(&dir, &common);
        if let Some(o) = &own {
            check_alternates(o, &common)?;
        }
        let sub = subcommand(args);
        let writes = WRITES.contains(&sub);
        if writes {
            check_links(&dir, &common)?;
        }
        let mut g = std::ffi::OsString::from("--git-dir=");
        g.push(&dir);
        let mut t = std::ffi::OsString::from("--work-tree=");
        t.push(&self.root);
        c.arg(g).arg(t).env("GIT_COMMON_DIR", &common);
        if let Some(o) = &own {
            c.env("GIT_OBJECT_DIRECTORY", o)
                .env("GIT_ALTERNATE_OBJECT_DIRECTORIES", alternates_value(&common.join("objects")));
        }
        c.args(args);
        if writes && !NETWORK.contains(&sub) {
            let (mut w, ro) = self.write_scope(&dir, &common);
            w.extend(extra.iter().cloned());
            c = confine::confine(c, &w, &ro);
        }
        Ok(c)
    }

    /// A task worktree's own object store, `<its git directory>/objects`,
    /// when the daemon made one ([`Repo::add_worktree`]) and it is a folder,
    /// not a link.
    fn own_objects(&self, dir: &Path, common: &Path) -> Option<PathBuf> {
        let o = dir.join("objects");
        (self.worktrees.is_some() && dir != common && std::fs::symlink_metadata(&o).is_ok_and(|m| m.is_dir()))
            .then_some(o)
    }

    /// What the daemon's git writes may write (SEC-13): in a task worktree,
    /// the worktree and its own git directory (the folders its agent writes
    /// anyway), but not the files there that name the repository; in a
    /// person's checkout, the checkout and the shared git directory, but not
    /// its settings, hooks and `info/`. Returns (writable, read-only inside).
    fn write_scope(&self, dir: &Path, common: &Path) -> (Vec<PathBuf>, Vec<PathBuf>) {
        if self.worktrees.is_some() {
            let ro = ["commondir", "gitdir", "config.worktree", "objects/info"].iter().map(|f| dir.join(f)).collect();
            (vec![self.root.clone(), dir.to_path_buf()], ro)
        } else {
            let ro = ["config", "config.worktree", "hooks", "info", "objects/info", "commondir"]
                .iter()
                .map(|f| common.join(f))
                .collect();
            (vec![self.root.clone(), common.to_path_buf(), dir.to_path_buf()], ro)
        }
    }

    /// The folders of `branch` in `refs/heads/` and `logs/refs/heads/`, made
    /// if missing: what an `update-ref` of it writes besides the worktree's
    /// own git directory.
    fn branch_scope(&self, branch: &str) -> Result<Vec<PathBuf>> {
        let common = self.common_dir()?;
        let parent = Path::new(branch).parent().unwrap_or(Path::new(""));
        let v = vec![
            common.join("refs").join("heads").join(parent),
            common.join("logs").join("refs").join("heads").join(parent),
        ];
        for d in &v {
            std::fs::create_dir_all(d).map_err(GitError::Spawn)?;
        }
        Ok(v)
    }

    /// The folder a worktree at `path` goes in, made if missing: `git
    /// worktree add` and `remove` write there.
    fn worktree_scope(path: &Path) -> Result<Vec<PathBuf>> {
        let parent = path.parent().filter(|p| !p.as_os_str().is_empty()).unwrap_or(Path::new("."));
        std::fs::create_dir_all(parent).map_err(GitError::Spawn)?;
        Ok(vec![canonical(parent).map_err(GitError::Spawn)?])
    }

    /// Before a fetch or push with the repository's own settings: git,
    /// finding the repository by itself, must arrive at the shared git
    /// directory verified above, `<main working copy>/.git`.
    fn check_trusted(&self, cmd: &str) -> Result<()> {
        let (_, common) = self.dirs()?;
        let (ok, out, err) = run(&self.root, &["rev-parse", "--git-common-dir"], &[], None)?;
        let found = PathBuf::from(out.trim());
        let found = canonical(&if found.is_absolute() { found } else { self.root.join(found) }).ok();
        let main = common.parent().map(|p| p.join(".git"));
        if !ok || found.as_ref() != Some(&common) || main.as_ref() != Some(&common) {
            let found = found.map(|p| p.display().to_string()).unwrap_or_else(|| err.trim().to_string());
            return Err(GitError::Invalid(format!(
                "refusing to {cmd} from {} with its own settings: git finds them in {found}, not in {}",
                self.root.display(),
                main.unwrap_or(common).display()
            )));
        }
        Ok(())
    }

    fn exec(&self, args: &[&str], env: &[(&str, &str)], stdin: Option<&[u8]>) -> Result<(bool, String, String)> {
        output(self.command(Trust::Agent, args)?, env, stdin)
    }

    fn git(&self, args: &[&str]) -> Result<String> {
        self.git_env(args, &[], None)
    }

    /// `fetch` and `push`: with the repository's own config when it is
    /// trusted ([`Repo::open_trusted`]).
    fn git_network(&self, args: &[&str]) -> Result<String> {
        let cmd = subcommand(args);
        if self.network == Trust::Person {
            self.check_trusted(cmd)?;
        }
        let (ok, out, err) = output(self.command(self.network, args)?, &[], None)?;
        if ok { Ok(out) } else { Err(GitError::Failed { cmd: cmd.to_string(), stderr: err.trim().into() }) }
    }

    fn git_env(&self, args: &[&str], env: &[(&str, &str)], stdin: Option<&[u8]>) -> Result<String> {
        let (ok, out, err) = self.exec(args, env, stdin)?;
        if ok {
            Ok(out)
        } else {
            Err(GitError::Failed { cmd: subcommand(args).to_string(), stderr: err.trim().into() })
        }
    }

    /// [`Repo::git`] with `extra` writable in the daemon's git sandbox.
    fn git_in(&self, args: &[&str], extra: &[PathBuf]) -> Result<String> {
        let (ok, out, err) = output(self.command_in(Trust::Agent, args, extra)?, &[], None)?;
        if ok {
            Ok(out)
        } else {
            Err(GitError::Failed { cmd: subcommand(args).to_string(), stderr: err.trim().into() })
        }
    }

    /// The shared `.git` directory (the same for every worktree), verified
    /// and with links resolved.
    pub fn common_dir(&self) -> Result<PathBuf> {
        Ok(self.dirs()?.1)
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
    /// If the branch exists already, checks it out as it is (the daemon
    /// decides whether it may: it hands an agent only branches it created).
    /// The worktree gets an object store of its own, `<its git
    /// directory>/objects`, where its agent's commits go (SEC-13).
    pub fn add_worktree(&self, path: &Path, branch: &str, base: &str) -> Result<Repo> {
        check_branch_name(branch)?;
        let p = path.to_string_lossy();
        let scope = Self::worktree_scope(path)?;
        if self.branch_exists(branch) {
            self.git_in(&["worktree", "add", &p, branch], &scope)?;
        } else {
            self.git_in(&["worktree", "add", "-b", branch, &p, base], &scope)?;
        }
        let wt = self.open_worktree(path)?;
        let (dir, _) = wt.dirs()?;
        std::fs::create_dir_all(dir.join("objects").join("info")).map_err(GitError::Spawn)?;
        Ok(wt)
    }

    /// Adds a detached worktree at `rev` (for running checks on a merge).
    pub fn add_detached(&self, path: &Path, rev: &str) -> Result<Repo> {
        self.git_in(&["worktree", "add", "--detach", &path.to_string_lossy(), rev], &Self::worktree_scope(path)?)?;
        self.open_worktree(path)
    }

    pub fn remove_worktree(&self, path: &Path) -> Result<()> {
        self.git_in(&["worktree", "remove", "--force", &path.to_string_lossy()], &Self::worktree_scope(path)?)
            .map(|_| ())
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

    /// Paths with uncommitted changes (including untracked files). Never
    /// looks inside submodules or other repositories in the working copy,
    /// whose own config could name programs; in an agent's worktree a
    /// repository that is not a submodule of HEAD is refused.
    pub fn dirty_files(&self) -> Result<Vec<String>> {
        if self.worktrees.is_some() {
            self.check_nested("read the changes in")?;
        }
        let out = self.git(&[
            "-c",
            "diff.ignoreSubmodules=all",
            "status",
            "--porcelain=v1",
            "-z",
            "--untracked-files=all",
            "--ignore-submodules=all",
        ])?;
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

    /// Refuses a working copy that holds a git repository (a `.git` file or
    /// folder below its root) not listed as a submodule in HEAD's
    /// `.gitmodules`, or a submodule entry (mode 160000) in the index that
    /// HEAD does not have. Git runs commands inside such a repository with
    /// its config, and an agent writes that config (the clean filter of a
    /// nested repository runs during `status`, SEC-13).
    fn check_nested(&self, what: &str) -> Result<()> {
        use std::collections::BTreeSet;
        let refuse = |why: String| GitError::Invalid(format!("refusing to {what} {}: {why}", self.root.display()));
        let z =
            |out: String| -> Vec<String> { out.split('\0').filter(|s| !s.is_empty()).map(str::to_string).collect() };
        let has_head = self.rev_parse("HEAD").is_ok();
        let mut submodules = BTreeSet::new();
        let mut head_links = BTreeSet::new();
        if has_head {
            if let Ok(out) =
                self.git(&["config", "-z", "--blob", "HEAD:.gitmodules", "--get-regexp", r"^submodule\..*\.path$"])
            {
                for kv in z(out) {
                    if let Some((_, path)) = kv.split_once('\n') {
                        submodules.insert(path.trim_end_matches('/').to_string());
                    }
                }
            }
            for e in z(self.git(&["ls-tree", "-r", "-z", "--full-tree", "HEAD"])?) {
                if let Some((meta, path)) = e.split_once('\t')
                    && meta.starts_with("160000 ")
                {
                    head_links.insert(path.to_string());
                }
            }
        }
        let mut dirs = BTreeSet::new();
        for e in z(self.git(&["ls-files", "-s", "-z"])?) {
            let Some((meta, path)) = e.split_once('\t') else { continue };
            if meta.starts_with("160000 ") {
                if !head_links.contains(path) {
                    return Err(refuse(format!("{path} is a new submodule entry; add submodules yourself")));
                }
                dirs.insert(path.to_string());
            }
            let mut p = Path::new(path).parent();
            while let Some(d) = p.filter(|d| !d.as_os_str().is_empty()) {
                if !dirs.insert(d.to_string_lossy().into_owned()) {
                    break;
                }
                p = d.parent();
            }
        }
        // Untracked repositories show as their folder, with a slash.
        for e in z(self.git(&["ls-files", "-z", "--others", "--exclude-standard"])?) {
            if let Some(d) = e.strip_suffix('/') {
                dirs.insert(d.to_string());
            }
        }
        for d in dirs {
            if std::fs::symlink_metadata(self.root.join(&d).join(".git")).is_ok() && !submodules.contains(&d) {
                return Err(refuse(format!(
                    "{d} is a git repository inside the worktree and not a submodule in .gitmodules; remove its .git"
                )));
            }
        }
        Ok(())
    }

    /// Commits every change in the working copy. Returns the new commit, or
    /// None when there was nothing to commit. Submodules are never looked
    /// into (see [`Repo::dirty_files`]).
    /// In a task worktree with an object store of its own, this is
    /// [`Repo::commit_task`] on the branch HEAD names.
    pub fn commit_all(&self, message: &str, who: &Signature) -> Result<Option<String>> {
        if let Ok((dir, common)) = self.dirs()
            && self.own_objects(&dir, &common).is_some()
        {
            let branch = self.current_branch().ok_or_else(|| {
                GitError::Invalid(format!("refusing to commit in {}: HEAD names no branch", self.root.display()))
            })?;
            return self.commit_task(&branch, message, who);
        }
        if self.dirty_files()?.is_empty() {
            return Ok(None);
        }
        let no_sub = ["-c", "diff.ignoreSubmodules=all", "-c", "submodule.recurse=false"];
        if self.worktrees.is_some() {
            self.check_nested("commit in")?;
        }
        // `add` looks into every submodule it updates, whatever the
        // config says, so submodule entries are left out by path.
        let links: Vec<String> = self
            .git(&["ls-files", "-s", "-z"])?
            .split('\0')
            .filter_map(|e| e.strip_prefix("160000 ")?.split_once('\t').map(|(_, p)| format!(":(exclude,literal){p}")))
            .collect();
        let mut add = vec!["add", "-A", "--", "."];
        add.extend(links.iter().map(String::as_str));
        self.git(&[&no_sub[..], &add].concat())?;
        let env = sig_env(who);
        let env: Vec<(&str, &str)> = env.iter().map(|(k, v)| (*k, v.as_str())).collect();
        self.git_env(
            &[&no_sub[..], &["commit", "--no-verify", "--quiet", "-F", "-"]].concat(),
            &env,
            Some(message.as_bytes()),
        )?;
        Ok(Some(self.rev_parse("HEAD")?))
    }

    /// Commits every change in this task worktree onto `branch`, which HEAD
    /// must name. The commit goes onto the branch itself (`commit-tree` on
    /// its tip, then `update-ref` from that tip), never through HEAD: a task
    /// agent writes its worktree's HEAD, and pointing it at `main` must not
    /// make the daemon commit there (SEC-13). Then the branch's commits,
    /// the agent's own included, go into the shared object store
    /// ([`Repo::import_branch`]). Returns the new commit, or None when there
    /// was nothing to commit.
    pub fn commit_task(&self, branch: &str, message: &str, who: &Signature) -> Result<Option<String>> {
        check_branch_name(branch)?;
        let full = format!("refs/heads/{branch}");
        let head = self.exec(&["symbolic-ref", "--quiet", "HEAD"], &[], None)?.1.trim().to_string();
        if head != full {
            let head = if head.is_empty() { "detached".to_string() } else { head };
            return Err(GitError::Invalid(format!(
                "refusing to commit in {}: HEAD is {head}, not {full}; check out {branch} there",
                self.root.display()
            )));
        }
        let made = self.commit_on(branch, &full, message, who)?;
        self.import_branch(branch)?;
        Ok(made)
    }

    fn commit_on(&self, branch: &str, full: &str, message: &str, who: &Signature) -> Result<Option<String>> {
        if self.dirty_files()?.is_empty() {
            return Ok(None);
        }
        let no_sub = ["-c", "diff.ignoreSubmodules=all", "-c", "submodule.recurse=false"];
        if self.worktrees.is_some() {
            self.check_nested("commit in")?;
        }
        let links: Vec<String> = self
            .git(&["ls-files", "-s", "-z"])?
            .split('\0')
            .filter_map(|e| e.strip_prefix("160000 ")?.split_once('\t').map(|(_, p)| format!(":(exclude,literal){p}")))
            .collect();
        let mut add = vec!["add", "-A", "--", "."];
        add.extend(links.iter().map(String::as_str));
        self.git(&[&no_sub[..], &add].concat())?;
        let parent = self.rev_parse(full)?;
        let tree = self.git(&["write-tree"])?.trim().to_string();
        if self.git(&["rev-parse", &format!("{parent}^{{tree}}")])?.trim() == tree {
            return Ok(None);
        }
        let env = sig_env(who);
        let env: Vec<(&str, &str)> = env.iter().map(|(k, v)| (*k, v.as_str())).collect();
        let text = format!("{}\n", message.trim());
        let commit = self
            .git_env(&["commit-tree", &tree, "-p", &parent, "-F", "-"], &env, Some(text.as_bytes()))?
            .trim()
            .to_string();
        let subject = text.lines().next().unwrap_or_default();
        self.git_in(
            &["update-ref", "-m", &format!("commit: {subject}"), full, &commit, &parent],
            &self.branch_scope(branch)?,
        )?;
        Ok(Some(commit))
    }

    /// Copies the commits on `branch` from this task worktree's own object
    /// store into the shared one (SEC-13). Git packs them from the task's
    /// store and `index-pack --strict` takes them into the shared one: it
    /// names every object by hashing its content and checks that every
    /// object they name is there, so a loose object or pack the agent made
    /// under another object's name never enters the shared store, and the
    /// copy fails rather than take a commit whose objects are not what
    /// their names say. Nothing to do when the worktree has no store of its
    /// own or the shared store has the tip already.
    pub fn import_branch(&self, branch: &str) -> Result<()> {
        check_branch_name(branch)?;
        let (dir, common) = self.dirs()?;
        if self.own_objects(&dir, &common).is_none() {
            return Ok(());
        }
        check_links(&dir, &common)?;
        let tip = self.rev_parse(&format!("refs/heads/{branch}"))?;
        let shared = |args: &[&str]| {
            let mut c = git_command(&self.root, Trust::Agent);
            let mut g = std::ffi::OsString::from("--git-dir=");
            g.push(&common);
            c.arg(g).args(args);
            c
        };
        let failed = |stderr: String| GitError::Failed {
            cmd: "index-pack".into(),
            stderr: format!("could not take the commits of {branch} into the shared object store: {}", stderr.trim()),
        };
        // Commits the shared store has, of the branches no agent writes.
        let has = |oids: &str| -> Result<Vec<String>> {
            let (_, out, _) =
                output(shared(&["cat-file", "--batch-check=%(objectname) %(objecttype)"]), &[], Some(oids.as_bytes()))?;
            Ok(out.lines().filter_map(|l| l.strip_suffix(" commit")).map(str::to_string).collect())
        };
        if !has(&format!("{tip}\n"))?.is_empty() {
            return Ok(());
        }
        let (ok, refs, err) = output(
            shared(&["for-each-ref", "--format=%(objectname) %(refname)", "refs/heads", "refs/remotes"]),
            &[],
            None,
        )?;
        if !ok {
            return Err(failed(err));
        }
        let oids: String = refs
            .lines()
            .filter_map(|l| l.split_once(' '))
            .filter(|(_, name)| !name.starts_with("refs/heads/polyph/"))
            .map(|(oid, _)| format!("{oid}\n"))
            .collect();
        let mut revs = format!("{tip}\n--not\n");
        for oid in has(&oids)? {
            revs.push_str(&oid);
            revs.push('\n');
        }
        let mut pack = self.command(Trust::Agent, &["pack-objects", "--revs", "--stdout", "-q"])?;
        pack.stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped());
        let mut p = pack.spawn().map_err(GitError::Spawn)?;
        let input = p.stdin.take();
        let revs2 = revs.clone();
        let writer = std::thread::spawn(move || {
            use std::io::Write;
            if let Some(mut i) = input {
                let _ = i.write_all(revs2.as_bytes());
            }
        });
        let Some(packed) = p.stdout.take() else { return Err(failed("no output from pack-objects".into())) };
        let objects = common.join("objects");
        let mut index = confine::confine(
            shared(&["index-pack", "--stdin", "--strict"]),
            std::slice::from_ref(&objects),
            &[objects.join("info")],
        );
        index.stdin(Stdio::from(packed)).stdout(Stdio::piped()).stderr(Stdio::piped());
        let indexed = index.output().map_err(GitError::Spawn)?;
        let _ = writer.join();
        let packed = p.wait_with_output().map_err(GitError::Spawn)?;
        if !packed.status.success() {
            return Err(failed(String::from_utf8_lossy(&packed.stderr).into_owned()));
        }
        if !indexed.status.success() {
            return Err(failed(String::from_utf8_lossy(&indexed.stderr).into_owned()));
        }
        // Everything the tip reaches is in the shared store now.
        let (ok, _, err) =
            output(shared(&["rev-list", "--objects", "--quiet", "--stdin"]), &[], Some(revs.as_bytes()))?;
        if !ok || has(&format!("{tip}\n"))?.is_empty() {
            return Err(failed(err));
        }
        Ok(())
    }

    /// Refuses `branch` when its tip is not where its reflog says git last
    /// set it: then something wrote the ref without git, as an agent that
    /// could write `refs/` would (SEC-13). The daemon checks the branch it
    /// merges into before merging and before moving it. A branch without a
    /// reflog (or an empty one) passes.
    pub fn check_branch_logged(&self, branch: &str) -> Result<()> {
        use std::io::{Read, Seek, SeekFrom};
        check_branch_name(branch)?;
        let full = format!("refs/heads/{branch}");
        let tip = self.git(&["rev-parse", "--verify", "--quiet", &full])?.trim().to_string();
        let log = self.common_dir()?.join("logs").join("refs").join("heads").join(branch);
        let bad = |why: String| GitError::Invalid(format!("refusing to merge into {branch}: {why}"));
        let Ok(m) = std::fs::symlink_metadata(&log) else { return Ok(()) };
        if !m.is_file() {
            return Err(bad(format!("{} is not a plain file", log.display())));
        }
        let mut f = std::fs::File::open(&log).map_err(GitError::Spawn)?;
        f.seek(SeekFrom::Start(m.len().saturating_sub(8192))).map_err(GitError::Spawn)?;
        let mut tail = vec![];
        f.read_to_end(&mut tail).map_err(GitError::Spawn)?;
        let tail = String::from_utf8_lossy(&tail);
        let Some(last) = tail.lines().rev().find(|l| !l.trim().is_empty()) else { return Ok(()) };
        let logged = last.split(' ').nth(1).unwrap_or_default();
        if logged != tip {
            let short = |s: &str| s.chars().take(12).collect::<String>();
            return Err(bad(format!(
                "it points at {}, but its reflog ({}) says git last set it to {}. Something changed it without \
                 git, perhaps an agent outside its sandbox; check {branch} before queueing the merge again",
                short(&tip),
                log.display(),
                short(logged)
            )));
        }
        Ok(())
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
        let out =
            self.git(&["diff", "--no-ext-diff", "--no-textconv", "--name-only", "-z", &format!("{base}...{head}")])?;
        Ok(out.split('\0').filter(|s| !s.is_empty()).map(str::to_string).collect())
    }

    /// Unified diff of `head` since it forked from `base`.
    pub fn diff(&self, base: &str, head: &str) -> Result<String> {
        self.git(&["diff", "--no-color", "--no-ext-diff", "--no-textconv", &format!("{base}...{head}")])
    }

    /// `git diff --shortstat` of `head` since it forked from `base`
    /// ("2 files changed, 5 insertions(+), 1 deletion(-)"), empty when
    /// nothing changed.
    pub fn diff_stat(&self, base: &str, head: &str) -> Result<String> {
        Ok(self
            .git(&["diff", "--no-ext-diff", "--no-textconv", "--shortstat", &format!("{base}...{head}")])?
            .trim()
            .to_string())
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
    /// Refuses a branch that changed without git ([`Repo::check_branch_logged`]).
    pub fn advance_branch(&self, branch: &str, commit: &str, old: &str) -> Result<()> {
        check_branch_name(branch)?;
        self.check_branch_logged(branch)?;
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
                let scope = if main { vec![] } else { wt.branch_scope(branch)? };
                wt.git_in(&["merge", "--ff-only", "--quiet", commit], &scope)?;
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
        self.git_network(&["push", "--quiet", remote, &format!("refs/heads/{branch}:refs/heads/{branch}")]).map(|_| ())
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

    /// Fetches `remote` (all branches, pruning gone ones) without ever
    /// prompting. Submodules are not fetched: their git directories stay
    /// writable for agents. Unless the repository is trusted, the remote's
    /// `uploadpack` is the plain `git-upload-pack`, whatever its config says
    /// (git keeps the first `uploadpack` it reads, so this is the command
    /// line option, not a `-c`).
    pub fn fetch(&self, remote: &str) -> Result<()> {
        if remote.starts_with('-') {
            return Err(GitError::Invalid(format!("not a valid remote: {remote}")));
        }
        let mut args = vec!["fetch", "--quiet", "--prune", "--no-recurse-submodules"];
        if self.network == Trust::Agent {
            args.push("--upload-pack=git-upload-pack");
        }
        args.push(remote);
        self.git_network(&args).map(|_| ())
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
        let (dir, common) = self.dirs()?;
        check_links(&dir, &common)?;
        let p = common.join("info").join("exclude");
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
    // A new repository has no config or hooks of its own yet, so the
    // person's settings apply as they are.
    let mut c = Command::new("git");
    c.arg("-C").arg(parent).args(&refs).env("GIT_PAGER", "cat");
    let status = c.status().map_err(GitError::Spawn)?;
    if !status.success() {
        return Err(GitError::Failed { cmd: "clone".into(), stderr: format!("git clone exited with {status}") });
    }
    Repo::open(dir)
}

/// How an agent's sandbox mounts the git checkout it works in (SEC-13).
/// Bubblewrap and Seatbelt enforce it; Landlock, Windows and container
/// mode do not.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SandboxGit {
    /// Folders of the shared git directory the agent writes so it can
    /// commit on its task branch `polyph/<task>/<name>`: its task's folder
    /// in `refs/heads/polyph/` and the matching folder in
    /// `logs/refs/heads/polyph/`, its own folder in `worktrees/` (its HEAD,
    /// index, reflog and per-worktree refs) and, listed on its own so it is
    /// a mount the agent cannot rename, its own object store in that
    /// folder. Empty unless it commits.
    pub writable: Vec<PathBuf>,
    /// Paths that stay read-only: the whole shared git directory (its
    /// `objects/` included), a linked worktree's `.git` file and, when it
    /// commits, the `commondir`, `gitdir` and `config.worktree` files and
    /// `objects/info/` in its own folder, and every other entry in its task
    /// folders.
    pub read_only: Vec<PathBuf>,
    /// Environment for the agent when it commits: its own object store as
    /// `GIT_OBJECT_DIRECTORY` and the shared one, read-only, as
    /// `GIT_ALTERNATE_OBJECT_DIRECTORIES`. Set by the daemon, never read
    /// from a file the agent writes.
    pub env: Vec<(String, String)>,
}

/// The task id in a task branch `polyph/<task>/<name>` (no further `/`).
pub fn task_folder(branch: &str) -> Option<&str> {
    let (task, name) = branch.strip_prefix("polyph/")?.split_once('/')?;
    (!task.is_empty() && !name.is_empty() && !name.contains('/') && check_branch_name(branch).is_ok()).then_some(task)
}

/// The [`SandboxGit`] for an agent working in `worktree`. With `branch` (a
/// task's branch, `polyph/<task>/<name>`) and a linked worktree, the agent
/// can run `git add` and `git commit` on that branch, with its objects in
/// its worktree's own store; otherwise (the person's own checkout, checks,
/// a branch of another form) the whole git directory is read-only and only
/// read-only git commands work; the daemon still commits its work when it
/// finishes. Empty outside a git checkout. Creates the task's folders in
/// `refs/` and `logs/`, the worktree's object store with its `info/`, and an
/// empty `config.worktree` when they are missing, so the agent cannot
/// create them.
pub fn sandbox_git(worktree: &Path, branch: Option<&str>) -> SandboxGit {
    let Ok(r) = Repo::open(worktree) else { return SandboxGit::default() };
    let Ok((gitdir, common)) = r.dirs() else { return SandboxGit::default() };
    let mut g = SandboxGit { writable: vec![], read_only: vec![common.clone()], env: vec![] };
    // The `.git` link file of a linked worktree.
    if let Ok(root) = canonical(r.root())
        && std::fs::symlink_metadata(root.join(".git")).is_ok_and(|m| m.is_file())
    {
        g.read_only.push(root.join(".git"));
    }
    let leaf = branch.and_then(|b| Some((task_folder(b)?, b.rsplit_once('/')?.1)));
    if let Some((task, name)) = leaf
        && gitdir != common
    {
        let refs = common.join("refs").join("heads").join("polyph").join(task);
        let logs = common.join("logs").join("refs").join("heads").join("polyph").join(task);
        let objects = gitdir.join("objects");
        let info = objects.join("info");
        for d in [&refs, &logs, &info] {
            let _ = std::fs::create_dir_all(d);
        }
        let per_worktree = gitdir.join("config.worktree");
        if std::fs::symlink_metadata(&per_worktree).is_err() {
            let _ = std::fs::write(&per_worktree, "");
        }
        // The object store a second time, after its folder: mounted on its
        // own, so the agent cannot rename it away and put a store of its
        // own making (with an `info/alternates`) in its place.
        g.writable = vec![refs.clone(), logs.clone(), gitdir.clone(), objects.clone()];
        g.read_only.extend(["commondir", "gitdir", "config.worktree"].iter().map(|f| gitdir.join(f)));
        g.read_only.push(info);
        // Everything else in the task folders stays as it is.
        for d in [&refs, &logs] {
            let mut others: Vec<PathBuf> = std::fs::read_dir(d)
                .into_iter()
                .flatten()
                .flatten()
                .filter(|e| e.file_name() != name)
                .map(|e| e.path())
                .collect();
            others.sort();
            g.read_only.extend(others);
        }
        if std::fs::symlink_metadata(&objects).is_ok_and(|m| m.is_dir()) {
            g.env = vec![
                ("GIT_OBJECT_DIRECTORY".into(), objects.to_string_lossy().into_owned()),
                (
                    "GIT_ALTERNATE_OBJECT_DIRECTORIES".into(),
                    alternates_value(&common.join("objects")).to_string_lossy().into_owned(),
                ),
            ];
        }
    }
    let real = |p: &PathBuf, dir: bool| {
        std::fs::symlink_metadata(p).is_ok_and(|m| if dir { m.is_dir() } else { m.is_dir() || m.is_file() })
    };
    g.writable.retain(|p| real(p, true));
    g.read_only.retain(|p| real(p, false));
    g
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

/// A branch name for a task: `polyph/<short id>/<slug>`, one folder per
/// task, so a task agent writes only its own task's branches (SEC-13).
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
    let id: String = short_id.chars().filter(|c| c.is_ascii_alphanumeric()).collect();
    let id = if id.is_empty() { "task".to_string() } else { id };
    if slug.is_empty() { format!("polyph/{id}/task") } else { format!("polyph/{id}/{slug}") }
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
        let r = Repo { root: d.path().to_path_buf(), worktrees: None, network: Trust::Agent };
        run(d.path(), &["init", "-q", "-b", "main"], &[], None).unwrap();
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
        assert_eq!(task_branch("Add OAuth login!", "ab12"), "polyph/ab12/add-oauth-login");
        assert_eq!(task_branch("日本語", "ab12"), "polyph/ab12/task");
        assert_eq!(task_folder("polyph/ab12/add-oauth-login"), Some("ab12"));
        for b in ["polyph/ab12", "polyph/ab12/a/b", "polyph//a", "main", "other/ab12/a", "polyph/ab12/x.lock"] {
            assert_eq!(task_folder(b), None, "{b}");
        }
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
        assert_eq!(r.diff_stat("main", "polyph/t1").unwrap(), "1 file changed, 1 insertion(+)");
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

    #[test]
    fn worktree_commands_name_the_verified_gitdir() {
        let (_d, r) = init();
        let wt_dir = tempfile::tempdir().unwrap();
        let wt = r.add_worktree(&wt_dir.path().join("t1"), "polyph/t1", "main").unwrap();
        let gitdir = canonical(&r.common_dir().unwrap()).unwrap().join("worktrees").join("t1");
        let c = wt.command(Trust::Agent, &["status"]).unwrap();
        let args: Vec<String> = c.get_args().map(|a| a.to_string_lossy().into_owned()).collect();
        assert!(args.contains(&format!("--git-dir={}", gitdir.display())), "{args:?}");
        assert!(args.contains(&format!("--work-tree={}", wt.root().display())), "{args:?}");
        let common = canonical(&r.root().join(".git")).unwrap();
        assert!(c.get_envs().any(|(k, v)| k == "GIT_COMMON_DIR" && v == Some(common.as_os_str())), "{c:?}");
        // The main checkout names its own `.git` too.
        let c = r.command(Trust::Agent, &["status"]).unwrap();
        let args: Vec<String> = c.get_args().map(|a| a.to_string_lossy().into_owned()).collect();
        assert!(args.contains(&format!("--git-dir={}", common.display())), "{args:?}");
        assert!(c.get_envs().any(|(k, v)| k == "GIT_COMMON_DIR" && v == Some(common.as_os_str())), "{c:?}");
    }

    /// SEC-13: a `commondir` file makes git read the config and hooks of
    /// another folder. The daemon refuses one in a main git directory and
    /// any value but the expected one in a worktree's folder.
    #[cfg(unix)]
    #[test]
    fn commondir_redirects_are_refused() {
        let (d, r) = init();
        let t = tempfile::tempdir().unwrap();
        let (script, ran) = tattler(t.path(), "hook");
        // A git directory of the agent's, with a hook and a pager.
        let evil = t.path().join("evil");
        run(t.path(), &["init", "-q", "--bare", &evil.to_string_lossy()], &[], None).unwrap();
        std::fs::create_dir_all(evil.join("hooks")).unwrap();
        std::fs::copy(&script, evil.join("hooks").join("pre-commit")).unwrap();
        std::fs::write(evil.join("config"), format!("[core]\n\tfsmonitor = {}\n", script.display())).unwrap();
        let common = canonical(&r.common_dir().unwrap()).unwrap();

        // In a main git directory.
        std::fs::write(common.join("commondir"), format!("{}\n", evil.display())).unwrap();
        let e = r.dirty_files().unwrap_err().to_string();
        assert!(e.contains("refusing to run git") && e.contains("commondir"), "{e}");
        let trusted = Repo { network: Trust::Person, ..r.clone() };
        assert!(trusted.fetch("origin").unwrap_err().to_string().contains("refusing"));
        assert!(r.commit_all("x", &Signature::agent("A", "1")).is_err());
        std::fs::remove_file(common.join("commondir")).unwrap();
        r.dirty_files().unwrap();

        // In a task worktree's own folder.
        let wt_dir = tempfile::tempdir().unwrap();
        let wt = r.add_worktree(&wt_dir.path().join("t1"), "polyph/t1", "main").unwrap();
        let own = common.join("worktrees").join("t1").join("commondir");
        assert_eq!(std::fs::read_to_string(&own).unwrap().trim(), "../..");
        std::fs::write(&own, format!("{}\n", evil.display())).unwrap();
        assert!(wt.dirty_files().unwrap_err().to_string().contains("not the repository"));
        assert!(r.open_worktree(wt.root()).is_err());
        std::fs::write(wt.root().join("n.rs"), "x\n").unwrap();
        assert!(wt.commit_all("n", &Signature::agent("A", "1")).is_err());
        std::fs::write(&own, "../..\n").unwrap();
        wt.commit_all("n", &Signature::agent("A", "1")).unwrap().unwrap();
        assert!(!ran.exists(), "ran: {}", std::fs::read_to_string(&ran).unwrap_or_default());
        assert_eq!(r.main_root().unwrap(), canonical(d.path()).unwrap());
    }

    /// The trusted fetch checks where git, finding the repository by itself,
    /// takes its settings from.
    #[test]
    fn trusted_fetch_checks_where_git_finds_its_settings() {
        let (d, r) = init();
        let bare = tempfile::tempdir().unwrap();
        run(bare.path(), &["init", "-q", "--bare"], &[], None).unwrap();
        r.git(&["remote", "add", "origin", &bare.path().to_string_lossy()]).unwrap();
        let trusted = Repo::open_trusted(d.path()).unwrap();
        trusted.push("origin", "main").unwrap();
        trusted.fetch("origin").unwrap();
        // A person's own linked worktree shares the checkout's settings.
        let wt_dir = tempfile::tempdir().unwrap();
        run(d.path(), &["worktree", "add", "-q", &wt_dir.path().join("mine").to_string_lossy()], &[], None).unwrap();
        Repo::open_trusted(&wt_dir.path().join("mine")).unwrap().fetch("origin").unwrap();
    }

    /// SEC-13: a repository the agent puts inside its worktree, with a
    /// clean filter in its config, never runs it when the daemon reads or
    /// commits the agent's changes.
    #[cfg(unix)]
    #[test]
    fn nested_repositories_run_no_filters() {
        let (_d, r) = init();
        let t = tempfile::tempdir().unwrap();
        let marker = t.path().join("filter.ran");
        let filter = format!("sh -c 'echo ran >> \"{}\"; cat'", marker.display());
        let wt_dir = tempfile::tempdir().unwrap();
        let wt = r.add_worktree(&wt_dir.path().join("t1"), "polyph/t1", "main").unwrap();
        let sub = wt.root().join("sub");
        std::fs::create_dir_all(&sub).unwrap();
        let git_in = |dir: &Path, args: &[&str]| {
            let out = Command::new("git").arg("-C").arg(dir).args(args).output().unwrap();
            assert!(out.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&out.stderr));
        };
        git_in(&sub, &["init", "-q"]);
        git_in(&sub, &["config", "filter.x.clean", &filter]);
        std::fs::write(sub.join(".gitattributes"), "* filter=x\n").unwrap();
        std::fs::write(sub.join("f"), "1\n").unwrap();
        git_in(&sub, &["add", "-A"]);
        git_in(&sub, &["-c", "user.name=a", "-c", "user.email=a@b", "commit", "-q", "-m", "s"]);
        std::fs::remove_file(&marker).ok();
        std::fs::write(sub.join("f"), "2\n").unwrap();
        // Plain git does run it, so the test can see the difference.
        git_in(wt.root(), &["add", "-A"]);
        git_in(wt.root(), &["status", "--porcelain"]);
        assert!(marker.exists(), "plain git status ran no filter, so the test shows nothing");
        std::fs::remove_file(&marker).unwrap();
        git_in(wt.root(), &["reset", "-q"]);

        // Untracked: refused with a clear error.
        let e = wt.commit_all("x", &Signature::agent("A", "1")).unwrap_err().to_string();
        assert!(e.contains("sub is a git repository inside the worktree"), "{e}");
        assert!(wt.dirty_files().is_err());
        // A new submodule entry in the index: refused too.
        git_in(wt.root(), &["add", "sub"]);
        let e = wt.commit_all("x", &Signature::agent("A", "1")).unwrap_err().to_string();
        assert!(e.contains("new submodule entry"), "{e}");
        // A `.git` in a folder that has tracked files.
        git_in(wt.root(), &["reset", "-q"]);
        std::fs::rename(sub.join(".git"), t.path().join("sub.git")).unwrap();
        git_in(wt.root(), &["add", "sub"]);
        std::fs::write(sub.join(".git"), format!("gitdir: {}\n", t.path().join("sub.git").display())).unwrap();
        assert!(wt.commit_all("x", &Signature::agent("A", "1")).unwrap_err().to_string().contains("sub is a git"));
        assert!(!marker.exists(), "the filter ran: {}", std::fs::read_to_string(&marker).unwrap_or_default());
        // In the person's own checkout the commands still never look inside.
        git_in(wt.root(), &["reset", "-q"]);
        std::fs::remove_file(sub.join(".git")).unwrap();
        std::fs::rename(t.path().join("sub.git"), sub.join(".git")).unwrap();
        let main = Repo::open(r.root()).unwrap();
        let nested = r.root().join("sub");
        std::fs::rename(&sub, &nested).unwrap();
        git_in(r.root(), &["add", "sub"]);
        git_in(r.root(), &["-c", "user.name=a", "-c", "user.email=a@b", "commit", "-q", "-m", "link"]);
        std::fs::remove_file(&marker).ok();
        std::fs::write(nested.join("f"), "3\n").unwrap();
        main.dirty_files().unwrap();
        std::fs::write(r.root().join("lib.rs"), "fn a() { 3 }\n").unwrap();
        main.commit_all("c", &Signature::agent("A", "1")).unwrap().unwrap();
        assert!(!marker.exists(), "the filter ran: {}", std::fs::read_to_string(&marker).unwrap_or_default());
    }

    #[test]
    fn guard_keeps_the_persons_own_programs() {
        let none = |_: &str| None;
        let empty = |_: &[&str]| vec![];
        let s = guard_settings(&none, &empty);
        assert!(s.contains(&"core.sshCommand=ssh".to_string()), "{s:?}");
        assert!(s.contains(&"core.askPass=".to_string()), "nothing to keep, and the repository's must go: {s:?}");
        assert_eq!(s.iter().filter(|k| k.starts_with("credential.")).count(), 1, "only the reset: {s:?}");

        // GIT_SSH ranks below core.sshCommand, so it is carried over.
        let env = |k: &str| match k {
            "GIT_SSH" => Some("/opt/my ssh/plink".into()),
            "SSH_ASKPASS" => Some("/usr/bin/ksshaskpass".into()),
            _ => None,
        };
        let s = guard_settings(&env, &empty);
        assert!(s.contains(&"core.sshCommand='/opt/my ssh/plink'".to_string()), "{s:?}");
        assert!(s.contains(&"core.askPass=/usr/bin/ksshaskpass".to_string()), "{s:?}");

        // The person's config wins over GIT_SSH, as in git; helpers come back after the reset.
        let config = |a: &[&str]| match a.last().copied() {
            Some("core.sshCommand") => vec!["ssh -i ~/.ssh/a".into(), "ssh -i ~/.ssh/b".into()],
            Some("core.askPass") => vec!["/usr/lib/ssh/x11-ssh-askpass".into()],
            Some(r"^credential\..*helper$") => {
                vec![
                    "credential.helper osxkeychain".into(),
                    "credential.https://github.com.helper !gh auth git-credential".into(),
                ]
            }
            _ => vec![],
        };
        let s = guard_settings(&env, &config);
        assert!(s.contains(&"core.sshCommand=ssh -i ~/.ssh/b".to_string()), "the last value wins: {s:?}");
        assert!(s.contains(&"core.askPass=/usr/lib/ssh/x11-ssh-askpass".to_string()), "{s:?}");
        let cred: Vec<&String> = s.iter().filter(|k| k.starts_with("credential.")).collect();
        assert_eq!(
            cred,
            [
                "credential.helper=",
                "credential.helper=osxkeychain",
                "credential.https://github.com.helper=!gh auth git-credential"
            ]
        );

        // Variables that win over any config need no override.
        let env = |k: &str| matches!(k, "GIT_SSH_COMMAND" | "GIT_ASKPASS").then(|| "x".into());
        let s = guard_settings(&env, &config);
        assert!(!s.iter().any(|k| k.starts_with("core.sshCommand") || k.starts_with("core.askPass")), "{s:?}");
    }

    /// A script that notes each run in `ran` and fails.
    #[cfg(unix)]
    fn tattler(dir: &Path, name: &str) -> (PathBuf, PathBuf) {
        use std::os::unix::fs::PermissionsExt;
        let (script, marker) = (dir.join(name), dir.join(format!("{name}.ran")));
        std::fs::write(&script, format!("#!/bin/sh\necho \"$0 $*\" >> '{}'\nexit 1\n", marker.display())).unwrap();
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
        (script, marker)
    }

    /// Plain git, as the person would run it.
    #[cfg(unix)]
    fn plain_git(dir: &Path, args: &[&str]) {
        let _ = Command::new("git").arg("-C").arg(dir).args(args).env("GIT_TERMINAL_PROMPT", "0").output().unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn agent_writable_fetch_runs_no_repository_programs() {
        let (src, _r) = init();
        let bare = tempfile::tempdir().unwrap();
        let bare_path = bare.path().join("origin.git");
        run(src.path(), &["clone", "-q", "--bare", ".", &bare_path.to_string_lossy()], &[], None).unwrap();
        let out = tempfile::tempdir().unwrap();
        let mine = clone_from(&bare_path.to_string_lossy(), &out.path().join("mine"), None, false).unwrap();
        let t = tempfile::tempdir().unwrap();
        let s = |p: &Path| p.to_string_lossy().into_owned();

        // remote.<name>.uploadpack, run by fetch over a local path or SSH.
        let (pack, pack_ran) = tattler(t.path(), "uploadpack");
        mine.git(&["config", "remote.origin.uploadpack", &s(&pack)]).unwrap();
        plain_git(mine.root(), &["fetch", "origin"]);
        assert!(pack_ran.exists(), "plain git fetch ran no uploadpack, so the test shows nothing");
        std::fs::remove_file(&pack_ran).unwrap();
        mine.fetch("origin").unwrap();
        mine.sync("origin").unwrap();
        assert!(!pack_ran.exists(), "ran: {}", std::fs::read_to_string(&pack_ran).unwrap_or_default());
        // A trusted repository keeps its own settings.
        assert!(Repo::open_trusted(mine.root()).unwrap().fetch("origin").is_err());
        assert!(pack_ran.exists(), "a trusted fetch uses the repository's config");

        // credential.helper, asked for a password by an HTTP remote.
        let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = l.local_addr().unwrap().port();
        std::thread::spawn(move || {
            use std::io::{Read, Write};
            for mut c in l.incoming().flatten() {
                let _ = c.read(&mut [0u8; 8192]);
                let _ = c.write_all(b"HTTP/1.1 401 Unauthorized\r\nWWW-Authenticate: Basic realm=\"x\"\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
            }
        });
        let (helper, helper_ran) = tattler(t.path(), "helper");
        mine.git(&["remote", "add", "web", &format!("http://127.0.0.1:{port}/r.git")]).unwrap();
        mine.git(&["config", "credential.helper", &format!("!{}", s(&helper))]).unwrap();
        mine.git(&["config", &format!("credential.http://127.0.0.1:{port}.helper"), &format!("!{}", s(&helper))])
            .unwrap();
        plain_git(mine.root(), &["fetch", "web"]);
        assert!(helper_ran.exists(), "plain git fetch asked no credential helper, so the test shows nothing");
        std::fs::remove_file(&helper_ran).unwrap();
        assert!(mine.fetch("web").is_err());
        assert!(!helper_ran.exists(), "ran: {}", std::fs::read_to_string(&helper_ran).unwrap_or_default());

        // core.gitProxy, for git:// remotes. Git keeps the first value it
        // reads, so this needs GIT_PROXY_COMMAND rather than a `-c`.
        let (proxy, proxy_ran) = tattler(t.path(), "gitproxy");
        mine.git(&["remote", "add", "daemon", "git://127.0.0.1:1/r.git"]).unwrap();
        mine.git(&["config", "core.gitProxy", &s(&proxy)]).unwrap();
        plain_git(mine.root(), &["fetch", "daemon"]);
        assert!(proxy_ran.exists(), "plain git fetch ran no gitProxy, so the test shows nothing");
        std::fs::remove_file(&proxy_ran).unwrap();
        assert!(mine.fetch("daemon").is_err());
        assert!(!proxy_ran.exists(), "ran: {}", std::fs::read_to_string(&proxy_ran).unwrap_or_default());

        // core.sshCommand, for SSH remotes.
        let (ssh, ssh_ran) = tattler(t.path(), "ssh");
        mine.git(&["remote", "add", "far", "ssh://git@127.0.0.1:1/r.git"]).unwrap();
        mine.git(&["config", "core.sshCommand", &s(&ssh)]).unwrap();
        assert!(mine.fetch("far").is_err());
        assert!(!ssh_ran.exists(), "ran: {}", std::fs::read_to_string(&ssh_ran).unwrap_or_default());
    }

    #[cfg(unix)]
    #[test]
    fn trusted_push_keeps_the_repositorys_ssh_command_and_hooks() {
        let (d, r) = init();
        let t = tempfile::tempdir().unwrap();
        // A per-repository SSH command (a deploy key, say).
        let (ssh, ssh_ran) = tattler(t.path(), "ssh");
        r.git(&["remote", "add", "far", "ssh://git@example.invalid/r.git"]).unwrap();
        r.git(&["config", "core.sshCommand", &ssh.to_string_lossy()]).unwrap();
        assert!(Repo::open_trusted(d.path()).unwrap().push("far", "main").is_err());
        let ran = std::fs::read_to_string(&ssh_ran).unwrap_or_default();
        assert!(ran.contains("example.invalid"), "the repository's core.sshCommand did not run: {ran:?}");

        // A pre-push hook (Git LFS installs one).
        let bare = t.path().join("origin.git");
        run(d.path(), &["init", "-q", "--bare", &bare.to_string_lossy()], &[], None).unwrap();
        r.git(&["remote", "add", "origin", &bare.to_string_lossy()]).unwrap();
        let hooks = r.common_dir().unwrap().join("hooks");
        std::fs::create_dir_all(&hooks).unwrap();
        let marker = t.path().join("pre-push.ran");
        std::fs::write(hooks.join("pre-push"), format!("#!/bin/sh\ntouch '{}'\n", marker.display())).unwrap();
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(hooks.join("pre-push"), std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        r.push("origin", "main").unwrap();
        assert!(!marker.exists(), "a repository an agent can write runs no hooks");
        // Something to push again (git runs no pre-push when up to date).
        run(&bare, &["update-ref", "-d", "refs/heads/main"], &[], None).unwrap();
        Repo::open_trusted(d.path()).unwrap().push("origin", "main").unwrap();
        assert!(marker.exists(), "the trusted push skipped the repository's pre-push hook");
    }

    #[test]
    fn sandbox_git_keeps_the_git_directory_read_only() {
        let (d, r) = init();
        let common = canonical(&r.common_dir().unwrap()).unwrap();
        // The person's own checkout: all of `.git`, whether it commits or not.
        let g = sandbox_git(d.path(), None);
        assert_eq!(g, SandboxGit { writable: vec![], read_only: vec![common.clone()], env: vec![] });
        assert_eq!(sandbox_git(d.path(), Some("main")), g, "agents commit only in a task's worktree");

        // A task's worktree: its task's folders in refs and logs and its own
        // folder (with its own object store), but not the shared `objects/`,
        // the files in its folder that name the repository or hold its
        // settings, its store's `info/`, or the other branches in its
        // task's folders.
        let wt_dir = tempfile::tempdir().unwrap();
        let wt = r.add_worktree(&wt_dir.path().join("t1"), "polyph/t1/work", "main").unwrap();
        r.add_worktree(&wt_dir.path().join("t2"), "polyph/t2/work", "main").unwrap();
        r.git(&["branch", "polyph/t1/other", "main"]).unwrap();
        let own = common.join("worktrees").join("t1");
        let (refs, logs) = (common.join("refs/heads/polyph/t1"), common.join("logs/refs/heads/polyph/t1"));
        let g = sandbox_git(wt.root(), Some("polyph/t1/work"));
        assert_eq!(g.writable, vec![refs.clone(), logs.clone(), own.clone(), own.join("objects")]);
        let mut ro = vec![common.clone(), wt.root().join(".git")];
        ro.extend(["commondir", "gitdir", "config.worktree", "objects/info"].iter().map(|f| own.join(f)));
        ro.extend([refs.join("other"), logs.join("other")]);
        assert_eq!(g.read_only, ro);
        assert!(own.join("config.worktree").is_file(), "created so the agent cannot");
        let alt = common.join("objects").to_string_lossy().into_owned();
        assert_eq!(
            g.env,
            [
                ("GIT_OBJECT_DIRECTORY".to_string(), own.join("objects").to_string_lossy().into_owned()),
                ("GIT_ALTERNATE_OBJECT_DIRECTORIES".to_string(), alt)
            ]
        );
        // Never `objects/`, `refs/`, `logs/`, `refs/heads/` or the shared
        // `polyph/` folders as a whole.
        for p in [
            common.join("objects"),
            common.join("refs"),
            common.join("logs"),
            common.join("refs/heads"),
            common.join("logs/refs"),
            common.join("refs/heads/polyph"),
            common.join("logs/refs/heads/polyph"),
        ] {
            assert!(!g.writable.iter().any(|w| p.starts_with(w)), "{p:?} is writable");
        }
        // A branch right in refs/heads shares its folder with main, and one
        // outside a task folder shares it with other tasks: no commits.
        r.git(&["branch", "flat", "main"]).unwrap();
        assert!(sandbox_git(wt.root(), Some("flat")).writable.is_empty());
        r.git(&["branch", "polyph/t1x", "main"]).unwrap();
        assert!(sandbox_git(wt.root(), Some("polyph/t1x")).writable.is_empty());
        assert!(sandbox_git(wt.root(), Some("polyph/t1x")).env.is_empty());
        // Checks in a worktree commit nothing.
        assert_eq!(sandbox_git(wt.root(), None).read_only, vec![common.clone(), wt.root().join(".git")]);
        assert!(sandbox_git(wt.root(), None).writable.is_empty());
        assert_eq!(sandbox_git(tempfile::tempdir().unwrap().path(), Some("polyph/t1/work")), SandboxGit::default());
    }

    /// SEC-13: git appends to reflogs through links. A reflog, a ref or a
    /// worktree's file replaced with a link stops every command that writes.
    #[cfg(unix)]
    #[test]
    fn links_in_the_git_directory_stop_writes() {
        let t = tempfile::tempdir().unwrap();
        let outside = t.path().join("profile");
        for planted in ["logs/HEAD", "logs/refs/heads/polyph/t1", "worktrees/t1/logs/HEAD", "refs/heads/polyph/x"] {
            let (_d, r) = init();
            let wt_dir = tempfile::tempdir().unwrap();
            let wt = r.add_worktree(&wt_dir.path().join("t1"), "polyph/t1", "main").unwrap();
            std::fs::write(&outside, "export PATH\n").unwrap();
            let p = r.common_dir().unwrap().join(planted);
            let _ = std::fs::remove_file(&p);
            std::os::unix::fs::symlink(&outside, &p).unwrap();
            std::fs::write(wt.root().join("new.rs"), "fn n() {}\n").unwrap();
            let who = Signature::agent("A", "1");
            let err = wt.commit_task("polyph/t1", "work", &who).unwrap_err().to_string();
            assert!(err.contains(planted), "{planted}: {err}");
            std::fs::write(r.root().join("lib.rs"), "fn a() { 9 }\n").unwrap();
            let err = r.commit_all("person", &who).unwrap_err().to_string();
            assert!(err.contains(planted), "{planted}: {err}");
            let head = r.rev_parse("HEAD").unwrap();
            assert!(r.advance_branch("polyph/t1", &head, &head).is_err(), "{planted}");
            assert!(r.exclude("/x/").is_err(), "{planted}");
            assert_eq!(std::fs::read_to_string(&outside).unwrap(), "export PATH\n", "{planted}");
            // Reading still works.
            assert!(r.rev_parse("HEAD").is_ok());
        }
        // A hard link to a file outside, in a reflog.
        let (_d, r) = init();
        std::fs::write(&outside, "export PATH\n").unwrap();
        let log = r.common_dir().unwrap().join("logs/refs/heads/main");
        std::fs::remove_file(&log).unwrap();
        std::fs::hard_link(&outside, &log).unwrap();
        std::fs::write(r.root().join("lib.rs"), "fn a() { 9 }\n").unwrap();
        let err = r.commit_all("person", &Signature::agent("A", "1")).unwrap_err().to_string();
        assert!(err.contains("logs/refs/heads/main"), "{err}");
        assert_eq!(std::fs::read_to_string(&outside).unwrap(), "export PATH\n");
    }

    /// SEC-13: `objects/info/alternates` makes git read another repository's
    /// objects; the daemon creates none and refuses to run git with one.
    #[test]
    fn alternates_are_refused() {
        let (o, other) = init();
        std::fs::write(o.path().join("lib.rs"), "fn theirs() {}\n").unwrap();
        other.commit_all("theirs", &Signature::agent("Bo", "1")).unwrap();
        let (_d, r) = init();
        let info = r.common_dir().unwrap().join("objects/info");
        std::fs::create_dir_all(&info).unwrap();
        let theirs = other.rev_parse("HEAD").unwrap();
        assert!(r.show(&theirs, "lib.rs").is_none());
        std::fs::write(info.join("alternates"), format!("{}\n", other.common_dir().unwrap().join("objects").display()))
            .unwrap();
        let err = r.rev_parse("HEAD").unwrap_err().to_string();
        assert!(err.contains("alternates"), "{err}");
        assert!(r.show(&theirs, "lib.rs").is_none(), "read another repository's objects");
        assert!(r.commit_all("x", &Signature::agent("A", "1")).is_err());
    }

    /// SEC-13: the daemon commits a task's work on the task's branch only,
    /// whatever the worktree's HEAD (which the agent writes) names.
    #[test]
    fn task_commits_go_to_the_task_branch_only() {
        let (_d, r) = init();
        let wt_dir = tempfile::tempdir().unwrap();
        let wt = r.add_worktree(&wt_dir.path().join("t1"), "polyph/t1", "main").unwrap();
        let main = r.rev_parse("main").unwrap();
        std::fs::write(wt.root().join("new.rs"), "fn n() {}\n").unwrap();
        let who = Signature::agent("A", "1");
        let c = wt.commit_task("polyph/t1", "Add n\n", &who).unwrap().unwrap();
        assert_eq!(r.rev_parse("polyph/t1").unwrap(), c);
        assert_eq!(r.log_subjects("main", "polyph/t1").unwrap(), ["Add n"]);
        assert!(wt.dirty_files().unwrap().is_empty());
        assert_eq!(wt.commit_task("polyph/t1", "again", &who).unwrap(), None);
        r.check_branch_logged("polyph/t1").unwrap();

        // The agent points HEAD at main and leaves work behind.
        let head = r.common_dir().unwrap().join("worktrees/t1/HEAD");
        std::fs::write(&head, "ref: refs/heads/main\n").unwrap();
        std::fs::write(wt.root().join("evil.rs"), "fn e() {}\n").unwrap();
        let err = wt.commit_task("polyph/t1", "x", &who).unwrap_err().to_string();
        assert!(err.contains("HEAD is refs/heads/main"), "{err}");
        assert_eq!(r.rev_parse("main").unwrap(), main, "main moved");
    }

    /// SEC-13: a branch written without git (as an agent with `refs/`
    /// writable would) is not merged into, nor moved.
    #[test]
    fn a_branch_changed_without_git_is_not_merged_into() {
        let (d, r) = init();
        r.check_branch_logged("main").unwrap();
        let first = r.rev_parse("main").unwrap();
        std::fs::write(d.path().join("lib.rs"), "fn a() { 1 }\n").unwrap();
        let second = r.commit_all("person", &Signature::agent("Ann", "0")).unwrap().unwrap();
        r.check_branch_logged("main").unwrap();
        // An "agent" makes a commit and writes main's ref file directly.
        let env = sig_env(&Signature::agent("Evil", "1"));
        let env: Vec<(&str, &str)> = env.iter().map(|(k, v)| (*k, v.as_str())).collect();
        let evil = r
            .git_env(&["commit-tree", &format!("{second}^{{tree}}"), "-p", &second, "-m", "evil"], &env, None)
            .unwrap()
            .trim()
            .to_string();
        let common = r.common_dir().unwrap();
        std::fs::write(common.join("refs/heads/main"), format!("{evil}\n")).unwrap();
        let err = r.check_branch_logged("main").unwrap_err().to_string();
        assert!(err.contains("refusing to merge into main") && err.contains("reflog"), "{err}");
        assert!(r.advance_branch("main", &first, &evil).is_err());
        // Moved back by git: fine again.
        r.git(&["update-ref", "refs/heads/main", &second]).unwrap();
        r.check_branch_logged("main").unwrap();
    }

    /// Plain git in a task worktree as its agent runs it: objects in the
    /// worktree's own store, the shared one an alternate.
    fn agent_git(wt: &Repo, args: &[&str]) -> String {
        let env = sandbox_git(wt.root(), wt.current_branch().as_deref()).env;
        assert!(!env.is_empty(), "no object store of its own");
        let out = Command::new("git")
            .arg("-C")
            .arg(wt.root())
            .args(["-c", "user.name=a", "-c", "user.email=a@b", "-c", "commit.gpgSign=false"])
            .args(args)
            .envs(env)
            .output()
            .unwrap();
        assert!(out.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&out.stderr));
        String::from_utf8_lossy(&out.stdout).trim().to_string()
    }

    /// The loose object file of `oid` in the object store `objects`.
    fn loose(objects: &Path, oid: &str) -> PathBuf {
        objects.join(&oid[..2]).join(&oid[2..])
    }

    /// SEC-13: a task agent's objects stay in its worktree's own store. The
    /// daemon takes its commits into the shared store only through
    /// `index-pack --strict`, which names objects by their content, so a
    /// blob the agent swaps (before or after) never reaches the shared
    /// store, a checkout, or the fast-forward of the person's checkout.
    #[test]
    fn swapped_task_objects_never_reach_the_shared_store() {
        let (d, r) = init();
        let common = canonical(&r.common_dir().unwrap()).unwrap();
        let shared = common.join("objects");
        let wt_dir = tempfile::tempdir().unwrap();
        let wt = r.add_worktree(&wt_dir.path().join("t1"), "polyph/t1/work", "main").unwrap();
        let own = common.join("worktrees/t1/objects");
        assert!(own.join("info").is_dir(), "add_worktree makes the task's own store");

        // The agent commits; its objects are in its own store only.
        std::fs::write(wt.root().join("a.rs"), "fn good() {}\n").unwrap();
        agent_git(&wt, &["add", "a.rs"]);
        agent_git(&wt, &["commit", "-q", "-m", "good"]);
        let tip = agent_git(&wt, &["rev-parse", "HEAD"]);
        let good = agent_git(&wt, &["rev-parse", "HEAD:a.rs"]);
        assert!(loose(&own, &good).is_file() && !loose(&shared, &good).exists());
        assert!(r.show(&tip, "a.rs").is_none(), "the shared store has the agent's commit already");

        // Before the daemon takes it: the blob's file now holds other content.
        std::fs::write(wt.root().join("evil.txt"), "fn evil() {}\n").unwrap();
        let evil = agent_git(&wt, &["hash-object", "-w", "evil.txt"]);
        std::fs::remove_file(wt.root().join("evil.txt")).unwrap();
        let (good_file, evil_file) = (loose(&own, &good), loose(&own, &evil));
        let good_bytes = std::fs::read(&good_file).unwrap();
        std::fs::remove_file(&good_file).unwrap();
        std::fs::copy(&evil_file, &good_file).unwrap();
        assert_eq!(agent_git(&wt, &["cat-file", "blob", &good]), "fn evil() {}", "git reads the swap as it is");
        let who = Signature::agent("A", "1");
        let err = wt.commit_task("polyph/t1/work", "x", &who).unwrap_err().to_string();
        assert!(err.contains("shared object store"), "{err}");
        assert!(r.show(&tip, "a.rs").is_none(), "a commit with a swapped blob reached the shared store");
        assert!(!loose(&shared, &good).exists());

        // Put back: the commit goes in, with its true content.
        std::fs::remove_file(&good_file).unwrap();
        std::fs::write(&good_file, &good_bytes).unwrap();
        assert_eq!(wt.commit_task("polyph/t1/work", "x", &who).unwrap(), None);
        assert_eq!(r.show(&tip, "a.rs").as_deref(), Some("fn good() {}\n"));

        // After: the agent swaps its copy again. The merge, the checkout and
        // the fast-forward of the person's checkout read the shared store.
        std::fs::remove_file(&good_file).unwrap();
        std::fs::copy(&evil_file, &good_file).unwrap();
        let old = r.rev_parse("main").unwrap();
        let out = r.merge("main", &tip, "merge", &Signature::agent("P", "q"), &mut |_| None).unwrap();
        let MergeOutcome::Merged { commit, .. } = out else { panic!("{out:?}") };
        r.advance_branch("main", &commit, &old).unwrap();
        assert_eq!(std::fs::read_to_string(d.path().join("a.rs")).unwrap(), "fn good() {}\n");
        let check = r.add_detached(&wt_dir.path().join("check"), &commit).unwrap();
        assert_eq!(std::fs::read_to_string(check.root().join("a.rs")).unwrap(), "fn good() {}\n");
    }

    /// SEC-13: while the daemon commits a task's work, its agent keeps
    /// flipping the branch's reflog between a file and a link to a file of
    /// the person's. The daemon's git writes run in a sandbox where that
    /// file is read-only, so no run writes it, whichever way the race goes.
    #[cfg(unix)]
    #[test]
    fn a_racing_reflog_link_never_writes_outside() {
        if confine::confinement() == confine::Confinement::None {
            eprintln!("skipped: no sandbox for the daemon's git writes on this machine");
            return;
        }
        use std::sync::atomic::{AtomicBool, Ordering};
        let (_d, r) = init();
        let wt_dir = tempfile::tempdir().unwrap();
        let wt = r.add_worktree(&wt_dir.path().join("t1"), "polyph/t1/work", "main").unwrap();
        let home = tempfile::tempdir().unwrap();
        let profile = home.path().join(".profile");
        std::fs::write(&profile, "export PATH\n").unwrap();
        let log = r.common_dir().unwrap().join("logs/refs/heads/polyph/t1/work");
        let stop = std::sync::Arc::new(AtomicBool::new(false));
        let flipper = {
            let (stop, log, profile) = (stop.clone(), log.clone(), profile.clone());
            std::thread::spawn(move || {
                let mut flips = 0u64;
                while !stop.load(Ordering::Relaxed) {
                    let _ = std::fs::remove_file(&log);
                    let _ = std::fs::write(&log, "");
                    let _ = std::fs::remove_file(&log);
                    let _ = std::os::unix::fs::symlink(&profile, &log);
                    flips += 1;
                }
                flips
            })
        };
        let who = Signature::agent("A", "1");
        let (mut ok, mut refused) = (0, 0);
        for i in 0..150 {
            std::fs::write(wt.root().join("n.rs"), format!("fn n() {{ {i} }}\n")).unwrap();
            match wt.commit_task("polyph/t1/work", &format!("; touch PWNED {i}"), &who) {
                Ok(_) => ok += 1,
                Err(_) => refused += 1,
            }
            assert_eq!(std::fs::read_to_string(&profile).unwrap(), "export PATH\n", "run {i} wrote through the link");
        }
        stop.store(true, Ordering::Relaxed);
        let flips = flipper.join().unwrap();
        eprintln!("{ok} commits, {refused} refused, {flips} flips");
        assert!(flips > 0);
    }
}
