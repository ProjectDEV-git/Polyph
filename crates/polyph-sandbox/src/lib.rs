//! Sandboxing for agents and everything they spawn.
//!
//! | Platform | Mechanism |
//! |---|---|
//! | Linux | bubblewrap with a private network namespace; Landlock when bubblewrap is missing |
//! | macOS | a Seatbelt profile run with `sandbox-exec` |
//! | Windows | low integrity level (writes only to the worktree, writable paths and TMPDIR), a job object and proxy settings (see `docs/sandbox.md` for what this does not cover) |
//! | Any | rootless Podman or Docker in "container mode" |
//!
//! Outbound network always goes through [`proxy`], which passes allowlisted
//! hosts and turns everything else into an approval request.

pub mod helper;
#[cfg(windows)]
pub mod lowil;
pub mod proxy;
pub mod selftest;

use std::path::{Path, PathBuf};

use serde::Serialize;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Mechanism {
    Bubblewrap,
    Landlock,
    Seatbelt,
    WindowsJob,
    Container,
    /// No isolation. Only used when the user explicitly turns the sandbox off.
    None,
}

#[derive(Clone, Debug)]
pub struct Policy {
    /// The agent's worktree: readable and writable.
    pub worktree: PathBuf,
    /// Other writable paths (the agent's own cache and config directory).
    pub writable: Vec<PathBuf>,
    /// Private temporary directory, exported as TMPDIR. Shared /tmp is not writable.
    pub tmp: Option<PathBuf>,
    /// Paths hidden from the agent entirely where the mechanism allows it.
    pub hidden: Vec<PathBuf>,
    /// Paths that stay read-only inside the writable ones (the git
    /// directory of the agent's checkout, see `polyph_git::sandbox_git`).
    /// Enforced by bubblewrap and Seatbelt only; Landlock, Windows and
    /// container mode ignore it.
    pub read_only: Vec<PathBuf>,
    /// Folders of the shared git directory a task agent writes to commit
    /// (`objects/`, `refs/`, `logs/`, its own `worktrees/<name>/`), each
    /// mounted writable over `read_only`, with the `read_only` paths inside
    /// them read-only again. Bubblewrap and Seatbelt only; elsewhere the
    /// agent cannot commit.
    pub git_writable: Vec<PathBuf>,
    /// Daemon proxy: loopback TCP address and (Unix) socket path.
    pub proxy_tcp: Option<std::net::SocketAddr>,
    pub proxy_unix: Option<PathBuf>,
    /// The helper binary (`polyph-sandbox-exec` or `polyph __sandbox`).
    pub helper: Vec<String>,
    /// Container image for container mode.
    pub container_image: Option<String>,
}

impl Policy {
    /// Default secret locations hidden from agents, plus Polyph's own data.
    pub fn default_hidden(polyph_home: &Path) -> Vec<PathBuf> {
        let mut v = vec![polyph_home.to_path_buf()];
        if let Some(home) = home_dir() {
            for d in
                [".ssh", ".aws", ".gnupg", ".config/gcloud", ".azure", ".kube", ".docker", ".netrc", ".git-credentials"]
            {
                v.push(home.join(d));
            }
        }
        v
    }

    fn writable_all(&self) -> Vec<PathBuf> {
        let mut v = vec![self.worktree.clone()];
        v.extend(self.writable.iter().cloned());
        v.extend(self.tmp.iter().cloned());
        v
    }

    /// The `read_only` paths that exist inside a writable path (others are
    /// read-only anyway), each with its folder and whether it lies in a
    /// `git_writable` folder (then it goes on after those). The folder must
    /// not be renamed away either, or the agent could put a writable copy in
    /// its place (`mv .git .old && cp -r .old .git`).
    fn read_only_inside(&self) -> Vec<(PathBuf, PathBuf, bool)> {
        let writable = self.writable_all();
        let git = self.git_writable_dirs();
        self.read_only
            .iter()
            .filter(|r| writable.iter().chain(&git).any(|w| r.starts_with(w) && *r != w))
            .filter(|r| std::fs::symlink_metadata(r).is_ok_and(|m| !m.file_type().is_symlink()))
            .filter_map(|r| {
                let late = git.iter().any(|g| r.starts_with(g) && r != g);
                Some((r.clone(), r.parent()?.to_path_buf(), late))
            })
            .collect()
    }

