//! `polyph`: the command line for Polyph. Every command talks to the local
//! daemon, starting it when needed. User-facing text comes from
//! `locales/*/cli.ftl`.

mod client;

use std::path::PathBuf;
use std::time::Duration;

use clap::{CommandFactory, FromArgMatches, Parser, Subcommand};
use polyph_daemon::{Daemon, DaemonOptions, Paths};
use polyph_i18n::{Localizer, tr};
use serde_json::{Value, json};

use client::{Api, CallError};

#[derive(Parser, Debug)]
#[command(name = "polyph", version)]
struct Cli {
    /// Language for messages (en, th, ja, zh-Hans).
    #[arg(long, global = true, env = "POLYPH_LANG")]
    lang: Option<String>,
    /// Print machine-readable JSON.
    #[arg(long, global = true)]
    json: bool,
    /// Polyph data directory (default: the platform config directory).
    #[arg(long, global = true, env = "POLYPH_HOME")]
    home: Option<PathBuf>,
    #[command(subcommand)]
    cmd: Option<Cmd>,
}

#[derive(Subcommand, Debug)]
enum Cmd {
    Daemon {
        #[command(subcommand)]
        cmd: DaemonCmd,
    },
    Whoami,
    Room {
        #[command(subcommand)]
        cmd: RoomCmd,
    },
    Host {
        /// Throwaway session deleted when you end it.
        #[arg(long)]
        quick: bool,
        /// Host on the local network with no internet needed.
        #[arg(long)]
        lan: bool,
        /// Let anyone with the code join without approval.
        #[arg(long)]
        no_approval: bool,
        /// Room name (persistent rooms only).
        #[arg(long)]
        name: Option<String>,
        /// Also connect the LAN room to your internet relay.
        #[arg(long)]
        bridge: bool,
        /// Code mode: distributed (default) or hosted.
        #[arg(long, default_value = "distributed")]
        mode: String,
    },
    Join {
        code: String,
        /// Your display name.
        #[arg(long)]
        name: Option<String>,
        /// Relay URL to use (for example a LAN address from `polyph discover`).
        #[arg(long)]
        relay: Option<String>,
        /// Look for the room on the local network.
        #[arg(long)]
        lan: bool,
    },
    Discover,
    Leave {
        room: Option<String>,
    },
    End {
        room: Option<String>,
        /// Save the board and chat into this repository before ending.
        #[arg(long)]
        export: Option<PathBuf>,
    },
    Chat {
        message: String,
        #[arg(long)]
        room: Option<String>,
    },
    Approve {
        id: String,
        /// Approve for the rest of this task, not just once.
        #[arg(long)]
        task: bool,
        /// Role for someone joining (viewer, member, maintainer).
        #[arg(long)]
        role: Option<String>,
        #[arg(long)]
        room: Option<String>,
    },
    Deny {
        id: String,
        #[arg(long)]
        note: Option<String>,
        #[arg(long)]
        room: Option<String>,
    },
    Claims {
        #[arg(long)]
        room: Option<String>,
    },
    Board {
        #[arg(long)]
        room: Option<String>,
    },
    Status {
        #[arg(long)]
        watch: bool,
    },
    Config {
        #[command(subcommand)]
        cmd: ConfigCmd,
    },
    Relay {
        #[command(subcommand)]
        cmd: RelayCmd,
    },
    Completions {
        shell: clap_complete::Shell,
    },
    Agent {
        #[command(subcommand)]
        cmd: AgentCmd,
    },
    Agents {
        #[command(subcommand)]
        cmd: AgentsCmd,
    },
    Sandbox {
        #[command(subcommand)]
        cmd: SandboxCmd,
    },
    Usage {
        #[arg(long)]
        room: Option<String>,
    },
    Budget {
        #[command(subcommand)]
        cmd: BudgetCmd,
    },
}

#[derive(Subcommand, Debug)]
enum AgentCmd {
    Start {
        /// Agent name: claude, codex, gemini, opencode, local, or one from polyph.toml.
        name: String,
        #[arg(long)]
        task: Option<String>,
        /// Run on your cloud agent host.
        #[arg(long)]
        on: Option<String>,
        /// Autonomy preset: ask-all, default or auto.
        #[arg(long)]
        preset: Option<String>,
        /// First prompt to send.
        #[arg(long)]
        prompt: Option<String>,
        #[arg(long)]
        room: Option<String>,
    },
    List {
        #[arg(long)]
        room: Option<String>,
    },
    Stop {
        id: String,
    },
    Follow {
        id: String,
        #[arg(long)]
        room: Option<String>,
    },
    Prompt {
        id: String,
        text: String,
    },
}

#[derive(Subcommand, Debug)]
enum AgentsCmd {
    Doctor,
}

#[derive(Subcommand, Debug)]
enum SandboxCmd {
    Test,
}

#[derive(Subcommand, Debug)]
enum BudgetCmd {
    Set {
        /// Limit in tokens.
        #[arg(long)]
        tokens: Option<u64>,
        /// Limit in estimated dollars.
        #[arg(long)]
        dollars: Option<f64>,
        /// Member id or name; omit for the whole room.
        #[arg(long)]
        member: Option<String>,
        #[arg(long)]
        room: Option<String>,
    },
}

#[derive(Subcommand, Debug)]
enum DaemonCmd {
    Run,
    Start,
    Stop,
    Status,
    Logs,
}

