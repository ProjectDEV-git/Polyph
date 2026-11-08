//! Getting a person's own agents into a room: `polyph connect`,
//! `polyph disconnect`, `polyph start`, `polyph doctor`, and `polyph mcp`
//! run by an agent Polyph did not start. Texts are in `locales/*/setup.ftl`.

use std::io::IsTerminal;
use std::path::{Path, PathBuf};

use polyph_i18n::tr;
use serde_json::{Value, json};

use crate::client::{self as daemon_client, Api};
use crate::{Ctx, call};
use polyph_daemon::clients::{self, CLIENTS, Change, Client, Env, Transport};

fn confirm(question: &str) -> anyhow::Result<bool> {
    use std::io::Write;
    eprint!("{question} ");
    std::io::stderr().flush()?;
    let mut s = String::new();
    std::io::stdin().read_line(&mut s)?;
    Ok(matches!(s.trim().to_lowercase().as_str(), "y" | "yes"))
}

/// Asks a yes/no question where pressing Enter means yes.
pub fn confirm_default_yes(question: &str) -> anyhow::Result<bool> {
    use std::io::Write;
    eprint!("{question} ");
    std::io::stderr().flush()?;
    let mut s = String::new();
    std::io::stdin().read_line(&mut s)?;
    Ok(matches!(s.trim().to_lowercase().as_str(), "" | "y" | "yes"))
}

fn ask(question: &str) -> anyhow::Result<String> {
    use std::io::Write;
    eprint!("{question} ");
    std::io::stderr().flush()?;
    let mut s = String::new();
    std::io::stdin().read_line(&mut s)?;
    Ok(s.trim().to_string())
}

fn env(ctx: &Ctx) -> anyhow::Result<Env> {
    Env::current().ok_or_else(|| anyhow::anyhow!(ctx.l.t("connect-no-home")))
}

/// `~` for the home directory in paths shown to the person.
fn show(env: &Env, p: &Path) -> String {
    match p.strip_prefix(&env.home) {
        Ok(rest) => format!("~/{}", rest.display()),
        Err(_) => p.display().to_string(),
    }
}

fn pick(ctx: &Ctx, env: &Env, names: &[String]) -> anyhow::Result<Vec<&'static Client>> {
    if names.is_empty() {
        return Ok(CLIENTS.iter().filter(|c| c.installed(env)).collect());
    }
    names
        .iter()
        .map(|n| {
            clients::find(n).ok_or_else(|| {
                let known: Vec<&str> = CLIENTS.iter().map(|c| c.id).collect();
                anyhow::anyhow!(tr!(ctx.l, "connect-unknown", name = n.as_str(), known = known.join(", ")))
            })
        })
        .collect()
}

/// The data directory to pass on when it is not the platform default.
fn home_override(ctx: &Ctx) -> Option<PathBuf> {
    let default = directories::ProjectDirs::from("dev", "polyph", "polyph").map(|d| d.config_dir().to_path_buf());
    (default.as_deref() != Some(ctx.paths.home.as_path())).then(|| ctx.paths.home.clone())
}

fn print_change(ctx: &Ctx, env: &Env, c: &Change) {
    let l = &ctx.l;
    let what = if c.after.is_none() {
        "connect-remove-from"
    } else if c.existed {
        "connect-change-in"
    } else {
        "connect-create"
    };
    ctx.out(tr!(l, what, client = c.client.name, file = show(env, &c.file)));
    for line in c.diff() {
        ctx.out(format!("    {line}"));
    }
    if c.drops_comments {
        ctx.out(format!("    {}", l.t("connect-drops-comments")));
    }
}