    /// The `git_writable` folders that exist and are not links.
    fn git_writable_dirs(&self) -> Vec<PathBuf> {
        self.git_writable
            .iter()
            .filter(|g| std::fs::symlink_metadata(g).is_ok_and(|m| m.is_dir()))
            .cloned()
            .collect()
    }

    fn tmp_env(&self) -> Vec<(String, String)> {
        match &self.tmp {
            Some(t) => {
                ["TMPDIR", "TEMP", "TMP"].iter().map(|k| (k.to_string(), t.to_string_lossy().into_owned())).collect()
            }
            None => vec![],
        }
    }
}

pub fn home_dir() -> Option<PathBuf> {
    std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" }).map(PathBuf::from)
}

/// A command ready to spawn.
#[derive(Clone, Debug)]
pub struct Wrapped {
    pub program: String,
    pub args: Vec<String>,
    pub env: Vec<(String, String)>,
    pub cwd: PathBuf,
    pub mechanism: Mechanism,
}

/// Port the helper listens on inside a private network namespace.
pub const INNER_PROXY_PORT: u16 = 3128;

fn proxy_env(url: &str) -> Vec<(String, String)> {
    let mut v = vec![];
    for k in ["HTTP_PROXY", "HTTPS_PROXY", "http_proxy", "https_proxy", "ALL_PROXY", "all_proxy"] {
        v.push((k.to_string(), url.to_string()));
    }
    // Everything, local services included, goes through the proxy so the
    // allowlist applies (and inside a private namespace localhost is empty).
    v.push(("NO_PROXY".into(), String::new()));
    v.push(("no_proxy".into(), String::new()));
    v
}

/// Wraps `program args` so it runs under `mech` with `policy`.
pub fn wrap(mech: Mechanism, policy: &Policy, program: &str, args: &[String]) -> anyhow::Result<Wrapped> {
    let mut w = wrap_inner(mech, policy, program, args)?;
    if mech != Mechanism::Bubblewrap && mech != Mechanism::Container {
        w.env.extend(policy.tmp_env());
    }
    Ok(w)
}