#[derive(Subcommand, Debug)]
enum RoomCmd {
    Create {
        name: String,
        #[arg(long)]
        no_approval: bool,
        #[arg(long)]
        lan: bool,
    },
    Open {
        name: String,
    },
    Close {
        name: Option<String>,
    },
    List,
    Invite {
        room: Option<String>,
        /// A code that can be used more than once.
        #[arg(long)]
        reusable: bool,
        /// Lifetime in minutes.
        #[arg(long)]
        expires: Option<i64>,
        #[arg(long)]
        max_uses: Option<u32>,
    },
    Members {
        room: Option<String>,
    },
    Knocks {
        room: Option<String>,
    },
    Settings {
        room: Option<String>,
        #[arg(long)]
        require_approval: Option<bool>,
    },
    Role {
        member: String,
        role: String,
        #[arg(long)]
        room: Option<String>,
    },
    Remove {
        member: String,
        #[arg(long)]
        room: Option<String>,
    },
    Delete {
        name: String,
    },
}

#[derive(Subcommand, Debug)]
enum ConfigCmd {
    Get { key: Option<String> },
    Set { key: String, value: String },
    Edit,
}

#[derive(Subcommand, Debug)]
enum RelayCmd {
    Serve {
        #[arg(long)]
        bind: Option<std::net::SocketAddr>,
        #[arg(long)]
        config: Option<PathBuf>,
    },
}

/// Replaces clap's English help with localized strings: subcommand `a b`
/// reads key `help-a-b`, the root reads `help-root`.
fn localize(mut cmd: clap::Command, l: &Localizer, path: &str) -> clap::Command {
    let key = if path.is_empty() { "help-root".to_string() } else { format!("help-{path}") };
    let text = l.t(&key);
    if text != key {
        cmd = cmd.about(text);
    }
    let names: Vec<String> = cmd.get_subcommands().map(|s| s.get_name().to_string()).collect();
    for n in names {
        let sub_path = if path.is_empty() { n.clone() } else { format!("{path}-{n}") };
        cmd = cmd.mut_subcommand(&n, |s| localize(s, l, &sub_path));
    }
    cmd
}

struct Ctx {
    l: Localizer,
    json: bool,
    paths: Paths,
}

impl Ctx {
    fn out(&self, s: impl AsRef<str>) {
        println!("{}", s.as_ref());
    }

    fn print_json(&self, v: &Value) {
        println!("{}", serde_json::to_string_pretty(v).unwrap_or_default());
    }

    async fn api(&self) -> anyhow::Result<Api> {
        Api::connect(&self.paths, true)
            .await
            .map_err(|e| anyhow::anyhow!(tr!(self.l, "api-unreachable", reason = e.to_string())))
    }

    /// The room argument, or the most recently opened room.
    async fn room(&self, api: &mut Api, room: Option<String>) -> anyhow::Result<String> {
        if let Some(r) = room {
            return Ok(r);
        }
        let s = call(api, "status", json!({})).await?;
        s.get("room")
            .and_then(|r| r.as_str())
            .map(str::to_string)
            .ok_or_else(|| anyhow::anyhow!(self.l.t("no-current-room")))
    }

    fn when(&self, at: i64) -> String {
        let secs = (at - polyph_proto::now()).max(60);
        if secs < 3600 {
            tr!(self.l, "when-minutes", n = secs / 60)
        } else if secs < 86_400 {
            tr!(self.l, "when-hours", n = secs / 3600)
        } else {
            tr!(self.l, "when-days", n = secs / 86_400)
        }
    }

    fn role(&self, r: &str) -> String {
        self.l.t(&format!("role-{r}"))
    }
}

async fn call(api: &mut Api, method: &str, params: Value) -> anyhow::Result<Value> {
    api.call(method, params).await.map_err(|e| match e {
        CallError::Api(m) => anyhow::anyhow!(m),
        CallError::Unreachable(m) => anyhow::anyhow!(m),
    })
}

fn main() {
    // The sandbox helper re-enters this binary before anything else runs.
    let raw: Vec<String> = std::env::args().collect();
    if raw.get(1).map(String::as_str) == Some("__sandbox") {
        std::process::exit(polyph_sandbox::helper::main_with_args(raw[2..].to_vec()));
    }
    let l0 = Localizer::new(&polyph_i18n::detect(None));
    let matches = localize(Cli::command(), &l0, "").get_matches();
    let cli = match Cli::from_arg_matches(&matches) {
        Ok(c) => c,
        Err(e) => e.exit(),
    };
    let lang = polyph_i18n::detect(cli.lang.as_deref());
    let paths = match cli.home.clone() {
        Some(h) => Paths::at(h),
        None => match Paths::from_env() {
            Ok(p) => p,
            Err(e) => {
                eprintln!("{e}");
                std::process::exit(1);
            }
        },
    };
    let ctx = Ctx { l: Localizer::new(&lang), json: cli.json, paths };
    let rt = match tokio::runtime::Builder::new_multi_thread().enable_all().build() {
        Ok(r) => r,
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(1);
        }
    };
    let code = rt.block_on(async {
        match run(&ctx, cli.cmd).await {
            Ok(()) => 0,
            Err(e) => {
                eprintln!("{e}");
                1
            }
        }
    });
    std::process::exit(code);
}