/// Shows the changes, asks once, writes them. Returns how many were written.
fn confirm_and_apply(ctx: &Ctx, env: &Env, changes: &[Change], yes: bool, question: &str) -> anyhow::Result<usize> {
    let l = &ctx.l;
    if !ctx.json {
        for c in changes {
            print_change(ctx, env, c);
        }
    }
    if !yes {
        if ctx.json || !std::io::stdin().is_terminal() {
            anyhow::bail!(l.t("connect-needs-yes"));
        }
        if !confirm(question)? {
            ctx.out(l.t("connect-cancelled"));
            return Ok(0);
        }
    }
    let mut done = 0;
    for c in changes {
        match clients::apply(c) {
            Ok(backup) => {
                done += 1;
                if !ctx.json {
                    let key = if c.after.is_some() { "connect-done" } else { "disconnect-done" };
                    ctx.out(tr!(l, key, client = c.client.name));
                    if let Some(b) = backup {
                        ctx.out(format!("    {}", tr!(l, "connect-backup", file = show(env, &b))));
                    }
                }
            }
            Err(e) => {
                ctx.out(tr!(l, "connect-write-failed", file = show(env, &c.file), error = e.to_string()));
            }
        }
    }
    Ok(done)
}

/// `polyph connect [clients] [--http] [--yes]`.
pub async fn connect(ctx: &Ctx, names: &[String], http: bool, yes: bool) -> anyhow::Result<usize> {
    let l = &ctx.l;
    let env = env(ctx)?;
    let targets = pick(ctx, &env, names)?;
    if targets.is_empty() {
        let known: Vec<&str> = CLIENTS.iter().map(|c| c.id).collect();
        ctx.out(tr!(l, "connect-none-found", known = known.join(", ")));
        return Ok(0);
    }
    // Unless named, skip VS Code when the Polyph extension registers itself.
    let (targets, by_extension): (Vec<&'static Client>, Vec<&'static Client>) =
        targets.into_iter().partition(|c| !names.is_empty() || !c.registered_by_extension(&env));
    for c in &by_extension {
        if !ctx.json {
            ctx.out(tr!(l, "connect-by-extension", client = c.name));
        }
    }
    let cwd = std::env::current_dir()?;
    let mut api = if http { Some(ctx.api().await?) } else { None };
    let exe = std::env::current_exe().and_then(|e| polyph_git::canonical(&e))?;
    let mut changes = vec![];
    let mut already = vec![];
    for c in targets {
        let t = match api.as_mut() {
            Some(api) => {
                let k = call(api, "connect.key", json!({ "client": c.id, "dir": cwd })).await?;
                let port = daemon_client::daemon_port(&ctx.paths).unwrap_or(0);
                Transport::Http {
                    url: format!("http://127.0.0.1:{port}/mcp"),
                    key: k["key"].as_str().unwrap_or_default().to_string(),
                }
            }
            None => Transport::stdio(&exe, home_override(ctx).as_deref()),
        };
        let change = clients::plan_add(c, &env, &t)?;
        if change.is_noop() {
            already.push(c.name);
        } else {
            changes.push(change);
        }
    }
    if ctx.json {
        let plan: Vec<Value> = changes
            .iter()
            .map(|c| json!({ "client": c.client.id, "file": c.file, "before": c.before, "after": c.after }))
            .collect();
        ctx.print_json(&json!({ "changes": plan, "already": already }));
    }
    for name in &already {
        if !ctx.json {
            ctx.out(tr!(l, "connect-already", client = *name));
        }
    }
    if changes.is_empty() {
        return Ok(0);
    }
    let n = confirm_and_apply(ctx, &env, &changes, yes, &l.t("connect-confirm"))?;
    if n > 0 && !ctx.json {
        ctx.out(l.t("connect-restart"));
    }
    Ok(n)
}