fn wrap_inner(mech: Mechanism, policy: &Policy, program: &str, args: &[String]) -> anyhow::Result<Wrapped> {
    let cwd = policy.worktree.clone();
    let tcp_url = policy.proxy_tcp.map(|a| format!("http://{a}")).unwrap_or_default();
    match mech {
        Mechanism::None => {
            Ok(Wrapped { program: program.into(), args: args.to_vec(), env: proxy_env(&tcp_url), cwd, mechanism: mech })
        }
        Mechanism::Bubblewrap => {
            let sock = policy
                .proxy_unix
                .clone()
                .ok_or_else(|| anyhow::anyhow!("bubblewrap mode needs the proxy's Unix socket"))?;
            let mut a: Vec<String> = vec![
                "--die-with-parent".into(),
                "--new-session".into(),
                "--unshare-net".into(),
                "--unshare-pid".into(),
                "--ro-bind".into(),
                "/".into(),
                "/".into(),
                "--dev".into(),
                "/dev".into(),
                "--proc".into(),
                "/proc".into(),
                "--tmpfs".into(),
                "/tmp".into(),
            ];
            let writable = policy.writable_all();
            for h in &policy.hidden {
                // Never hide something that contains a writable path.
                if h.exists() && !writable.iter().any(|w| w.starts_with(h)) {
                    let p = h.to_string_lossy().into_owned();
                    if h.is_dir() {
                        a.extend(["--tmpfs".into(), p]);
                    } else {
                        a.extend(["--ro-bind".into(), "/dev/null".into(), p]);
                    }
                }
            }
            for w in &writable {
                if w.exists() {
                    let p = w.to_string_lossy().into_owned();
                    a.extend(["--bind".into(), p.clone(), p]);
                }
            }
            // Each folder bound onto itself is a mount point, which cannot
            // be renamed or removed; then the read-only paths over it, the
            // git folders a task agent commits into, and the read-only
            // files inside those.
            let ro = policy.read_only_inside();
            let git = policy.git_writable_dirs();
            let mut pinned: Vec<&PathBuf> = vec![];
            for (_, dir, _) in &ro {
                if !writable.contains(dir) && !git.contains(dir) && !pinned.contains(&dir) {
                    pinned.push(dir);
                    let p = dir.to_string_lossy().into_owned();
                    a.extend(["--bind".into(), p.clone(), p]);
                }
            }
            for late in [false, true] {
                if late {
                    for g in &git {
                        let p = g.to_string_lossy().into_owned();
                        a.extend(["--bind".into(), p.clone(), p]);
                    }
                }
                for (r, _, _) in ro.iter().filter(|(_, _, l)| *l == late) {
                    let p = r.to_string_lossy().into_owned();
                    a.extend(["--ro-bind".into(), p.clone(), p]);
                }
            }
            let s = sock.to_string_lossy().into_owned();
            a.extend(["--bind".into(), s.clone(), s.clone()]);
            a.extend(["--chdir".into(), cwd.to_string_lossy().into_owned(), "--".into()]);
            a.extend(policy.helper.iter().cloned());
            a.extend([
                "forward".into(),
                "--sock".into(),
                s,
                "--port".into(),
                INNER_PROXY_PORT.to_string(),
                "--".into(),
                program.into(),
            ]);
            a.extend(args.iter().cloned());
            Ok(Wrapped {
                program: "bwrap".into(),
                args: a,
                env: proxy_env(&format!("http://127.0.0.1:{INNER_PROXY_PORT}")),
                cwd,
                mechanism: mech,
            })
        }
        Mechanism::Landlock => {
            let port = policy
                .proxy_tcp
                .map(|a| a.port())
                .ok_or_else(|| anyhow::anyhow!("Landlock mode needs the proxy's TCP port"))?;
            let helper = policy.helper.first().cloned().ok_or_else(|| anyhow::anyhow!("no sandbox helper"))?;
            let mut a: Vec<String> = policy.helper[1..].to_vec();
            a.extend(["landlock".into(), "--proxy-port".into(), port.to_string()]);
            for w in policy.writable_all() {
                a.extend(["--write".into(), w.to_string_lossy().into_owned()]);
            }
            a.push("--".into());
            a.push(program.into());
            a.extend(args.iter().cloned());
            Ok(Wrapped { program: helper, args: a, env: proxy_env(&tcp_url), cwd, mechanism: mech })
        }
        Mechanism::Seatbelt => {
            let port = policy.proxy_tcp.map(|a| a.port()).unwrap_or(0);
            let profile = seatbelt_profile(policy, port);
            let mut a = vec!["-p".to_string(), profile, program.to_string()];
            a.extend(args.iter().cloned());
            Ok(Wrapped {
                program: "/usr/bin/sandbox-exec".into(),
                args: a,
                env: proxy_env(&tcp_url),
                cwd,
                mechanism: mech,
            })
        }
        Mechanism::WindowsJob => {
            // Through the helper, which runs the command at low integrity
            // level so it can write only to the policy's folders.
            let Some(helper) = policy.helper.first().cloned() else {
                return Ok(Wrapped {
                    program: program.into(),
                    args: args.to_vec(),
                    env: proxy_env(&tcp_url),
                    cwd,
                    mechanism: mech,
                });
            };
            let mut a: Vec<String> = policy.helper[1..].to_vec();
            a.push("lowil".into());
            for w in policy.writable_all() {
                a.extend(["--write".into(), w.to_string_lossy().into_owned()]);
            }
            a.push("--".into());
            a.push(program.into());
            a.extend(args.iter().cloned());
            Ok(Wrapped { program: helper, args: a, env: proxy_env(&tcp_url), cwd, mechanism: mech })
        }
        Mechanism::Container => {
            let engine = container_engine().ok_or_else(|| anyhow::anyhow!("container mode needs podman or docker"))?;
            let image = policy.container_image.clone().unwrap_or_else(|| "docker.io/library/debian:stable-slim".into());
            let sock = policy
                .proxy_unix
                .clone()
                .ok_or_else(|| anyhow::anyhow!("container mode needs the proxy's Unix socket"))?;
            let wt = cwd.to_string_lossy().into_owned();
            let helper = policy.helper.first().cloned().unwrap_or_default();
            let s = sock.to_string_lossy().into_owned();
            let mut a = vec![
                "run".into(),
                "--rm".into(),
                "-i".into(),
                "--network".into(),
                "none".into(),
                "-v".into(),
                format!("{wt}:{wt}"),
                "-w".into(),
                wt.clone(),
                "-v".into(),
                format!("{s}:{s}"),
                "-v".into(),
                format!("{helper}:/usr/local/bin/polyph-sandbox-exec:ro"),
            ];
            if engine.ends_with("podman") {
                a.extend(["--userns".into(), "keep-id".into()]);
            }
            for (k, v) in proxy_env(&format!("http://127.0.0.1:{INNER_PROXY_PORT}")) {
                a.extend(["-e".into(), format!("{k}={v}")]);
            }
            a.extend([
                image,
                "/usr/local/bin/polyph-sandbox-exec".into(),
                "forward".into(),
                "--sock".into(),
                s,
                "--port".into(),
                INNER_PROXY_PORT.to_string(),
                "--".into(),
                program.into(),
            ]);
            a.extend(args.iter().cloned());
            Ok(Wrapped { program: engine, args: a, env: vec![], cwd, mechanism: mech })
        }
    }
}

