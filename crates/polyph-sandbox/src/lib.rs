//! Sandboxing for agents and everything they spawn.
//!
//! | Platform | Mechanism |
//! |---|---|
//! | Linux | bubblewrap with a private network namespace; Landlock when bubblewrap is missing |
//! | macOS | a Seatbelt profile run with `sandbox-exec` |
//! | Windows | a job object plus proxy settings (see `docs/sandbox.md` for what this does not cover) |
//! | Any | rootless Podman or Docker in "container mode" |
//!
//! Outbound network always goes through [`proxy`], which passes allowlisted
//! hosts and turns everything else into an approval request.

pub mod helper;
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
    v.push(("NO_PROXY".into(), "localhost,127.0.0.1,::1".into()));
    v.push(("no_proxy".into(), "localhost,127.0.0.1,::1".into()));
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
            Ok(Wrapped { program: program.into(), args: args.to_vec(), env: proxy_env(&tcp_url), cwd, mechanism: mech })
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
    s.push_str("(allow file-write* (literal \"/dev/null\") (literal \"/dev/tty\") (regex #\"^/dev/fd/\") (subpath \"/private/tmp\") (subpath \"/private/var/folders\"))\n");
    for w in policy.writable_all() {
        s.push_str(&format!("(allow file-write* (subpath \"{}\"))\n", sb_quote(&w)));
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
            detail: "job object; file and network isolation are limited".into(),
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