/// `polyph disconnect [clients] [--yes]`.
pub async fn disconnect(ctx: &Ctx, names: &[String], yes: bool) -> anyhow::Result<()> {
    let l = &ctx.l;
    let env = env(ctx)?;
    let targets: Vec<&'static Client> =
        if names.is_empty() { CLIENTS.iter().collect() } else { pick(ctx, &env, names)? };
    let mut changes = vec![];
    for c in &targets {
        if let Some(ch) = clients::plan_remove(c, &env)? {
            changes.push(ch);
        }
    }
    if changes.is_empty() {
        ctx.out(l.t("disconnect-nothing"));
        return Ok(());
    }
    let n = confirm_and_apply(ctx, &env, &changes, yes, &l.t("disconnect-confirm"))?;
    if n > 0 {
        // Keys for the web address stop working too (the daemon may be stopped).
        if let Ok(mut api) = Api::connect(&ctx.paths, false).await {
            for c in &changes {
                let _ = call(&mut api, "connect.revoke", json!({ "client": c.client.id })).await;
            }
        }
    }
    Ok(())
}

/// `polyph mcp`: inside a Polyph sandbox, the bridge to the daemon through
/// the agent's proxy; otherwise the bridge for an agent the person runs.
pub async fn mcp(ctx: &Ctx, client: Option<String>, room: Option<String>) -> anyhow::Result<()> {
    if polyph_mcp::inside_polyph() {
        return polyph_mcp::run_stdio().await;
    }
    // stdout is the MCP channel: nothing else may be printed there.
    daemon_client::ensure_daemon(&ctx.paths, true).await.map_err(|e| anyhow::anyhow!(e.to_string()))?;
    let cwd = std::env::current_dir()?;
    let bridge = polyph_mcp::bridge::LocalBridge::new(
        ctx.paths.daemon_info(),
        ctx.paths.token(),
        client.as_deref().unwrap_or("agent"),
        room.as_deref(),
        &cwd,
    );
    polyph_mcp::run_direct(bridge).await
}

/// Where a first-run name comes from when none is saved and `--me` is absent.
#[derive(Debug, PartialEq, Eq)]
enum NameSource {
    Git(String),
    Ask,
    Missing,
}

fn name_source(git: Option<String>, interactive: bool) -> NameSource {
    match git {
        Some(n) => NameSource::Git(n),
        None if interactive => NameSource::Ask,
        None => NameSource::Missing,
    }
}

/// `git config user.name`; `global` points it at one config file (for tests).
fn git_user_name(global: Option<&Path>) -> Option<String> {
    let mut cmd = std::process::Command::new("git");
    cmd.args(["config", "user.name"]);
    if let Some(g) = global {
        cmd.env("GIT_CONFIG_GLOBAL", g).env("GIT_CONFIG_NOSYSTEM", "1");
    }
    let out = cmd.output().ok().filter(|o| o.status.success())?;
    let n = String::from_utf8_lossy(&out.stdout).trim().to_string();
    (!n.is_empty()).then_some(n)
}

#[cfg(test)]
#[path = "setup_tests.rs"]
mod setup_tests;

