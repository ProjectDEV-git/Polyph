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
    /// (its task's folders in `refs/heads/polyph/` and
    /// `logs/refs/heads/polyph/`, and its own `worktrees/<name>/`, which
    /// holds its own object store; the shared `objects/` stays read-only),
    /// each mounted writable over `read_only`, with the `read_only` paths
    /// inside them (its store's `info/`, other branches) read-only again.
    /// Bubblewrap and Seatbelt only; elsewhere the agent cannot commit.
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
        self.git_writable.iter().filter(|g| std::fs::symlink_metadata(g).is_ok_and(|m| m.is_dir())).cloned().collect()
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

/// Variables that point at the person's IPC endpoints: session and system
/// bus, ssh-agent, gpg-agent, the keyring, Kerberos tickets, display and
/// sound servers, window managers, terminals and editors that take commands
/// (kitty, WezTerm, Neovim, VS Code), container engines, systemd
/// notification and WSL interop. Sandboxed agents start
/// without them (SEC-14); the sockets themselves are out of reach as
/// `docs/sandbox.md` describes per mechanism.
pub const IPC_ENV: &[&str] = &[
    "DBUS_SESSION_BUS_ADDRESS",
    "DBUS_SYSTEM_BUS_ADDRESS",
    "XDG_RUNTIME_DIR",
    "SSH_AUTH_SOCK",
    "GPG_AGENT_INFO",
    "DISPLAY",
    "WAYLAND_DISPLAY",
    "SWAYSOCK",
    "I3SOCK",
    "TMUX",
    "DOCKER_HOST",
    "CONTAINER_HOST",
    "NOTIFY_SOCKET",
    "WSL_INTEROP",
    "DBUS_STARTER_ADDRESS",
    "XAUTHORITY",
    "PULSE_SERVER",
    "PIPEWIRE_REMOTE",
    "KRB5CCNAME",
    "GNOME_KEYRING_CONTROL",
    "VSCODE_IPC_HOOK_CLI",
    "KITTY_LISTEN_ON",
    "WEZTERM_UNIX_SOCKET",
    "NVIM",
];

/// Container engine sockets and other host IPC outside `/run`, `/var/run`
/// and `/tmp` (which bubblewrap replaces whole): the usual Docker, Podman,
/// Colima, Rancher Desktop, Lima and OrbStack locations, WSLg's runtime
/// folder, and the Unix sockets `$DOCKER_HOST` and `$CONTAINER_HOST` name.
pub fn host_ipc_paths() -> Vec<PathBuf> {
    host_ipc_paths_in(home_dir().as_deref(), &|k| std::env::var(k).ok())
}

fn host_ipc_paths_in(home: Option<&Path>, env: &dyn Fn(&str) -> Option<String>) -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = [
        "/var/run/docker.sock",
        "/run/docker.sock",
        "/run/podman/podman.sock",
        "/var/run/podman/podman.sock",
        "/run/containerd/containerd.sock",
        "/mnt/wslg",
    ]
    .iter()
    .map(PathBuf::from)
    .collect();
    if let Some(h) = home {
        for d in [".colima", ".rd", ".lima", ".orbstack", ".local/share/containers/podman/machine"] {
            v.push(h.join(d));
        }
    }
    for k in ["DOCKER_HOST", "CONTAINER_HOST"] {
        if let Some(p) = env(k).as_deref().and_then(|u| u.strip_prefix("unix://")).filter(|p| p.starts_with('/')) {
            v.push(PathBuf::from(p));
        }
    }
    v.dedup();
    v
}

/// Folders bubblewrap replaces with an empty tmpfs: everything there is
/// sockets and state of running services (`/run/user/<uid>` holds the
/// session bus, `systemd --user`, keyring and gpg agents).
const RUN_DIRS: [&str; 2] = ["/run", "/var/run"];