async fn run(ctx: &Ctx, cmd: Option<Cmd>) -> anyhow::Result<()> {
    let Some(cmd) = cmd else {
        // The terminal UI arrives in milestone 6; until then show help.
        let l = &ctx.l;
        let mut c = localize(Cli::command(), l, "");
        c.print_help()?;
        return Ok(());
    };
    let l = &ctx.l;
    match cmd {
        Cmd::Daemon { cmd } => daemon_cmd(ctx, cmd).await,
        Cmd::Relay { cmd: RelayCmd::Serve { bind, config } } => {
            init_stderr_logs();
            let mut cfg = polyph_relay::RelayConfig::load(config.as_deref())?;
            if let Some(b) = bind {
                cfg.bind = b;
            }
            let h = polyph_relay::serve(cfg, Default::default()).await?;
            ctx.out(tr!(l, "relay-listening", addr = h.addr.to_string()));
            let _ = tokio::signal::ctrl_c().await;
            h.shutdown().await;
            Ok(())
        }
        Cmd::Completions { shell } => {
            let mut c = Cli::command();
            clap_complete::generate(shell, &mut c, "polyph", &mut std::io::stdout());
            Ok(())
        }
        other => with_api(ctx, other).await,
    }
}

async fn with_api(ctx: &Ctx, cmd: Cmd) -> anyhow::Result<()> {
    let l = &ctx.l;
    let mut api = ctx.api().await?;
    match cmd {
        Cmd::Whoami => {
            let v = call(&mut api, "whoami", json!({})).await?;
            if ctx.json {
                ctx.print_json(&v);
            } else {
                let name = v["name"].as_str().map(str::to_string).unwrap_or_else(|| l.t("name-unset"));
                ctx.out(tr!(l, "whoami-name", name = name));
                ctx.out(tr!(
                    l,
                    "whoami-device",
                    device = v["device"].as_str().unwrap_or_default().chars().take(16).collect::<String>()
                ));
                ctx.out(tr!(
                    l,
                    "whoami-member",
                    member = v["member"].as_str().unwrap_or_default().chars().take(16).collect::<String>()
                ));
                ctx.out(tr!(l, "whoami-relay", relay = v["relay"].as_str().unwrap_or_default()));
                ctx.out(tr!(l, "whoami-keystore", store = v["keystore"].as_str().unwrap_or_default()));
            }
        }
        Cmd::Room { cmd } => room_cmd(ctx, &mut api, cmd).await?,
        Cmd::Host { quick, lan, no_approval, name, bridge, mode: _ } => {
            let name = name.unwrap_or_default();
            let room = call(&mut api, "room.create", json!({ "name": name, "quick": quick || name.is_empty(), "require_approval": !no_approval, "lan": lan, "bridge": bridge })).await?;
            let rid = room["id"].as_str().unwrap_or_default().to_string();
            let inv = call(&mut api, "room.invite", json!({ "room": rid })).await?;
            if ctx.json {
                ctx.print_json(&json!({ "room": room, "invite": inv }));
            } else {
                if room["quick"].as_bool() == Some(true) {
                    ctx.out(l.t("quick-created"));
                } else {
                    ctx.out(tr!(l, "room-created", name = room["name"].as_str().unwrap_or_default()));
                }
                print_invite(ctx, &inv);
            }
        }
        Cmd::Join { code, name, relay, lan } => {
            if !ctx.json {
                ctx.out(l.t("join-connecting"));
            }
            let mut waiting_shown = false;
            let r = api
                .call_with_events("join", json!({ "code": code, "name": name, "relay": relay, "lan": lan }), |ev| {
                    if ev["params"]["event"]["type"] == "join_pending" && !waiting_shown && !ctx.json {
                        waiting_shown = true;
                        println!("{}", l.t("join-waiting"));
                    }
                })
                .await
                .map_err(|e| anyhow::anyhow!(e.to_string()))?;
            if ctx.json {
                ctx.print_json(&r);
            } else {
                ctx.out(tr!(
                    l,
                    "joined",
                    name = r["name"].as_str().unwrap_or_default(),
                    role = ctx.role(r["role"].as_str().unwrap_or("member"))
                ));
            }
        }
        Cmd::Discover => {
            if !ctx.json {
                ctx.out(l.t("discover-searching"));
            }
            let v = call(&mut api, "lan.discover", json!({ "timeout_ms": 2500 })).await?;
            if ctx.json {
                ctx.print_json(&v);
            } else if v.as_array().is_none_or(|a| a.is_empty()) {
                ctx.out(l.t("discover-empty"));
            } else {
                for d in v.as_array().into_iter().flatten() {
                    ctx.out(tr!(
                        l,
                        "discover-line",
                        name = d["name"].as_str().unwrap_or_default(),
                        url = d["url"].as_str().unwrap_or_default()
                    ));
                }
                ctx.out(l.t("discover-hint"));
            }
        }
        Cmd::Leave { room } => {
            let room = ctx.room(&mut api, room).await?;
            let info = call(&mut api, "room.get", json!({ "room": room })).await?;
            call(&mut api, "room.close", json!({ "room": room })).await?;
            ctx.out(tr!(l, "room-closed", name = display_name(ctx, &info)));
        }
        Cmd::End { room, export } => {
            let room = ctx.room(&mut api, room).await?;
            let info = call(&mut api, "room.get", json!({ "room": room })).await?;
            let r = call(&mut api, "room.end", json!({ "room": room, "export_dir": export })).await?;
            if let Some(p) = r["exported"].as_str() {
                ctx.out(tr!(l, "room-exported", path = p));
            }
            ctx.out(tr!(l, "room-ended", name = display_name(ctx, &info)));
        }
        Cmd::Chat { message, room } => {
            let room = ctx.room(&mut api, room).await?;
            call(&mut api, "chat.send", json!({ "room": room, "text": message })).await?;
            if !ctx.json {
                ctx.out(l.t("chat-sent"));
            }
        }
        Cmd::Approve { id, task, role, room } => {
            let room = ctx.room(&mut api, room).await?;
            decide(ctx, &mut api, &room, &id, true, task, role, None).await?;
        }
        Cmd::Deny { id, note, room } => {
            let room = ctx.room(&mut api, room).await?;
            decide(ctx, &mut api, &room, &id, false, false, None, note).await?;
        }
        Cmd::Claims { room } => {
            let room = ctx.room(&mut api, room).await?;
            let v = call(&mut api, "claims.list", json!({ "room": room })).await?;
            if ctx.json {
                ctx.print_json(&v);
            } else if v.as_array().is_none_or(|a| a.is_empty()) {
                ctx.out(l.t("claims-empty"));
            } else {
                for c in v.as_array().into_iter().flatten() {
                    let target = match c["target"]["symbol"].as_str() {
                        Some(s) => format!("{}#{s}", c["target"]["path"].as_str().unwrap_or_default()),
                        None => c["target"]["path"].as_str().unwrap_or_default().to_string(),
                    };
                    ctx.out(tr!(
                        l,
                        "claim-line",
                        target = target,
                        agent = c["owner"]["label"].as_str().unwrap_or_default(),
                        reason = c["reason"].as_str().unwrap_or_default()
                    ));
                }
            }
        }
        Cmd::Board { room } => {
            let room = ctx.room(&mut api, room).await?;
            let v = call(&mut api, "board.get", json!({ "room": room })).await?;
            if ctx.json {
                ctx.print_json(&v);
            } else {
                for col in v["columns"].as_array().into_iter().flatten() {
                    let id = col["id"].as_str().unwrap_or_default();
                    let title = l.t(&format!("column-{}", id.replace('_', "-")));
                    ctx.out(format!("\n{title}"));
                    let cards: Vec<&Value> =
                        v["cards"].as_array().into_iter().flatten().filter(|c| c["column"] == id).collect();
                    if cards.is_empty() {
                        ctx.out(format!("  {}", l.t("board-empty-column")));
                    }
                    for c in cards {
                        ctx.out(format!(
                            "  {}  {}",
                            c["id"].as_str().unwrap_or_default().chars().take(6).collect::<String>(),
                            c["title"].as_str().unwrap_or_default()
                        ));
                    }
                }
            }
        }
        Cmd::Status { watch } => {
            if watch {
                let first = call(&mut api, "status.watch", json!({})).await;
                drop(first);
                while let Some(n) = api.next_notification().await {
                    if n["method"] == "status" {
                        println!("{}", serde_json::to_string(&n["params"]).unwrap_or_default());
                    }
                }
            } else {
                let s = call(&mut api, "status", json!({})).await?;
                if ctx.json {
                    ctx.print_json(&s);
                } else if s["room"].is_null() {
                    ctx.out(l.t("no-current-room"));
                } else {
                    ctx.out(tr!(
                        l,
                        "status-room",
                        name = s["room_name"]
                            .as_str()
                            .filter(|n| !n.is_empty())
                            .map(str::to_string)
                            .unwrap_or_else(|| l.t("quick-session-name"))
                    ));
                    ctx.out(if s["connected"] == true { l.t("status-connected") } else { l.t("status-disconnected") });
                    ctx.out(tr!(l, "status-members", count = s["members_online"].as_u64().unwrap_or(0)));
                    ctx.out(tr!(
                        l,
                        "status-approvals",
                        count = s["approvals"].as_array().map(|a| a.len()).unwrap_or(0)
                    ));
                }
            }
        }
        Cmd::Config { cmd } => match cmd {
            ConfigCmd::Get { key } => {
                let v = call(&mut api, "config.get", json!({ "key": key })).await?;
                if ctx.json || key.is_none() {
                    ctx.print_json(&v);
                } else {
                    ctx.out(v["value"].as_str().unwrap_or_default());
                }
            }
            ConfigCmd::Set { key, value } => {
                call(&mut api, "config.set", json!({ "key": key, "value": value })).await?;
                ctx.out(l.t("config-set"));
            }
            ConfigCmd::Edit => {
                let path = ctx.paths.config();
                let editor = std::env::var("VISUAL")
                    .or_else(|_| std::env::var("EDITOR"))
                    .unwrap_or_else(|_| if cfg!(windows) { "notepad".into() } else { "vi".into() });
                let ok = std::process::Command::new(editor).arg(&path).status().map(|s| s.success()).unwrap_or(false);
                if !ok {
                    ctx.out(tr!(l, "editor-failed", path = path.display().to_string()));
                }
            }
        },
        Cmd::Agent { cmd } => agent_cmd(ctx, &mut api, cmd).await?,
        Cmd::Agents { cmd: AgentsCmd::Doctor } => {
            let cwd = std::env::current_dir()?;
            let v = call(&mut api, "agents.doctor", json!({ "cwd": cwd })).await?;
            if ctx.json {
                ctx.print_json(&v);
            } else {
                for a in v["agents"].as_array().into_iter().flatten() {
                    let name = a["name"].as_str().unwrap_or_default();
                    let line = if a["adapter_installed"] != true || a["cli_installed"] == false {
                        tr!(l, "doctor-missing", name = name)
                    } else if a["logged_in"] == false {
                        tr!(l, "doctor-logged-out", name = name)
                    } else {
                        tr!(l, "doctor-ok", name = name)
                    };
                    ctx.out(line);
                    for f in a["fixes"].as_array().into_iter().flatten() {
                        let key = f[0].as_str().unwrap_or_default();
                        ctx.out(format!("  {}", tr!(l, key, cmd = f[1].as_str().unwrap_or_default())));
                    }
                }
                for s in v["sandbox"].as_array().into_iter().flatten().filter(|s| s["available"] == true).take(1) {
                    ctx.out(tr!(
                        l,
                        "doctor-sandbox",
                        mechanism = s["mechanism"].as_str().unwrap_or_default(),
                        detail = s["detail"].as_str().unwrap_or_default()
                    ));
                }
            }
        }
        Cmd::Sandbox { cmd: SandboxCmd::Test } => {
            let v = call(&mut api, "sandbox.test", json!({})).await?;
            if ctx.json {
                ctx.print_json(&v);
            } else {
                let mut any = false;
                for m in v.as_array().into_iter().flatten() {
                    ctx.out(tr!(
                        l,
                        "sandbox-mechanism",
                        mechanism = m["mechanism"].as_str().unwrap_or_default(),
                        detail = m["detail"].as_str().unwrap_or_default()
                    ));
                    for c in m["checks"].as_array().into_iter().flatten() {
                        any = true;
                        let mark = if c["passed"] == true { l.t("sandbox-pass") } else { l.t("sandbox-fail") };
                        ctx.out(format!("  {mark}  {}", l.t(c["key"].as_str().unwrap_or_default())));
                    }
                }
                if !any {
                    ctx.out(l.t("sandbox-none"));
                }
            }
        }
        Cmd::Usage { room } => {
            let room = ctx.room(&mut api, room).await?;
            let cwd = std::env::current_dir()?;
            let v = call(&mut api, "usage.get", json!({ "room": room, "cwd": cwd })).await?;
            if ctx.json {
                ctx.print_json(&v);
            } else {
                let tot = |t: &Value| {
                    ["input_tokens", "output_tokens", "cache_read_tokens", "cache_write_tokens"]
                        .iter()
                        .map(|k| t[*k].as_u64().unwrap_or(0))
                        .sum::<u64>()
                };
                let cost = |t: &Value| format!("{:.2}", t["cost"].as_f64().unwrap_or(0.0));
                let r = &v["ledger"]["room"];
                ctx.out(tr!(l, "usage-room", tokens = tot(r), cost = cost(r)));
                for (who, t) in v["ledger"]["by_agent"].as_object().into_iter().flatten() {
                    ctx.out(format!("  {}", tr!(l, "usage-line", who = who.as_str(), tokens = tot(t), cost = cost(t))));
                }
                let unknown = r["unknown_turns"].as_u64().unwrap_or(0);
                if unknown > 0 {
                    ctx.out(tr!(l, "usage-unknown", count = unknown));
                }
                if r["subscription"] == true {
                    ctx.out(l.t("usage-subscription"));
                }
                for b in v["budgets"].as_array().into_iter().flatten() {
                    let scope = b["budget"]["scope"]
                        .as_object()
                        .and_then(|o| o.get("member"))
                        .and_then(|m| m.as_str())
                        .map(|m| m.chars().take(8).collect())
                        .unwrap_or_else(|| l.t("budget-room"));
                    let state = match b["state"]
                        .as_str()
                        .unwrap_or_else(|| if b["state"].is_object() { "warning" } else { "ok" })
                    {
                        "exceeded" => l.t("budget-state-exceeded"),
                        "ok" => l.t("budget-state-ok"),
                        _ => l.t("budget-state-warning"),
                    };
                    ctx.out(tr!(
                        l,
                        "budget-line",
                        scope = scope,
                        used = format!("{:.0}", b["used"].as_f64().unwrap_or(0.0)),
                        limit = b["budget"]["limit"]["amount"].to_string(),
                        unit = b["budget"]["limit"]["unit"].as_str().unwrap_or_default(),
                        state = state
                    ));
                }
            }
        }
        Cmd::Budget { cmd: BudgetCmd::Set { tokens, dollars, member, room } } => {
            let room = ctx.room(&mut api, room).await?;
            let limit = match (tokens, dollars) {
                (Some(t), _) => json!({ "unit": "tokens", "amount": t }),
                (None, Some(d)) => json!({ "unit": "dollars", "amount": d }),
                (None, None) => anyhow::bail!("give --tokens or --dollars"),
            };
            let scope = match member {
                Some(m) => {
                    let members = call(&mut api, "room.members", json!({ "room": room })).await?;
                    let id = members
                        .as_array()
                        .into_iter()
                        .flatten()
                        .find(|x| {
                            x["name"].as_str() == Some(m.as_str())
                                || x["member"].as_str().is_some_and(|i| i.starts_with(&m))
                        })
                        .and_then(|x| x["member"].as_str())
                        .map(str::to_string)
                        .unwrap_or(m);
                    json!({ "member": id })
                }
                None => json!("room"),
            };
            let current = call(&mut api, "usage.get", json!({ "room": room })).await?;
            let mut budgets: Vec<Value> = current["budgets"]
                .as_array()
                .into_iter()
                .flatten()
                .map(|b| b["budget"].clone())
                .filter(|b| b["scope"] != scope)
                .collect();
            budgets.push(json!({ "scope": scope, "limit": limit }));
            call(&mut api, "budget.set", json!({ "room": room, "budgets": budgets })).await?;
            ctx.out(l.t("budget-saved"));
        }
        Cmd::Daemon { .. } | Cmd::Relay { .. } | Cmd::Completions { .. } => {}
    }
    Ok(())
}

