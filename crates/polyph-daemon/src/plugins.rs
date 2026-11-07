//! Plugins on this device (`polyph-plugins`): their MCP tool servers join
//! every agent's session, and their hooks run on room events. Both run in
//! the agent sandbox with only the network hosts the manifest declares.
//!
//! - Tool servers: the agent starts them inside its own sandbox, from a copy
//!   of the plugin in the agent's private temporary directory (the Polyph
//!   home, where plugins are installed, is hidden from agents). Their hosts
//!   join that agent's network allowlist.
//! - Hooks: the daemon runs each in a fresh sandbox (a scratch copy of the
//!   plugin as its working directory, a proxy that allows only the
//!   manifest's hosts), with the event as JSON on stdin and a time limit.
//!   A hook prints `{"chat": "…"}` lines to post to the room.

use std::path::Path;
use std::process::Stdio;
use std::sync::{Arc, Weak};
use std::time::Duration;

use polyph_agents::wire::{EnvVariable, McpServerStdio};
use polyph_plugins::manifest::{Hook, Manifest};
use polyph_plugins::store::{Installed, Store};
use polyph_sandbox::Policy;
use polyph_sandbox::proxy::{Allowlist, Decider};
use serde_json::json;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::broadcast;
use tracing::{debug, warn};

use crate::daemon::{Daemon, Shared};
use crate::events::ApiEvent;

/// Label chat posts by plugins carry (and hooks never react to).
pub const LABEL_PREFIX: &str = "plugin/";
/// Largest hook output read.
const MAX_OUTPUT: u64 = 64 * 1024;

pub fn store(shared: &Shared) -> Store {
    Store::new(shared.paths.home.join("plugins"))
}

/// Network hosts all installed plugins need (added to an agent's allowlist).
pub fn hosts(shared: &Shared) -> Vec<String> {
    store(shared).list().into_iter().flat_map(|p| p.manifest.permissions.hosts).collect()
}

/// Makes paths in a plugin command point into `dir`: an argument naming a
/// file in the plugin folder becomes its absolute path.
fn resolve(dir: &Path, command: &str, args: &[String]) -> (String, Vec<String>) {
    let abs = |s: &str| {
        let p = dir.join(s.trim_start_matches("./"));
        if !Path::new(s).is_absolute() && p.exists() { p.to_string_lossy().into_owned() } else { s.to_string() }
    };
    (abs(command), args.iter().map(|a| abs(a)).collect())
}

/// MCP servers for an agent's session: one per plugin with `[mcp]`, run
/// from a copy in `tmp` (inside the agent's sandbox).
pub fn mcp_servers(shared: &Shared, tmp: &Path) -> Vec<McpServerStdio> {
    let mut out = vec![];
    for p in store(shared).list() {
        let Some(m) = &p.manifest.mcp else { continue };
        let copy = tmp.join("plugins").join(&p.manifest.name);
        let mut budget = polyph_plugins::store::MAX_PLUGIN_BYTES;
        if let Err(e) = polyph_plugins::store::copy_dir(&p.dir, &copy, &mut budget) {
            warn!(plugin = %p.manifest.name, error = %e, "could not prepare plugin for an agent");
            continue;
        }
        let (command, args) = resolve(&copy, &m.command, &m.args);
        let mut env: Vec<EnvVariable> =
            m.env.iter().map(|(k, v)| EnvVariable { name: k.clone(), value: v.clone() }).collect();
        env.push(EnvVariable { name: "POLYPH_PLUGIN_DIR".into(), value: copy.to_string_lossy().into_owned() });
        out.push(McpServerStdio { name: format!("plugin-{}", p.manifest.name), command, args, env });
    }
    out
}

/// What one hook run produced.
#[derive(Debug)]
pub struct HookRun {
    pub status: Option<i32>,
    pub stdout: String,
    pub timed_out: bool,
}

/// Runs one hook in a sandbox with the event on stdin.
pub async fn run_hook(
    shared: &Shared,
    plugin: &Installed,
    hook: &Hook,
    event: &serde_json::Value,
) -> anyhow::Result<HookRun> {
    let scratch = std::env::temp_dir().join(format!(
        "polyph-hook-{}-{}",
        plugin.manifest.name,
        polyph_proto::ids::ObjectId::random()?.short()
    ));
    let work = scratch.join("plugin");
    let mut budget = polyph_plugins::store::MAX_PLUGIN_BYTES;
    polyph_plugins::store::copy_dir(&plugin.dir, &work, &mut budget)?;
    std::fs::create_dir_all(scratch.join("tmp"))?;
    let result = run_in(shared, plugin, hook, event, &scratch, &work).await;
    let _ = std::fs::remove_dir_all(&scratch);
    result
}