/// `polyph start`: daemon, name, a room for this folder, an invite code, and
/// the person's agents connected, in one command.
pub async fn start(
    ctx: &Ctx,
    name: Option<String>,
    me: Option<String>,
    yes: bool,
    no_connect: bool,
) -> anyhow::Result<()> {
    let l = &ctx.l;
    let mut api = ctx.api().await?;
    let who = call(&mut api, "whoami", json!({})).await?;
    let me = me.map(|m| m.trim().to_string()).filter(|m| !m.is_empty());
    if let Some(me) = me {
        call(&mut api, "config.set", json!({ "key": "name", "value": me })).await?;
        ctx.out(tr!(l, "start-name-set", name = me.as_str()));
    } else if who["name"].as_str().is_none_or(|n| n.trim().is_empty()) {
        let interactive = std::io::stdin().is_terminal() && !yes;
        let chosen = match name_source(git_user_name(None), interactive) {
            NameSource::Git(n) => {
                ctx.out(tr!(l, "start-name-git", name = n.as_str()));
                n
            }
            NameSource::Ask => {
                let guess = std::env::var("USER").or_else(|_| std::env::var("USERNAME")).unwrap_or_default();
                let a = ask(&tr!(l, "start-ask-name", guess = guess.as_str()))?;
                let n = if a.is_empty() { guess } else { a };
                if !n.trim().is_empty() {
                    ctx.out(tr!(l, "start-name-set", name = n.trim()));
                }
                n
            }
            NameSource::Missing => String::new(),
        };
        if chosen.trim().is_empty() {
            anyhow::bail!(l.t("start-needs-name"));
        }
        call(&mut api, "config.set", json!({ "key": "name", "value": chosen.trim() })).await?;
    }
    let cwd = std::env::current_dir()?;
    let root = polyph_daemon::folders::root(&cwd);
    let linked = call(&mut api, "folder.find", json!({ "dir": cwd })).await?;
    let room = match linked["room"].as_str() {
        Some(r) => {
            let info = call(&mut api, "room.open", json!({ "room": r })).await?;
            ctx.out(tr!(l, "start-room-existing", name = info["name"].as_str().unwrap_or_default()));
            info
        }
        None => {
            let room_name = name.unwrap_or_else(|| {
                root.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "project".into())
            });
            let info = call(&mut api, "room.create", json!({ "name": room_name })).await?;
            let rid = info["id"].as_str().unwrap_or_default().to_string();
            call(&mut api, "folder.link", json!({ "room": rid, "dir": root })).await?;
            crate::repo::remember(ctx, &mut api, &rid, &root).await;
            ctx.out(tr!(
                l,
                "start-room-created",
                name = info["name"].as_str().unwrap_or_default(),
                dir = root.display().to_string()
            ));
            info
        }
    };
    let rid = room["id"].as_str().unwrap_or_default().to_string();
    if !no_connect {
        connect(ctx, &[], false, yes).await?;
    }
    let inv = call(&mut api, "room.invite", json!({ "room": rid, "reusable": true })).await?;
    ctx.out(String::new());
    crate::print_invite(ctx, &inv);
    let me = call(&mut api, "whoami", json!({})).await?;
    ctx.out(tr!(
        l,
        "start-ready",
        dir = root.display().to_string(),
        room = room["name"].as_str().unwrap_or_default(),
        name = me["name"].as_str().unwrap_or_default()
    ));
    ctx.out(tr!(l, "start-teammates", code = inv["code"].as_str().unwrap_or_default()));
    Ok(())
}

struct Check {
    ok: bool,
    text: String,
    fix: Option<String>,
}

fn pass(text: String) -> Check {
    Check { ok: true, text, fix: None }
}

fn fail(text: String, fix: impl Into<String>) -> Check {
    Check { ok: false, text, fix: Some(fix.into()) }
}