/// Renders a summary line (translation key + JSON args) with the localizer.
fn render_summary(l: &Localizer, line: &Value) -> String {
    let key = line["key"].as_str().unwrap_or_default();
    let mut args: Vec<(String, polyph_i18n::FluentValue<'static>)> = vec![];
    for (k, v) in line["args"].as_object().into_iter().flatten() {
        let fv = match v {
            Value::Number(n) => polyph_i18n::FluentValue::from(n.as_f64().unwrap_or(0.0)),
            Value::String(s) => polyph_i18n::FluentValue::from(s.clone()),
            other => polyph_i18n::FluentValue::from(other.to_string()),
        };
        args.push((k.clone(), fv));
    }
    let refs: Vec<(&str, polyph_i18n::FluentValue<'_>)> = args.iter().map(|(k, v)| (k.as_str(), v.clone())).collect();
    l.t_args(key, &refs)
}

fn render_event(ctx: &Ctx, e: &Value) -> Option<String> {
    let l = &ctx.l;
    let label = e["agent"]["label"].as_str().unwrap_or_default();
    let kind = &e["kind"];
    match kind["t"].as_str()? {
        "prompt" => Some(tr!(
            l,
            "follow-prompt",
            who = label.split('/').next().unwrap_or_default(),
            text = kind["c"]["text"].as_str().unwrap_or_default()
        )),
        "message" => Some(kind["c"]["text"].as_str().unwrap_or_default().trim_end().to_string()),
        "thought" => Some(format!("  … {}", kind["c"]["text"].as_str().unwrap_or_default().trim())),
        "tool" => {
            let t = &kind["c"];
            if !matches!(t["status"].as_str(), Some("completed" | "failed")) {
                return None;
            }
            let lines = render_tools(l, std::slice::from_ref(t));
            Some(format!("  • {}", lines.join(" · ")))
        }
        "state" => Some(tr!(
            l,
            "follow-state",
            label = label,
            state = l.t(&format!("state-{}", kind["c"]["state"].as_str().unwrap_or("idle").replace('_', "-")))
        )),
        _ => None,
    }
}

fn render_tools(l: &Localizer, calls: &[Value]) -> Vec<String> {
    let parsed: Vec<polyph_proto::messages::ToolCall> =
        calls.iter().filter_map(|c| serde_json::from_value(c.clone()).ok()).collect();
    let lines = polyph_agents::summary::summarize(&parsed);
    lines.iter().map(|ln| render_summary(l, &serde_json::to_value(ln).unwrap_or(Value::Null))).collect()
}

async fn agent_cmd(ctx: &Ctx, api: &mut Api, cmd: AgentCmd) -> anyhow::Result<()> {
    let l = &ctx.l;
    match cmd {
        AgentCmd::Start { name, task, on, preset, prompt, room } => {
            if on.is_some() {
                anyhow::bail!(l.t("agent-cloud-unavailable"));
            }
            let room = ctx.room(api, room).await?;
            let cwd = std::env::current_dir()?;
            let v = call(
                api,
                "agent.start",
                json!({ "room": room, "kind": name, "cwd": cwd, "preset": preset, "task": task, "prompt": prompt }),
            )
            .await?;
            if ctx.json {
                ctx.print_json(&v);
            } else {
                ctx.out(tr!(
                    l,
                    "agent-started",
                    label = v["label"].as_str().unwrap_or_default(),
                    sandbox = v["sandbox"].as_str().unwrap_or_default()
                ));
            }
        }
        AgentCmd::List { room } => {
            let room = ctx.room(api, room).await?;
            let v = call(api, "agent.list", json!({ "room": room })).await?;
            if ctx.json {
                ctx.print_json(&v);
            } else if v.as_array().is_none_or(|a| a.is_empty()) {
                ctx.out(l.t("agents-empty"));
            } else {
                for a in v.as_array().into_iter().flatten() {
                    let state = l.t(&format!("state-{}", a["state"].as_str().unwrap_or("idle").replace('_', "-")));
                    ctx.out(format!(
                        "{}  {}",
                        a["id"].as_str().unwrap_or_default().chars().take(8).collect::<String>(),
                        tr!(l, "agent-line", label = a["label"].as_str().unwrap_or_default(), state = state)
                    ));
                }
            }
        }
        AgentCmd::Stop { id } => {
            call(api, "agent.stop", json!({ "agent": id })).await?;
            ctx.out(l.t("agent-stopped"));
        }
        AgentCmd::Prompt { id, text } => {
            let v = call(api, "agent.prompt", json!({ "agent": id, "text": text })).await?;
            if ctx.json {
                ctx.print_json(&v);
            } else {
                ctx.out(tr!(l, "agent-prompted", label = id, reason = v["stop_reason"].as_str().unwrap_or_default()));
            }
        }
        AgentCmd::Follow { id, room } => {
            let room = ctx.room(api, room).await?;
            let t = call(api, "agent.transcript", json!({ "room": room, "agent": id })).await?;
            let agent_id = t["events"]
                .as_array()
                .and_then(|e| e.first())
                .map(|e| e["agent"]["id"].as_str().unwrap_or_default().to_string())
                .unwrap_or_default();
            for e in t["events"].as_array().into_iter().flatten() {
                if ctx.json {
                    println!("{e}");
                } else if let Some(line) = render_event(ctx, e) {
                    ctx.out(line);
                }
            }
            while let Some(n) = api.next_notification().await {
                let ev = &n["params"]["event"];
                if ev["type"] == "agent" && ev["event"]["agent"]["id"] == agent_id.as_str() {
                    if ctx.json {
                        println!("{}", ev["event"]);
                    } else if let Some(line) = render_event(ctx, &ev["event"]) {
                        ctx.out(line);
                    }
                }
            }
        }
    }
    Ok(())
}

fn display_name(ctx: &Ctx, info: &Value) -> String {
    info["name"].as_str().filter(|n| !n.is_empty()).map(str::to_string).unwrap_or_else(|| ctx.l.t("quick-session-name"))
}

fn print_invite(ctx: &Ctx, inv: &Value) {
    let l = &ctx.l;
    let code = inv["code"].as_str().unwrap_or_default();
    ctx.out(tr!(l, "invite-code", code = code));
    ctx.out(tr!(l, "invite-link", link = format!("https://{}/j/{code}", web_domain())));
    ctx.out(tr!(l, "invite-expires", when = ctx.when(inv["expires_at"].as_i64().unwrap_or(0))));
    ctx.out(if inv["reusable"] == true { l.t("invite-reusable") } else { l.t("invite-one-time") });
    if inv["pending"] == true {
        ctx.out(l.t("invite-pending"));
    }
    if let Some(lan) = inv["lan"].as_str() {
        ctx.out(tr!(l, "invite-lan", code = code, url = lan.split('#').next().unwrap_or(lan)));
    }
}

/// Website domain for join links. Set at build time; the product domain is
/// not final (see STUBS.md).
fn web_domain() -> &'static str {
    option_env!("POLYPH_DOMAIN").unwrap_or("polyph.dev")
}

#[allow(clippy::too_many_arguments)]
async fn decide(
    ctx: &Ctx,
    api: &mut Api,
    room: &str,
    id: &str,
    yes: bool,
    task: bool,
    role: Option<String>,
    note: Option<String>,
) -> anyhow::Result<()> {
    let l = &ctx.l;
    let knocks = call(api, "knock.list", json!({ "room": room })).await?;
    let is_knock = knocks
        .as_array()
        .into_iter()
        .flatten()
        .any(|k| k["id"].as_str().is_some_and(|k| k.starts_with(&id.to_lowercase())));
    if is_knock {
        call(api, "knock.decide", json!({ "room": room, "knock": id, "approve": yes, "role": role })).await?;
        ctx.out(if yes { l.t("admitted") } else { l.t("not-admitted") });
        return Ok(());
    }
    let method = if yes { "approve" } else { "deny" };
    match call(api, method, json!({ "room": room, "id": id, "task": task, "note": note })).await {
        Ok(_) => ctx.out(if yes { l.t("approved") } else { l.t("denied") }),
        Err(_) => anyhow::bail!(tr!(l, "nothing-matches", id = id)),
    }
    Ok(())
}

async fn room_cmd(ctx: &Ctx, api: &mut Api, cmd: RoomCmd) -> anyhow::Result<()> {
    let l = &ctx.l;
    match cmd {
        RoomCmd::Create { name, no_approval, lan } => {
            let r =
                call(api, "room.create", json!({ "name": name, "require_approval": !no_approval, "lan": lan })).await?;
            if ctx.json {
                ctx.print_json(&r);
            } else {
                ctx.out(tr!(l, "room-created", name = r["name"].as_str().unwrap_or_default()));
            }
        }
        RoomCmd::Open { name } => {
            let r = call(api, "room.open", json!({ "room": name })).await?;
            ctx.out(tr!(l, "room-opened", name = display_name(ctx, &r)));
        }
        RoomCmd::Close { name } => {
            let room = ctx.room(api, name).await?;
            let info = call(api, "room.get", json!({ "room": room })).await?;
            call(api, "room.close", json!({ "room": room })).await?;
            ctx.out(tr!(l, "room-closed", name = display_name(ctx, &info)));
        }
        RoomCmd::List => {
            let v = call(api, "room.list", json!({})).await?;
            if ctx.json {
                ctx.print_json(&v);
            } else if v.as_array().is_none_or(|a| a.is_empty()) {
                ctx.out(l.t("rooms-empty"));
            } else {
                for r in v.as_array().into_iter().flatten() {
                    let state = if r["open"] != true {
                        l.t("room-state-closed")
                    } else if r["connected"] == true {
                        l.t("room-state-connected")
                    } else {
                        l.t("room-state-connecting")
                    };
                    ctx.out(tr!(
                        l,
                        "room-line",
                        name = display_name(ctx, r),
                        role = ctx.role(r["role"].as_str().unwrap_or("member")),
                        state = state,
                        online = r["members_online"].as_u64().unwrap_or(0)
                    ));
                }
            }
        }
        RoomCmd::Invite { room, reusable, expires, max_uses } => {
            let room = ctx.room(api, room).await?;
            let inv = call(api, "room.invite", json!({ "room": room, "reusable": reusable, "expires_secs": expires.map(|m| m * 60), "max_uses": max_uses })).await?;
            if ctx.json {
                ctx.print_json(&inv);
            } else {
                print_invite(ctx, &inv);
            }
        }
        RoomCmd::Members { room } => {
            let room = ctx.room(api, room).await?;
            let info = call(api, "room.get", json!({ "room": room })).await?;
            let v = call(api, "room.members", json!({ "room": room })).await?;
            if ctx.json {
                ctx.print_json(&v);
            } else {
                ctx.out(tr!(l, "members-header", room = display_name(ctx, &info)));
                for m in v.as_array().into_iter().flatten() {
                    let mut name = m["name"].as_str().unwrap_or_default().to_string();
                    if m["me"] == true {
                        name = format!("{name} {}", l.t("member-you"));
                    }
                    let state = if m["online"] == true { l.t("member-online") } else { l.t("member-offline") };
                    ctx.out(tr!(
                        l,
                        "member-line",
                        name = name,
                        role = ctx.role(m["role"].as_str().unwrap_or("member")),
                        state = state
                    ));
                }
            }
        }
        RoomCmd::Knocks { room } => {
            let room = ctx.room(api, room).await?;
            let v = call(api, "knock.list", json!({ "room": room })).await?;
            if ctx.json {
                ctx.print_json(&v);
            } else if v.as_array().is_none_or(|a| a.is_empty()) {
                ctx.out(l.t("knocks-empty"));
            } else {
                for k in v.as_array().into_iter().flatten() {
                    let id: String = k["id"].as_str().unwrap_or_default().chars().take(6).collect();
                    ctx.out(tr!(
                        l,
                        "knock-line",
                        id = id.clone(),
                        name = k["display_name"].as_str().unwrap_or_default()
                    ));
                    ctx.out(tr!(l, "knock-hint", id = id));
                }
            }
        }
        RoomCmd::Settings { room, require_approval } => {
            let room = ctx.room(api, room).await?;
            let v = call(api, "room.settings", json!({ "room": room, "require_approval": require_approval })).await?;
            if ctx.json {
                ctx.print_json(&v);
            } else {
                ctx.out(if v["require_approval"] == true {
                    l.t("settings-approval-on")
                } else {
                    l.t("settings-approval-off")
                });
            }
        }
        RoomCmd::Role { member, role, room } => {
            let room = ctx.room(api, room).await?;
            call(api, "room.set_role", json!({ "room": room, "member": member, "role": role })).await?;
            ctx.out(l.t("role-changed"));
        }
        RoomCmd::Remove { member, room } => {
            let room = ctx.room(api, room).await?;
            call(api, "room.remove", json!({ "room": room, "member": member })).await?;
            ctx.out(l.t("member-removed"));
        }
        RoomCmd::Delete { name } => {
            let info = call(api, "room.get", json!({ "room": name })).await?;
            call(api, "room.delete", json!({ "room": name })).await?;
            ctx.out(tr!(l, "room-deleted", name = display_name(ctx, &info)));
        }
    }
    Ok(())
}

fn init_stderr_logs() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .with_writer(std::io::stderr)
        .try_init();
}