fn sb_quote(p: &Path) -> String {
    p.to_string_lossy().replace('\\', "\\\\").replace('"', "\\\"")
}

/// Seatbelt profile: everything readable except hidden paths; writes only to
/// the worktree, writable paths and temp; network only to the proxy.
pub fn seatbelt_profile(policy: &Policy, proxy_port: u16) -> String {
    let mut s = String::from("(version 1)\n(allow default)\n(deny file-write*)\n");
    // Device files only; shared temp folders (/private/tmp, /private/var/folders)
    // stay read-only: the agent gets its own TMPDIR through the policy.
    s.push_str("(allow file-write* (literal \"/dev/null\") (literal \"/dev/tty\") (regex #\"^/dev/fd/\"))\n");
    for w in policy.writable_all() {
        s.push_str(&format!("(allow file-write* (subpath \"{}\"))\n", sb_quote(&w)));
        // Seatbelt matches resolved paths (/var is /private/var on macOS).
        if let Ok(real) = w.canonicalize()
            && real != w
        {
            s.push_str(&format!("(allow file-write* (subpath \"{}\"))\n", sb_quote(&real)));
        }
    }
    // Later rules win: read-only paths inside the writable ones, and their
    // folders themselves (not what is in them), so they cannot be renamed.
    // A writable root's own folder cannot be renamed: its parent is
    // read-only. Then the git folders a task agent commits into (not the
    // folders themselves), and the read-only files inside those.
    let writable = policy.writable_all();
    let git = policy.git_writable_dirs();
    let ro = policy.read_only_inside();
    let mut rules: Vec<String> = vec![];
    let mut push = |rule: String| {
        if !rules.contains(&rule) {
            rules.push(rule);
        }
    };
    let both = |p: &Path| {
        let real = p.canonicalize().ok().filter(|real| real != p);
        std::iter::once(p.to_path_buf()).chain(real).collect::<Vec<_>>()
    };
    for late in [false, true] {
        if late {
            for g in &git {
                for p in both(g) {
                    push(format!("(allow file-write* (subpath \"{}\"))\n", sb_quote(&p)));
                }
                for p in both(g) {
                    push(format!("(deny file-write* (literal \"{}\"))\n", sb_quote(&p)));
                }
            }
        }
        for (r, dir, _) in ro.iter().filter(|(_, _, l)| *l == late) {
            let filter = if r.is_dir() { "subpath" } else { "literal" };
            let dir = (!writable.contains(dir) && !git.contains(dir)).then_some(("literal", dir.clone()));
            for (f, p) in std::iter::once((filter, r.clone())).chain(dir) {
                for p in both(&p) {
                    push(format!("(deny file-write* ({f} \"{}\"))\n", sb_quote(&p)));
                }
            }
        }
    }
    for r in rules {
        s.push_str(&r);
    }
    for h in &policy.hidden {
        s.push_str(&format!("(deny file-read* (subpath \"{}\"))\n", sb_quote(h)));
    }
    s.push_str("(deny network-outbound)\n(allow network-outbound (remote unix-socket))\n");
    if proxy_port > 0 {
        s.push_str(&format!("(allow network-outbound (remote ip \"localhost:{proxy_port}\"))\n"));
    }
    s
}