/// `polyph doctor`: checks the setup, printing the command that fixes each problem.
pub async fn doctor(ctx: &Ctx) -> anyhow::Result<()> {
    let l = &ctx.l;
    let mut checks: Vec<Check> = vec![];
    let git = std::process::Command::new("git").arg("--version").output().is_ok_and(|o| o.status.success());
    checks.push(if git {
        pass(l.t("doctor-git-ok"))
    } else {
        fail(l.t("doctor-git-missing"), "https://git-scm.com/downloads")
    });
    let port = match daemon_client::daemon_port(&ctx.paths) {
        Some(p) if daemon_client::healthy(p).await => Some(p),
        _ => None,
    };
    let mut api = match port {
        Some(_) => Api::connect(&ctx.paths, false).await.ok(),
        None => None,
    };
    match api.as_mut() {
        None => checks.push(fail(l.t("doctor-daemon-down"), "polyph daemon start")),
        Some(api) => {
            let ping = call(api, "ping", json!({})).await.unwrap_or_default();
            let v = ping["version"].as_str().unwrap_or_default().to_string();
            checks.push(if v == env!("CARGO_PKG_VERSION") {
                pass(tr!(l, "doctor-daemon-ok", version = v.as_str()))
            } else {
                fail(
                    tr!(l, "doctor-daemon-version", daemon = v.as_str(), cli = env!("CARGO_PKG_VERSION")),
                    "polyph daemon stop && polyph daemon start",
                )
            });
            let who = call(api, "whoami", json!({})).await.unwrap_or_default();
            checks.push(match who["name"].as_str().filter(|n| !n.trim().is_empty()) {
                Some(n) => pass(tr!(l, "doctor-name-ok", name = n)),
                None => fail(l.t("doctor-name-missing"), "polyph config set name \"Your Name\""),
            });
            let cwd = std::env::current_dir()?;
            let linked = call(api, "folder.find", json!({ "dir": cwd })).await.unwrap_or_default();
            match linked["room"].as_str() {
                None => checks.push(fail(l.t("doctor-folder-unlinked"), "polyph start")),
                Some(r) => {
                    let name = linked["name"].as_str().unwrap_or(r).to_string();
                    match call(api, "room.get", json!({ "room": r })).await {
                        Err(_) => checks.push(fail(tr!(l, "doctor-folder-gone", room = name.as_str()), "polyph start")),
                        Ok(info) if info["open"] != true => checks.push(fail(
                            tr!(l, "doctor-room-closed", room = name.as_str()),
                            format!("polyph room open {r}"),
                        )),
                        Ok(info) if info["connected"] != true => checks.push(fail(
                            tr!(
                                l,
                                "doctor-room-offline",
                                room = name.as_str(),
                                relay = info["relay"].as_str().unwrap_or_default()
                            ),
                            "polyph config get relay",
                        )),
                        Ok(_) => checks.push(pass(tr!(l, "doctor-room-ok", room = name.as_str()))),
                    }
                    if let Some(c) = crate::repo::doctor(ctx, api, r, &cwd).await {
                        checks.push(c.map_or_else(|(t, f)| fail(t, f), pass));
                    }
                }
            }
        }
    }
    if let Some(env) = Env::current() {
        for c in CLIENTS {
            let installed = c.installed(&env);
            match clients::current(c, &env) {
                None if c.registered_by_extension(&env) => {
                    checks.push(pass(tr!(l, "doctor-client-extension", client = c.name)))
                }
                None if installed => checks.push(fail(
                    tr!(l, "doctor-client-unconnected", client = c.name),
                    format!("polyph connect {}", c.id),
                )),
                None => {}
                Some(entry) => {
                    let target = clients::target(&entry).unwrap_or_default();
                    if let Some(rest) = target.strip_prefix("http://127.0.0.1:") {
                        let at: Option<u16> = rest.split('/').next().and_then(|p| p.parse().ok());
                        if port.is_some() && at != port {
                            checks.push(fail(
                                tr!(l, "doctor-client-port", client = c.name, port = port.unwrap_or(0).to_string()),
                                format!("polyph connect {} --http", c.id),
                            ));
                        } else {
                            checks.push(pass(tr!(l, "doctor-client-ok", client = c.name)));
                        }
                    } else if Path::new(&target).is_file() {
                        checks.push(pass(tr!(l, "doctor-client-ok", client = c.name)));
                    } else {
                        checks.push(fail(
                            tr!(l, "doctor-client-missing-exe", client = c.name, path = target.as_str()),
                            format!("polyph connect {}", c.id),
                        ));
                    }
                }
            }
        }
    }
    let problems = checks.iter().filter(|c| !c.ok).count();
    if ctx.json {
        let v: Vec<Value> = checks.iter().map(|c| json!({ "ok": c.ok, "text": c.text, "fix": c.fix })).collect();
        ctx.print_json(&json!({ "checks": v, "problems": problems }));
    } else {
        for c in &checks {
            ctx.out(format!("{} {}", if c.ok { "✓" } else { "✗" }, c.text));
            if let Some(f) = &c.fix {
                ctx.out(format!("    {}", tr!(l, "doctor-fix", command = f.as_str())));
            }
        }
    }
    if problems > 0 {
        anyhow::bail!(tr!(l, "doctor-problems", count = problems));
    }
    if !ctx.json {
        ctx.out(l.t("doctor-all-good"));
    }
    Ok(())
}