async fn run_in(
    shared: &Shared,
    plugin: &Installed,
    hook: &Hook,
    event: &serde_json::Value,
    scratch: &Path,
    work: &Path,
) -> anyhow::Result<HookRun> {
    // Only the manifest's hosts; anything else is refused without asking.
    let allow = Allowlist { hosts: plugin.manifest.permissions.hosts.clone() };
    let deny: Decider = Arc::new(|_, _| Box::pin(async { false }));
    let proxy = polyph_sandbox::proxy::start_with(allow, deny, None, Some(scratch.join("run"))).await?;
    let mech = crate::agents::mechanism(shared, &Default::default()).map_err(|e| anyhow::anyhow!("{e}"))?;
    let policy = Policy {
        worktree: work.to_path_buf(),
        writable: vec![],
        tmp: Some(scratch.join("tmp")),
        hidden: Policy::default_hidden(&shared.paths.home),
        read_only: vec![],
        proxy_tcp: Some(proxy.tcp),
        proxy_unix: proxy.unix.clone(),
        helper: shared.opts.sandbox_helper.clone(),
        container_image: None,
    };
    let (command, args) = resolve(work, &hook.command, &hook.args);
    let w = polyph_sandbox::wrap(mech, &policy, &command, &args)?;
    let mut cmd = tokio::process::Command::new(&w.program);
    cmd.args(&w.args)
        .current_dir(&w.cwd)
        .env_clear()
        .env("PATH", std::env::var_os("PATH").unwrap_or_default())
        .env("POLYPH_PLUGIN_DIR", work)
        .envs(w.env.iter().map(|(k, v)| (k.as_str(), v.as_str())))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true);
    let mut child = cmd.spawn()?;
    if let Some(mut stdin) = child.stdin.take() {
        let body = serde_json::to_vec(event)?;
        tokio::spawn(async move {
            let _ = stdin.write_all(&body).await;
        });
    }
    let mut out = String::new();
    let stdout = child.stdout.take();
    let waited = tokio::time::timeout(Duration::from_secs(hook.timeout()), async {
        if let Some(s) = stdout {
            let _ = s.take(MAX_OUTPUT).read_to_string(&mut out).await;
        }
        child.wait().await
    })
    .await;
    drop(proxy);
    match waited {
        Ok(status) => Ok(HookRun { status: status?.code(), stdout: out, timed_out: false }),
        Err(_) => {
            let _ = child.kill().await;
            Ok(HookRun { status: None, stdout: out, timed_out: true })
        }
    }
}

/// Runs plugin hooks on this daemon's room events until it stops.
pub async fn run(d: Weak<Daemon>, mut rx: broadcast::Receiver<ApiEvent>) {
    loop {
        let ev = match rx.recv().await {
            Ok(e) => e,
            Err(broadcast::error::RecvError::Lagged(n)) => {
                warn!(n, "plugin hook events lagged");
                continue;
            }
            Err(_) => return,
        };
        let kind = ev.kind().to_string();
        if !polyph_plugins::manifest::EVENTS.contains(&kind.as_str()) {
            continue;
        }
        // Never react to a plugin's own posts (no loops between plugins).
        if kind == "chat" && ev.event["agent"].as_str().is_some_and(|a| a.starts_with(LABEL_PREFIX)) {
            continue;
        }
        let Some(room) = ev.room.clone() else { continue };
        let Some(d) = d.upgrade() else { return };
        for p in store(&d.shared).list() {
            for hook in p.manifest.hooks_for(&kind).cloned().collect::<Vec<_>>() {
                let (d, p, room, ev) = (d.clone(), p.clone(), room.clone(), ev.clone());
                tokio::spawn(async move { fire(d, p, hook, room, ev).await });
            }
        }
    }
}

async fn fire(d: Arc<Daemon>, p: Installed, hook: Hook, room: String, ev: ApiEvent) {
    let payload = json!({ "room": room, "event": ev.event, "plugin": p.manifest.name });
    let run = match run_hook(&d.shared, &p, &hook, &payload).await {
        Ok(r) => r,
        Err(e) => {
            warn!(plugin = %p.manifest.name, error = %e, "plugin hook failed to start");
            return;
        }
    };
    if run.timed_out {
        warn!(plugin = %p.manifest.name, "plugin hook timed out");
    }
    let actions = polyph_plugins::manifest::actions(&run.stdout);
    if actions.is_empty() {
        debug!(plugin = %p.manifest.name, status = ?run.status, "plugin hook ran");
        return;
    }
    let Ok(rt) = d.find(&room).await else { return };
    let label = format!("{LABEL_PREFIX}{}", p.manifest.name);
    for a in actions.into_iter().take(5) {
        let Some(text) = a.chat else { continue };
        let text: String = text.chars().take(4000).collect();
        let mut c = rt.ctx.lock().await;
        let now = d.shared.clock.now();
        match c.core.send_chat(&text, None, Some(label.clone()), now) {
            Ok(fx) => crate::runtime::apply(&rt, &mut c, &d.shared, fx).await,
            Err(e) => warn!(plugin = %p.manifest.name, error = %e, "plugin chat refused"),
        }
    }
}

/// Plugins as the API lists them.
pub fn list(shared: &Shared) -> serde_json::Value {
    json!(
        store(shared)
            .list()
            .into_iter()
            .map(|p| {
                json!({
                    "name": p.manifest.name,
                    "version": p.manifest.version,
                    "description": p.manifest.description,
                    "tools": p.manifest.mcp.is_some(),
                    "hooks": p.manifest.hooks.iter().flat_map(|h| h.on.clone()).collect::<Vec<_>>(),
                    "hosts": p.manifest.permissions.hosts,
                })
            })
            .collect::<Vec<_>>()
    )
}

/// Validates without installing (for `plugin add` to show what it asks for).
pub fn inspect(dir: &Path) -> Result<Manifest, polyph_plugins::Error> {
    Manifest::load(dir)
}