/// Links directly in `/run` to folders outside it (NixOS keeps its system
/// profile at `/run/current-system`), recreated inside the empty `/run`.
fn run_links() -> Vec<(PathBuf, PathBuf)> {
    let Ok(rd) = std::fs::read_dir("/run") else { return vec![] };
    rd.flatten()
        .filter(|e| e.file_type().is_ok_and(|t| t.is_symlink()))
        .filter_map(|e| {
            let target = std::fs::read_link(e.path()).ok()?;
            let real = e.path().canonicalize().ok()?;
            (real.is_dir() && !RUN_DIRS.iter().any(|r| real.starts_with(r))).then(|| (target, e.path()))
        })
        .collect()
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
                "--unshare-ipc".into(),
                "--unshare-uts".into(),
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
            // Host IPC (SEC-14): `/run` and `/var/run` empty (a read-only
            // mount does not stop `connect` on a socket), container sockets
            // elsewhere hidden. The network namespace already separates
            // abstract sockets. Writable paths and the proxy socket are
            // bound back below, wherever they are.
            for r in RUN_DIRS {
                if std::fs::symlink_metadata(r).is_ok_and(|m| m.is_dir()) {
                    a.extend(["--tmpfs".into(), r.into()]);
                }
            }
            for (target, link) in run_links() {
                a.extend([
                    "--symlink".into(),
                    target.to_string_lossy().into_owned(),
                    link.to_string_lossy().into_owned(),
                ]);
            }
            let writable = policy.writable_all();
            let ipc = host_ipc_paths()
                .into_iter()
                .filter(|p| !RUN_DIRS.iter().chain(&["/tmp"]).any(|r| p.starts_with(r)) && !policy.hidden.contains(p));
            for h in policy.hidden.iter().cloned().chain(ipc).collect::<Vec<_>>().iter() {
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
            // Without the variables naming host IPC (SEC-14).
            let mut a = vec!["-p".to_string(), profile, "/usr/bin/env".into()];
            for k in IPC_ENV {
                a.extend(["-u".to_string(), k.to_string()]);
            }
            a.push(program.to_string());
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

/// A Seatbelt rule for writes and the same for hard links: `verb` is
/// `allow` or `deny`, `filter` `subpath` or `literal`.
fn sb_rule(verb: &str, filter: &str, p: &Path) -> String {
    let q = sb_quote(p);
    format!("({verb} file-write* ({filter} \"{q}\"))\n({verb} file-link ({filter} \"{q}\"))\n")
}

/// Mach services agents may look up under Seatbelt: user and group lookups,
/// logging, notifications, preferences (read; writes are denied), TLS trust
/// evaluation, network settings and file events. Not on it, among others:
/// LaunchServices (`open`), Apple Events (`osascript`), the pasteboard and
/// the window server (SEC-14).
pub const SEATBELT_MACH_SERVICES: &[&str] = &[
    "com.apple.system.opendirectoryd.libinfo",
    "com.apple.system.opendirectoryd.membership",
    "com.apple.system.DirectoryService.libinfo_v1",
    "com.apple.system.DirectoryService.membership_v1",
    "com.apple.system.logger",
    "com.apple.logd",
    "com.apple.system.notification_center",
    "com.apple.bsd.dirhelper",
    "com.apple.cfprefsd.daemon",
    "com.apple.cfprefsd.agent",
    "com.apple.SecurityServer",
    "com.apple.trustd",
    "com.apple.trustd.agent",
    "com.apple.ocspd",
    "com.apple.SystemConfiguration.configd",
    "com.apple.SystemConfiguration.DNSConfiguration",
    "com.apple.networkd",
    "com.apple.FSEvents",
    "com.apple.PowerManagement.control",
];

/// Programs that start other programs outside the profile, through
/// LaunchServices or Apple Events. Copies of them are stopped by the Mach
/// lookup list instead.
pub const SEATBELT_DENIED_PROGRAMS: &[&str] = &["/usr/bin/open", "/usr/bin/osascript", "/usr/bin/osacompile"];

/// `p` as a Seatbelt regular expression matching what is below it.
fn sb_regex_under(p: &Path) -> String {
    let mut r = String::from("^");
    for c in p.to_string_lossy().chars() {
        if "\\.^$|?*+()[]{}\"".contains(c) {
            r.push('\\');
        }
        r.push(c);
    }
    r.push('/');
    r
}

/// Seatbelt profile: nothing is allowed unless listed (SEC-14). Programs
/// run and fork, except `open`, `osascript` and `osacompile`; everything is
/// readable except hidden paths; writes only to the worktree, writable
/// paths and temp; Mach lookups only to [`SEATBELT_MACH_SERVICES`];
/// network only to the proxy, Unix sockets only inside the agent's own temp
/// folder; signals only within the sandbox; no ioctls on terminals.
///
/// Hard links too (`file-link`): the agent could otherwise link a file of
/// the person's (`~/.profile`) into a folder it writes, where git run by
/// the daemon as the person could append to it. A link is allowed only
/// from a file in a writable folder, and never from a read-only one inside
/// it. Bubblewrap needs no rule: every writable folder is its own mount,
/// and Linux refuses links across mounts.
pub fn seatbelt_profile(policy: &Policy, proxy_port: u16) -> String {
    let mut s = String::from("(version 1)\n(deny default)\n(deny file-write*)\n(deny file-link)\n");
    s.push_str("(allow process-fork)\n(allow process-exec*)\n(allow process-info*)\n");
    s.push_str("(allow signal (target same-sandbox))\n");
    s.push_str("(allow file-read*)\n(allow file-map-executable)\n(allow sysctl-read)\n(allow user-preference-read)\n");
    // ioctls, but none on terminals (TIOCSTI types into the person's shell).
    s.push_str("(allow file-ioctl)\n");
    s.push_str("(deny file-ioctl (literal \"/dev/tty\") (literal \"/dev/console\") (regex #\"^/dev/ttys[0-9]+$\"))\n");
    s.push_str("(allow ipc-posix-sem)\n(allow ipc-posix-shm*)\n(allow system-socket)\n");
    s.push_str("(allow mach-lookup");
    // Exact names, as anchored expressions.
    for m in SEATBELT_MACH_SERVICES {
        s.push_str(&format!(" (global-name-regex #\"^{}$\")", m.replace('.', "\\.")));
    }
    s.push_str(")\n(deny process-exec*");
    for p in SEATBELT_DENIED_PROGRAMS {
        s.push_str(&format!(" (literal \"{p}\")"));
    }
    s.push_str(")\n");
    // Device files only; shared temp folders (/private/tmp, /private/var/folders)
    // stay read-only: the agent gets its own TMPDIR through the policy.
    s.push_str("(allow file-write* (literal \"/dev/null\") (literal \"/dev/tty\") (regex #\"^/dev/fd/\"))\n");
    for w in policy.writable_all() {
        s.push_str(&sb_rule("allow", "subpath", &w));
        // Seatbelt matches resolved paths (/var is /private/var on macOS).
        if let Ok(real) = w.canonicalize()
            && real != w
        {
            s.push_str(&sb_rule("allow", "subpath", &real));
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
                    push(sb_rule("allow", "subpath", &p));
                }
                for p in both(g) {
                    push(sb_rule("deny", "literal", &p));
                }
            }
        }
        for (r, dir, _) in ro.iter().filter(|(_, _, l)| *l == late) {
            let filter = if r.is_dir() { "subpath" } else { "literal" };
            let dir = (!writable.contains(dir) && !git.contains(dir)).then_some(("literal", dir.clone()));
            for (f, p) in std::iter::once((filter, r.clone())).chain(dir) {
                for p in both(&p) {
                    push(sb_rule("deny", f, &p));
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
    // No Unix sockets outside the agent's own temp folder: the session's
    // services, ssh-agent, docker.sock and the like stay out of reach.
    s.push_str("(deny network-outbound)\n(deny network-outbound (remote unix-socket))\n");
    s.push_str("(allow network-bind (local ip \"localhost:*\"))\n(allow network-inbound (local ip \"localhost:*\"))\n");
    if let Some(t) = &policy.tmp {
        for p in std::iter::once(t.clone()).chain(t.canonicalize().ok().filter(|r| r != t)) {
            let r = sb_regex_under(&p);
            s.push_str(&format!("(allow network-bind (local unix-socket (path-regex #\"{r}\")))\n"));
            s.push_str(&format!("(allow network-outbound (remote unix-socket (path-regex #\"{r}\")))\n"));
        }
    }
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
        let probe = |what: &str| {
            helper.first().is_some_and(|h| {
                std::process::Command::new(h)
                    .args(&helper[1..])
                    .args(["probe", what])
                    .stderr(std::process::Stdio::null())
                    .status()
                    .is_ok_and(|s| s.success())
            })
        };
        let ll = probe("landlock");
        v.push(Availability {
            mechanism: Mechanism::Landlock,
            available: ll,
            detail: match (ll, ll && probe("seccomp")) {
                (true, true) => "Landlock is enforced by this kernel; seccomp blocks Unix sockets".into(),
                (true, false) => "Landlock is enforced by this kernel, but there is no seccomp filter: agents can \
                                  reach host IPC (the session bus, ssh-agent, docker.sock) over Unix sockets"
                    .into(),
                _ => "Landlock is not available in this kernel".into(),
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

    /// SEC-14: Seatbelt starts from nothing; no Unix socket but in the
    /// agent's own temp folder; no LaunchServices, Apple Events, pasteboard
    /// or window server; `open` and `osascript` do not run.
    #[test]
    fn seatbelt_profile_denies_host_ipc() {
        let t = tempfile::tempdir().unwrap();
        let (worktree, tmp) = (t.path().join("w"), t.path().join("t.x"));
        std::fs::create_dir_all(&worktree).unwrap();
        std::fs::create_dir_all(&tmp).unwrap();
        let policy = Policy {
            worktree,
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
        let p = seatbelt_profile(&policy, 4000);
        assert!(p.starts_with("(version 1)\n(deny default)\n"), "{p}");
        assert!(!p.contains("(allow default)"), "{p}");
        assert!(!p.contains("(allow network-outbound (remote unix-socket))"), "{p}");
        assert!(!p.contains("(allow mach-lookup)\n") && !p.contains("(allow network*") && !p.contains("lsopen"), "{p}");
        for svc in ["launchservicesd", "lsd", "coreservicesd", "appleevents", "pasteboard", "windowserver"] {
            assert!(!p.to_lowercase().contains(svc), "{svc} in\n{p}");
        }
        let deny_unix = p.find("(deny network-outbound (remote unix-socket))").unwrap();
        let own = sb_regex_under(&tmp);
        assert!(own.ends_with("t\\.x/"), "{own}");
        let allow_own =
            p.find(&format!("(allow network-outbound (remote unix-socket (path-regex #\"{own}\")))")).unwrap();
        assert!(deny_unix < allow_own, "{p}");
        assert!(p.contains("(allow network-outbound (remote ip \"localhost:4000\"))"), "{p}");
        let exec = p.find("(allow process-exec*)").unwrap();
        for prog in SEATBELT_DENIED_PROGRAMS {
            assert!(p.find(&format!("(literal \"{prog}\")")).unwrap() > exec, "{p}");
        }
        // The command runs without the variables naming host IPC.
        let w = wrap_inner(Mechanism::Seatbelt, &policy, "sh", &[]).unwrap();
        let env = w.args.iter().position(|a| a == "/usr/bin/env").unwrap();
        assert_eq!(w.args.last().map(String::as_str), Some("sh"));
        for k in IPC_ENV {
            assert!(w.args[env..].windows(2).any(|x| x[0] == "-u" && x[1] == *k), "{k}: {:?}", w.args);
        }
    }

    /// SEC-14: bubblewrap empties `/run` and `/var/run` before binding the
    /// agent's paths back, and gets its own IPC and UTS namespaces.
    #[cfg(target_os = "linux")]
    #[test]
    fn bubblewrap_empties_run_before_the_agents_paths() {
        let t = tempfile::tempdir().unwrap();
        let (policy, _) = git_policy(t.path());
        let w = wrap_inner(Mechanism::Bubblewrap, &policy, "true", &[]).unwrap();
        for flag in ["--unshare-ipc", "--unshare-uts", "--unshare-net"] {
            assert!(w.args.iter().any(|a| a == flag), "{flag}: {:?}", w.args);
        }
        let run = w.args.windows(2).position(|x| x[0] == "--tmpfs" && x[1] == "/run").unwrap();
        let wt = policy.worktree.to_string_lossy().into_owned();
        let bind = w.args.windows(2).position(|x| x[0] == "--bind" && x[1] == wt).unwrap();
        assert!(run < bind, "{:?}", w.args);
    }

    #[test]
    fn host_ipc_paths_cover_container_sockets() {
        let env = |k: &str| match k {
            "DOCKER_HOST" => Some("unix:///srv/d/docker.sock".to_string()),
            "CONTAINER_HOST" => Some("ssh://host/run/podman.sock".to_string()),
            _ => None,
        };
        let v = host_ipc_paths_in(Some(Path::new("/home/ann")), &env);
        for p in ["/var/run/docker.sock", "/home/ann/.colima", "/home/ann/.orbstack", "/srv/d/docker.sock"] {
            assert!(v.contains(&PathBuf::from(p)), "{p}: {v:?}");
        }
        assert!(!v.iter().any(|p| p.to_string_lossy().contains("podman.sock") && p.starts_with("/host")), "{v:?}");
    }

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
        // Hard links: denied first, then only from the worktree, and never
        // from the read-only paths in it.
        let none = p.find("(deny file-link)\n").unwrap_or_else(|| panic!("{p}"));
        let link_allow = p.find(&format!("(allow file-link (subpath \"{}\"))", sb_quote(&policy.worktree))).unwrap();
        let link_deny = p.find(&format!("(deny file-link (literal \"{}\"))", sb_quote(&git.join("config")))).unwrap();
        assert!(none < link_allow && link_allow < link_deny, "{p}");
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
    /// its own worktree folder closed again after them. The shape
    /// `polyph_git::sandbox_git` gives for branch `polyph/t1/a` (its task
    /// folders in `refs/` and `logs/`, its own folder, then its own object
    /// store on its own).
    fn task_policy(t: &Path) -> (Policy, PathBuf, PathBuf) {
        let common = t.join("repo").join(".git");
        let own = common.join("worktrees").join("t1");
        for d in ["objects/info", "refs/heads/polyph/t1", "logs/refs/heads/polyph/t1", "refs/heads/polyph/t2"] {
            std::fs::create_dir_all(common.join(d)).unwrap();
        }
        std::fs::create_dir_all(own.join("objects").join("info")).unwrap();
        for f in [
            "refs/heads/main",
            "refs/heads/polyph/t1/a",
            "refs/heads/polyph/t1/other",
            "logs/refs/heads/polyph/t1/other",
            "refs/heads/polyph/t2/b",
        ] {
            std::fs::write(common.join(f), "").unwrap();
        }
        for f in ["commondir", "gitdir", "config.worktree"] {
            std::fs::write(own.join(f), "").unwrap();
        }
        std::fs::write(common.join("config"), "").unwrap();
        let worktree = t.join("repo").join(".polyph").join("t1");
        std::fs::create_dir_all(&worktree).unwrap();
        std::fs::write(worktree.join(".git"), "").unwrap();
        let (refs, logs) = (common.join("refs/heads/polyph/t1"), common.join("logs/refs/heads/polyph/t1"));
        let mut read_only = vec![common.clone(), worktree.join(".git")];
        read_only.extend(["commondir", "gitdir", "config.worktree"].iter().map(|f| own.join(f)));
        read_only.push(own.join("objects").join("info"));
        read_only.extend([refs.join("other"), logs.join("other")]);
        let git_writable = vec![refs, logs, own.clone(), own.join("objects")];
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
        let objects = own.join("objects");
        let objects_rw = pos(&["--bind", &s(&objects), &s(&objects)]);
        let commondir = pos(&["--ro-bind", &s(&own.join("commondir")), &s(&own.join("commondir"))]);
        let gitdir = pos(&["--ro-bind", &s(&own.join("gitdir")), &s(&own.join("gitdir"))]);
        let link = pos(&["--ro-bind", &s(&policy.worktree.join(".git")), &s(&policy.worktree.join(".git"))]);
        assert!(own_rw < commondir && own_rw < gitdir && link < own_rw, "{:?}", w.args);
        // Its own object store is a mount of its own, after its folder, so
        // it cannot be renamed; its `info/` read-only after that.
        let info = objects.join("info");
        assert!(own_rw < objects_rw && objects_rw < pos(&["--ro-bind", &s(&info), &s(&info)]), "{:?}", w.args);
        // Only the task's folders, with the other branches in them
        // read-only again after them.
        let (refs, logs) = (common.join("refs/heads/polyph/t1"), common.join("logs/refs/heads/polyph/t1"));
        let refs_rw = pos(&["--bind", &s(&refs), &s(&refs)]);
        let logs_rw = pos(&["--bind", &s(&logs), &s(&logs)]);
        for p in [refs.join("other"), logs.join("other")] {
            assert!(pos(&["--ro-bind", &s(&p), &s(&p)]) > refs_rw.max(logs_rw), "{:?}", w.args);
        }
        for p in [
            common.join("refs"),
            common.join("logs"),
            common.join("refs/heads"),
            common.join("refs/heads/polyph"),
            common.join("refs/heads/polyph/t2"),
            common.join("objects"),
            common.join("objects").join("info"),
            refs.join("a"),
        ] {
            assert!(!w.args.iter().any(|a| *a == s(&p)), "{p:?} in {:?}", w.args);
        }
        // The shared directory is outside the worktree: read-only already, and
        // never bound over the commit folders.
        assert!(!w.args.iter().any(|a| *a == s(&common)), "{:?}", w.args);

        let p = seatbelt_profile(&policy, 0);
        let at = |rule: String| p.find(&rule).unwrap_or_else(|| panic!("no {rule} in\n{p}"));
        let allow = at(format!("(allow file-write* (subpath \"{}\"))", sb_quote(&own)));
        let pin = at(format!("(deny file-write* (literal \"{}\"))", sb_quote(&own)));
        let deny = at(format!("(deny file-write* (literal \"{}\"))", sb_quote(&own.join("commondir"))));
        let objects_pin = at(format!("(deny file-write* (literal \"{}\"))", sb_quote(&objects)));
        let refs_allow = at(format!("(allow file-write* (subpath \"{}\"))", sb_quote(&refs)));
        at(format!("(allow file-write* (subpath \"{}\"))", sb_quote(&logs)));
        for d in [refs.join("other"), logs.join("other")] {
            assert!(at(format!("(deny file-write* (literal \"{}\"))", sb_quote(&d))) > refs_allow, "{p}");
        }
        assert!(at(format!("(deny file-write* (subpath \"{}\"))", sb_quote(&info))) > objects_pin, "{p}");
        for d in [
            common.join("refs"),
            common.join("logs"),
            common.join("refs/heads"),
            common.join("refs/heads/polyph"),
            common.join("objects"),
        ] {
            assert!(!p.contains(&format!("(allow file-write* (subpath \"{}\"))", sb_quote(&d))), "{p}");
        }
        assert!(allow < pin && pin < deny, "{p}");
        assert!(!p.contains(&format!("(allow file-write* (subpath \"{}\"))", sb_quote(&common))), "{p}");
        // Hard links follow the same rules: none from outside, none from a
        // read-only file inside a folder it writes.
        assert!(
            p.find("(deny file-link)").unwrap() < at(format!("(allow file-link (subpath \"{}\"))", sb_quote(&own)))
        );
        assert!(at(format!("(deny file-link (literal \"{}\"))", sb_quote(&own.join("commondir")))) > allow, "{p}");
        assert!(!p.contains(&format!("(allow file-link (subpath \"{}\"))", sb_quote(&common))), "{p}");

        // Mechanisms that cannot narrow a writable folder never open them.
        let l = wrap_inner(
            Mechanism::Landlock,
            &Policy { proxy_tcp: Some(([127, 0, 0, 1], 1).into()), ..policy },
            "true",
            &[],
        )
        .unwrap();
        assert!(!l.args.iter().any(|a| a.contains(".git")), "{:?}", l.args);
    }
}