pub fn container_engine() -> Option<String> {
    ["podman", "docker"].iter().find_map(|e| which::which(e).ok()).map(|p| p.to_string_lossy().into_owned())
}

#[derive(Clone, Debug, Serialize)]
pub struct Availability {
    pub mechanism: Mechanism,
    pub available: bool,
    pub detail: String,
}

/// Which mechanisms this machine supports, best first.
pub fn detect(helper: &[String]) -> Vec<Availability> {
    let mut v = vec![];
    if cfg!(target_os = "linux") {
        let bwrap = which::which("bwrap").ok();
        let works = bwrap.is_some()
            && std::process::Command::new("bwrap")
                .args(["--ro-bind", "/", "/", "--unshare-net", "--dev", "/dev", "--proc", "/proc", "true"])
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .status()
                .is_ok_and(|s| s.success());
        v.push(Availability {
            mechanism: Mechanism::Bubblewrap,
            available: works,
            detail: match (bwrap.is_some(), works) {
                (true, true) => "bubblewrap works".into(),
                (true, false) => "bubblewrap is installed but cannot create namespaces here".into(),
                _ => "bubblewrap is not installed".into(),
            },
        });
        let ll = helper.first().is_some_and(|h| {
            std::process::Command::new(h)
                .args(&helper[1..])
                .args(["probe", "landlock"])
                .status()
                .is_ok_and(|s| s.success())
        });
        v.push(Availability {
            mechanism: Mechanism::Landlock,
            available: ll,
            detail: if ll {
                "Landlock is enforced by this kernel".into()
            } else {
                "Landlock is not available in this kernel".into()
            },
        });
    }
    if cfg!(target_os = "macos") {
        let ok = Path::new("/usr/bin/sandbox-exec").exists();
        v.push(Availability {
            mechanism: Mechanism::Seatbelt,
            available: ok,
            detail: if ok { "sandbox-exec works".into() } else { "sandbox-exec is missing".into() },
        });
    }
    if cfg!(windows) {
        v.push(Availability {
            mechanism: Mechanism::WindowsJob,
            available: true,
            detail: "low integrity level and a job object; network isolation is limited".into(),
        });
    }
    let c = container_engine();
    v.push(Availability {
        mechanism: Mechanism::Container,
        available: c.is_some(),
        detail: c.unwrap_or_else(|| "neither podman nor docker is installed".into()),
    });
    v
}

/// The strongest mechanism available, or `None` (callers must refuse to run
/// agents unsandboxed unless the user allowed it).
pub fn best(helper: &[String], container_mode: bool) -> Mechanism {
    let d = detect(helper);
    if container_mode && d.iter().any(|a| a.mechanism == Mechanism::Container && a.available) {
        return Mechanism::Container;
    }
    d.into_iter()
        .find(|a| a.available && a.mechanism != Mechanism::Container)
        .map(|a| a.mechanism)
        .unwrap_or(Mechanism::None)
}

/// Windows: puts a spawned child in a job object that dies with the daemon.
#[cfg(windows)]
pub fn contain_child(child: &tokio::process::Child) -> Option<win32job::Job> {
    let job = win32job::Job::create().ok()?;
    let mut info = job.query_extended_limit_info().ok()?;
    info.limit_kill_on_job_close();
    job.set_extended_limit_info(&info).ok()?;
    let h = child.raw_handle()?;
    job.assign_process(h as isize).ok()?;
    Some(job)
}