async fn daemon_cmd(ctx: &Ctx, cmd: DaemonCmd) -> anyhow::Result<()> {
    let l = &ctx.l;
    match cmd {
        DaemonCmd::Run => {
            ctx.paths.ensure()?;
            let log = std::fs::OpenOptions::new().create(true).append(true).open(ctx.paths.log())?;
            let _ = tracing_subscriber::fmt()
                .with_env_filter(
                    tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
                )
                .json()
                .with_writer(std::sync::Mutex::new(log))
                .try_init();
            let daemon = Daemon::start(DaemonOptions::new(ctx.paths.clone())).await?;
            let port = daemon.shared.config.lock().api_port;
            let server = polyph_daemon::api::serve(daemon.clone(), port).await?;
            tracing::info!(port = server.addr.port(), "local API listening");
            tokio::select! {
                _ = tokio::signal::ctrl_c() => {}
                _ = daemon.shutdown.notified() => {}
            }
            daemon.stop().await;
            server.abort();
            let _ = std::fs::remove_file(ctx.paths.daemon_info());
            Ok(())
        }
        DaemonCmd::Start => {
            ctx.out(l.t("daemon-starting"));
            match client::ensure_daemon(&ctx.paths, true).await {
                Ok(p) => ctx.out(tr!(l, "daemon-started", port = p)),
                Err(_) => anyhow::bail!(l.t("daemon-start-failed")),
            }
            Ok(())
        }
        DaemonCmd::Stop => {
            match Api::connect(&ctx.paths, false).await {
                Ok(mut api) => {
                    let _ = api.call("shutdown", json!({})).await;
                    for _ in 0..50 {
                        tokio::time::sleep(Duration::from_millis(100)).await;
                        if client::daemon_port(&ctx.paths).is_none() {
                            break;
                        }
                    }
                    ctx.out(l.t("daemon-stopped"));
                }
                Err(_) => ctx.out(l.t("daemon-not-running")),
            }
            Ok(())
        }
        DaemonCmd::Status => {
            match Api::connect(&ctx.paths, false).await {
                Ok(mut api) => {
                    let v = call(&mut api, "ping", json!({})).await?;
                    let port = client::daemon_port(&ctx.paths).unwrap_or(0);
                    if ctx.json {
                        ctx.print_json(&json!({ "running": true, "port": port, "version": v["version"] }));
                    } else {
                        ctx.out(tr!(
                            l,
                            "daemon-running",
                            version = v["version"].as_str().unwrap_or_default(),
                            port = port
                        ));
                    }
                }
                Err(_) => {
                    if ctx.json {
                        ctx.print_json(&json!({ "running": false }));
                    } else {
                        ctx.out(l.t("daemon-not-running"));
                    }
                }
            }
            Ok(())
        }
        DaemonCmd::Logs => {
            match std::fs::read_to_string(ctx.paths.log()) {
                Ok(s) => print!("{s}"),
                Err(_) => ctx.out(l.t("daemon-no-log")),
            }
            Ok(())
        }
    }
}