#[cfg(test)]
mod seatbelt_tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    #[test]
    fn seatbelt_profile_allows_only_the_policys_folders() {
        let t = tempfile::tempdir().unwrap();
        let worktree = t.path().join("worktree");
        let tmp = t.path().join("tmp");
        std::fs::create_dir_all(&worktree).unwrap();
        std::fs::create_dir_all(&tmp).unwrap();
        let policy = Policy {
            worktree: worktree.clone(),
            writable: vec![],
            tmp: Some(tmp.clone()),
            hidden: vec![],
            read_only: vec![],
            git_writable: vec![],
            proxy_tcp: None,
            proxy_unix: None,
            helper: vec![],
            container_image: None,
        };
        let p = seatbelt_profile(&policy, 0);
        assert!(!p.contains("/private/var/folders\""), "shared temp must not be writable:\n{p}");
        assert!(!p.contains("/private/tmp\""), "shared temp must not be writable:\n{p}");
        for dir in [&worktree, &tmp] {
            // Compared in the profile's quoted form (backslashes doubled).
            assert!(p.contains(&format!("(subpath \"{}\")", sb_quote(dir))), "{p}");
            let real = dir.canonicalize().unwrap();
            assert!(p.contains(&format!("(subpath \"{}\")", sb_quote(&real))), "{p}");
        }
    }

    fn git_policy(t: &Path) -> (Policy, PathBuf) {
        let worktree = t.join("worktree");
        let git = worktree.join(".git");
        std::fs::create_dir_all(git.join("hooks")).unwrap();
        std::fs::write(git.join("config"), "").unwrap();
        let outside = t.join("elsewhere-config");
        std::fs::write(&outside, "").unwrap();
        let policy = Policy {
            worktree: worktree.clone(),
            writable: vec![],
            tmp: None,
            hidden: vec![],
            // `info` is missing; `elsewhere-config` is not under a writable path.
            read_only: vec![git.join("config"), git.join("hooks"), git.join("info"), outside],
            git_writable: vec![],
            proxy_tcp: None,
            proxy_unix: Some(t.join("proxy.sock")),
            helper: vec!["helper".into()],
            container_image: None,
        };
        (policy, git)
    }

    #[test]
    fn seatbelt_profile_keeps_git_settings_read_only() {
        let t = tempfile::tempdir().unwrap();
        let (policy, git) = git_policy(t.path());
        let p = seatbelt_profile(&policy, 0);
        let allow = p.find(&format!("(allow file-write* (subpath \"{}\"))", sb_quote(&policy.worktree))).unwrap();
        for rule in [
            format!("(deny file-write* (literal \"{}\"))", sb_quote(&git.join("config"))),
            format!("(deny file-write* (subpath \"{}\"))", sb_quote(&git.join("hooks"))),
            // `.git` itself, so it cannot be renamed and replaced.
            format!("(deny file-write* (literal \"{}\"))", sb_quote(&git)),
        ] {
            let at = p.find(&rule).unwrap_or_else(|| panic!("no {rule} in\n{p}"));
            assert!(at > allow, "a deny must come after the allow it narrows:\n{p}");
        }
        assert!(!p.contains("info\""), "{p}");
        assert!(!p.contains("elsewhere-config"), "{p}");
        assert!(!p.contains(&format!("(deny file-write* (literal \"{}\"))", sb_quote(&policy.worktree))), "{p}");
    }

    #[test]
    fn bubblewrap_keeps_git_settings_read_only() {
        let t = tempfile::tempdir().unwrap();
        let (policy, git) = git_policy(t.path());
        let w = wrap_inner(Mechanism::Bubblewrap, &policy, "true", &[]).unwrap();
        let s = |p: &Path| p.to_string_lossy().into_owned();
        let pos = |a: &[&str]| {
            w.args.windows(a.len()).position(|x| x.iter().zip(a).all(|(x, a)| x == a)).unwrap_or_else(|| {
                panic!("no {a:?} in {:?}", w.args);
            })
        };
        let wt = pos(&["--bind", &s(&policy.worktree), &s(&policy.worktree)]);
        let pin = pos(&["--bind", &s(&git), &s(&git)]);
        let config = pos(&["--ro-bind", &s(&git.join("config")), &s(&git.join("config"))]);
        let hooks = pos(&["--ro-bind", &s(&git.join("hooks")), &s(&git.join("hooks"))]);
        assert!(wt < pin && pin < config && pin < hooks, "{:?}", w.args);
        assert!(!w.args.iter().any(|a| a.ends_with("info") || a.ends_with("elsewhere-config")), "{:?}", w.args);
    }

    /// A task agent: the shared git directory is outside its worktree, so
    /// read-only already; its commit folders are opened, and the files in
    /// its own worktree folder closed again after them.
    fn task_policy(t: &Path) -> (Policy, PathBuf, PathBuf) {
        let common = t.join("repo").join(".git");
        let own = common.join("worktrees").join("t1");
        std::fs::create_dir_all(&own).unwrap();
        for d in ["objects", "refs", "logs"] {
            std::fs::create_dir_all(common.join(d)).unwrap();
        }
        for f in ["commondir", "gitdir", "config.worktree"] {
            std::fs::write(own.join(f), "").unwrap();
        }
        std::fs::write(common.join("config"), "").unwrap();
        let worktree = t.join("repo").join(".polyph").join("t1");
        std::fs::create_dir_all(&worktree).unwrap();
        std::fs::write(worktree.join(".git"), "").unwrap();
        let mut read_only = vec![common.clone(), worktree.join(".git")];
        read_only.extend(["commondir", "gitdir", "config.worktree"].iter().map(|f| own.join(f)));
        let mut git_writable: Vec<PathBuf> = ["objects", "refs", "logs"].iter().map(|d| common.join(d)).collect();
        git_writable.push(own.clone());
        let policy = Policy {
            worktree,
            writable: vec![],
            tmp: None,
            hidden: vec![],
            read_only,
            git_writable,
            proxy_tcp: None,
            proxy_unix: Some(t.join("proxy.sock")),
            helper: vec!["helper".into()],
            container_image: None,
        };
        (policy, common, own)
    }

    #[test]
    fn task_agents_write_only_their_commit_folders() {
        let t = tempfile::tempdir().unwrap();
        let (policy, common, own) = task_policy(t.path());
        let s = |p: &Path| p.to_string_lossy().into_owned();

        let w = wrap_inner(Mechanism::Bubblewrap, &policy, "true", &[]).unwrap();
        let pos = |a: &[&str]| {
            w.args.windows(a.len()).position(|x| x.iter().zip(a).all(|(x, a)| x == a)).unwrap_or_else(|| {
                panic!("no {a:?} in {:?}", w.args);
            })
        };
        let own_rw = pos(&["--bind", &s(&own), &s(&own)]);
        let objects = pos(&["--bind", &s(&common.join("objects")), &s(&common.join("objects"))]);
        let commondir = pos(&["--ro-bind", &s(&own.join("commondir")), &s(&own.join("commondir"))]);
        let gitdir = pos(&["--ro-bind", &s(&own.join("gitdir")), &s(&own.join("gitdir"))]);
        let link = pos(&["--ro-bind", &s(&policy.worktree.join(".git")), &s(&policy.worktree.join(".git"))]);
        assert!(own_rw < commondir && own_rw < gitdir && objects < commondir && link < own_rw, "{:?}", w.args);
        // The shared directory is outside the worktree: read-only already, and
        // never bound over the commit folders.
        assert!(!w.args.iter().any(|a| *a == s(&common)), "{:?}", w.args);

        let p = seatbelt_profile(&policy, 0);
        let at = |rule: String| p.find(&rule).unwrap_or_else(|| panic!("no {rule} in\n{p}"));
        let allow = at(format!("(allow file-write* (subpath \"{}\"))", sb_quote(&own)));
        let pin = at(format!("(deny file-write* (literal \"{}\"))", sb_quote(&own)));
        let deny = at(format!("(deny file-write* (literal \"{}\"))", sb_quote(&own.join("commondir"))));
        at(format!("(allow file-write* (subpath \"{}\"))", sb_quote(&common.join("refs"))));
        assert!(allow < pin && pin < deny, "{p}");
        assert!(!p.contains(&format!("(allow file-write* (subpath \"{}\"))", sb_quote(&common))), "{p}");

        // Mechanisms that cannot narrow a writable folder never open them.
        let l = wrap_inner(Mechanism::Landlock, &Policy { proxy_tcp: Some(([127, 0, 0, 1], 1).into()), ..policy }, "true", &[]).unwrap();
        assert!(!l.args.iter().any(|a| a.contains(".git")), "{:?}", l.args);
    }
}
