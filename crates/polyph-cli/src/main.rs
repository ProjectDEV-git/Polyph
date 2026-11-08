//! `polyph`: the command line for Polyph. Every command talks to the local
//! daemon, starting it when needed. User-facing text comes from
//! `locales/*/cli.ftl`.

mod account;
mod client;
mod feedback;
mod hook;
mod knowledge;
mod repo;
mod setup;
mod update;
mod work;

#[cfg(test)]
mod arg_tests;

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
    Feedback {
        /// Where the problem happened.
        #[arg(long, default_value = "cli", value_parser = clap::builder::PossibleValuesParser::new(feedback::SURFACES))]
        surface: String,
        /// Print the link instead of opening the browser.
        #[arg(long)]
        print: bool,
    },
    Web {
        /// Print the link instead of opening the browser.
        #[arg(long)]
        no_open: bool,
    },
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
        /// Your name as teammates see it (saved if you have none yet).
        #[arg(long = "me", alias = "name")]
        me: Option<String>,
        /// Relay URL to use (for example a LAN address from `polyph discover`).
        #[arg(long)]
        relay: Option<String>,
        /// Look for the room on the local network.
        #[arg(long)]
        lan: bool,
        /// Clone the room's repository into this (empty) folder without asking.
        #[arg(long, conflicts_with = "no_clone")]
        clone: bool,
        /// Do not offer to clone the room's repository.
        #[arg(long)]
        no_clone: bool,
    },
    Discover,
    Phone {
        room: Option<String>,
        /// Phone app address (default: the website's /phone/).
        #[arg(long)]
        base: Option<String>,
    },
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
    #[command(name = "agent-host")]
    AgentHost {
        #[command(subcommand)]
        cmd: HostCmd,
    },
    Plugin {
        #[command(subcommand)]
        cmd: PluginCmd,
    },
    Skills {
        #[command(subcommand)]
        cmd: SkillsCmd,
    },
    Template {
        #[command(subcommand)]
        cmd: TemplateCmd,
    },
    Agents {
        #[command(subcommand)]
        cmd: AgentsCmd,
    },
    Sandbox {
        #[command(subcommand)]
        cmd: SandboxCmd,
    },
    Mcp {
        /// How the agent shows in the room (claude-code, codex, cursor...).
        #[arg(long)]
        client: Option<String>,
        /// Room to join (default: the room linked to this folder).
        #[arg(long)]
        room: Option<String>,
    },
    Connect {
        /// Clients to connect (default: every one found on this computer).
        clients: Vec<String>,
        /// Use the daemon's local web address instead of starting `polyph mcp`.
        #[arg(long)]
        http: bool,
        /// Write the changes without asking.
        #[arg(long, short = 'y')]
        yes: bool,
    },
    Disconnect {
        /// Clients to disconnect (default: all).
        clients: Vec<String>,
        /// Remove Polyph without asking.
        #[arg(long, short = 'y')]
        yes: bool,
    },
    Start {
        /// Name for a new room (default: the folder's name).
        #[arg(long = "room", alias = "name")]
        room: Option<String>,
        /// Your name as teammates see it. Saved, and used as your name in the room.
        #[arg(long)]
        me: Option<String>,
        /// Never ask: take your name from --me or git, and connect agents without confirming.
        #[arg(long, short = 'y')]
        yes: bool,
        /// Do not connect agents.
        #[arg(long)]
        no_connect: bool,
    },
    Doctor,
    Trust {
        /// Stop trusting this repository's polyph.toml.
        #[arg(long)]
        revoke: bool,
        /// Trust without asking, after reviewing the file (for scripts).
        #[arg(long, short = 'y')]
        yes: bool,
    },
    Hook {
        #[command(subcommand)]
        cmd: HookCmd,
    },
    Repo {
        #[command(subcommand)]
        cmd: Option<repo::RepoCmd>,
        #[arg(long, global = true)]
        room: Option<String>,
    },
    Brief {
        #[command(subcommand)]
        cmd: Option<knowledge::BriefCmd>,
        #[arg(long, global = true)]
        room: Option<String>,
    },
    Search {
        #[arg(required = true)]
        query: Vec<String>,
        #[arg(long)]
        room: Option<String>,
    },
    Catchup {
        #[arg(long)]
        room: Option<String>,
    },
    Areas {
        /// A file to look up (default: list every area).
        path: Option<String>,
    },
    Sync {
        #[arg(long)]
        room: Option<String>,
        /// Every folder linked to a room.
        #[arg(long)]
        all: bool,
    },
    Usage {
        #[arg(long)]
        room: Option<String>,
    },
    Term {
        #[command(subcommand)]
        cmd: work::TermCmd,
    },
    Thread {
        #[command(subcommand)]
        cmd: work::ThreadCmd,
    },
    Activity {
        #[arg(long)]
        room: Option<String>,
        #[arg(long, default_value_t = 30)]
        limit: usize,
    },
    Whiteboard {
        #[command(subcommand)]
        cmd: work::WhiteboardCmd,
    },
    Plan {
        #[command(subcommand)]
        cmd: work::PlanCmd,
    },
    Task {
        #[command(subcommand)]
        cmd: work::TaskCmd,
    },
    Merge {
        #[arg(long)]
        room: Option<String>,
    },
    Memory {
        /// A file or directory (default: every note).
        path: Option<String>,
        #[arg(long)]
        room: Option<String>,
    },
    Github {
        #[command(subcommand)]
        cmd: work::GithubCmd,
    },
    Linear {
        #[command(subcommand)]
        cmd: work::LinearCmd,
    },
    Webhook {
        #[command(subcommand)]
        cmd: work::WebhookCmd,
    },
    Mirror {
        /// Directory for your mirror of the room's shared working copy.
        dir: PathBuf,
        #[arg(long)]
        room: Option<String>,
    },
    Budget {
        #[command(subcommand)]
        cmd: BudgetCmd,
    },
    Login {
        /// Sign in with this email address (you get a code by email).
        #[arg(long)]
        email: Option<String>,
        /// Sign in with GitHub instead of email.
        #[arg(long, conflicts_with = "email")]
        github: bool,
        /// Account service URL (default: the `cloud` setting).
        #[arg(long)]
        server: Option<String>,
    },
    Logout,
    Cloud {
        #[command(subcommand)]
        cmd: account::CloudCmd,
    },
    Update {
        /// Only say whether a newer release exists.
        #[arg(long)]
        check: bool,
        /// Install this version (for example 0.2.0) instead of the latest.
        #[arg(long)]
        version: Option<String>,
        /// Reinstall even when this version is already the latest.
        #[arg(long)]
        force: bool,
    },
    Uninstall {
        /// Also delete this device's Polyph data: identity, rooms and settings.
        #[arg(long)]
        purge: bool,
        /// Do not ask for confirmation.
        #[arg(long, short)]
        yes: bool,
    },
}

#[derive(Subcommand, Debug)]
enum PluginCmd {
    List,
    /// Install a plugin from a folder with a polyph-plugin.toml.
    Add {
        path: PathBuf,
        /// Replace an installed plugin with the same name.
        #[arg(long)]
        replace: bool,
    },
    Remove {
        name: String,
    },
}

#[derive(Subcommand, Debug)]
enum SkillsCmd {
    /// Skills for this repository and your own.
    List,
    Show {
        name: String,
    },
    /// Copy a skill folder into this repository's .polyph/skills.
    Add {
        path: PathBuf,
    },
}

#[derive(Subcommand, Debug)]
enum TemplateCmd {
    List,
    /// Write a template's files into this repository (existing files are kept).
    Apply {
        name: String,
    },
}

#[derive(Subcommand, Debug)]
enum HostCmd {
    /// On your computer: make a one-time enrollment for a server.
    Invite {
        #[arg(long)]
        room: Option<String>,
    },
    /// On the server: join a room as an agent host of the person who invited it.
    Join {
        code: String,
        #[arg(long)]
        member: String,
        #[arg(long)]
        relay: String,
        /// The checkout agents work in on this server.
        #[arg(long)]
        workdir: PathBuf,
        /// The name agents are labeled with (the person's own name).
        #[arg(long)]
        name: Option<String>,
    },
    /// On the server: change the directory agents work in.
    Workdir { dir: PathBuf },
}

#[derive(Subcommand, Debug)]
enum AgentCmd {
    Start {
        /// Agent name: claude, codex, gemini, opencode, local, or one from polyph.toml.
        name: String,
        #[arg(long)]
        task: Option<String>,
        /// Run on one of your agent hosts: its device id, or "cloud" for any online one.
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
    /// Your agent hosts in the room.
    Hosts {
        #[arg(long)]
        room: Option<String>,
    },
    Prompt {
        id: String,
        text: String,
    },
    /// Leave a message in an agent's inbox; it reads it at its next turn.
    Message {
        agent: String,
        text: String,
        #[arg(long)]
        room: Option<String>,
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
enum HookCmd {
    /// Before an agent edits a file: block it when another agent holds the file.
    PreEdit {
        /// The editing client (claude-code, cursor...).
        #[arg(long, default_value = "claude-code")]
        client: String,
    },
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

/// `polyph trust`: records (or with `--revoke` removes) this person's trust in
/// the current content of the repository's polyph.toml, kept in Polyph's
/// data directory. Uses the main checkout, as the daemon does. Shows the
/// settings it is about to trust and asks first, unless `yes`.
fn trust(ctx: &Ctx, revoke: bool, yes: bool) -> anyhow::Result<()> {
    use polyph_agents::project::{ProjectFile, TrustStore};
    let l = &ctx.l;
    let cwd = std::env::current_dir()?;
    let root = polyph_git::Repo::open(&cwd).ok().and_then(|r| r.main_root().ok()).unwrap_or(cwd);
    let store = TrustStore::at(ctx.paths.trusted_projects());
    let file = ProjectFile::find(&root)?;
    let shown = |p: &std::path::Path| p.display().to_string();
    if revoke {
        let dir = match &file {
            Some(f) => f.dir.clone(),
            None => polyph_git::canonical(&root)?,
        };
        let had = store.revoke(&dir)?;
        if ctx.json {
            ctx.print_json(&json!({ "dir": dir, "revoked": had }));
        } else {
            ctx.out(tr!(l, if had { "trust-revoked" } else { "trust-revoke-none" }, dir = shown(&dir)));
        }
        return Ok(());
    }
    let Some(f) = file else {
        anyhow::bail!(tr!(l, "trust-no-file", dir = shown(&root)));
    };
    let already = store.is_trusted(&f.dir, &f.sha256);
    if !already {
        let sections = f.parse()?.security_toml();
        if !ctx.json {
            if sections.is_empty() {
                ctx.out(tr!(l, "trust-review-none", file = shown(&f.path)));
            } else {
                ctx.out(tr!(l, "trust-review", file = shown(&f.path)));
                ctx.out("");
                for s in sections.lines() {
                    ctx.out(format!("    {s}"));
                }
                ctx.out("");
            }
        }
        if !yes {
            use std::io::IsTerminal;
            if ctx.json || !std::io::stdin().is_terminal() {
                anyhow::bail!(l.t("trust-needs-yes"));
            }
            if !setup::confirm(&l.t("trust-confirm"))? {
                ctx.out(l.t("trust-cancelled"));
                return Ok(());
            }
        }
        store.trust(&f)?;
    }
    if ctx.json {
        ctx.print_json(&json!({ "file": f.path, "sha256": f.sha256, "trusted": true, "already": already, "settings": f.parse()?.security_toml() }));
    } else {
        ctx.out(tr!(l, if already { "trust-already" } else { "trust-saved" }, file = shown(&f.path)));
        ctx.out(tr!(l, "trust-hash", hash = f.sha256.as_str()));
    }
    Ok(())
}

struct Ctx {
    l: Localizer,
    json: bool,
    paths: Paths,
}

/// Prints a line; a closed pipe (`polyph ... | head`) ends the program quietly.
fn line(s: &str) {
    use std::io::Write;
    let mut out = std::io::stdout().lock();
    if let Err(e) = writeln!(out, "{s}")
        && e.kind() == std::io::ErrorKind::BrokenPipe
    {
        std::process::exit(0);
    }
}

impl Ctx {
    fn out(&self, s: impl AsRef<str>) {
        line(s.as_ref());
    }

    fn print_json(&self, v: &Value) {
        line(&serde_json::to_string_pretty(v).unwrap_or_default());
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
    // Read before any other thread starts: on Unix `time` refuses to read the
    // local offset once the process has more than one thread.
    let _ = UTC_OFFSET.set(time::UtcOffset::current_local_offset().map(|o| i64::from(o.whole_seconds())).unwrap_or(0));
    // The sandbox helper re-enters this binary before anything else runs. It
    // stays on the main thread: it confines the process it is running in.
    let raw: Vec<String> = std::env::args().collect();
    if raw.get(1).map(String::as_str) == Some("__sandbox") {
        std::process::exit(polyph_sandbox::helper::main_with_args(raw[2..].to_vec()));
    }
    // Everything else runs on a thread with a 16 MiB stack. In a debug build
    // the frame of clap's derived `Cmd::augment_subcommands` alone is about
    // 800 KiB on Linux and larger on Windows, whose main thread has 1 MiB, so
    // building the parser overflowed there before any subcommand ran. The
    // poll frame of `run` (one state machine for every subcommand) is large
    // too.
    let code = std::thread::Builder::new()
        .name("polyph".into())
        .stack_size(MAIN_STACK)
        .spawn(cli_main)
        .map_err(|e| e.to_string())
        .and_then(|h| h.join().map_err(|_| "the command panicked".to_string()))
        .unwrap_or_else(|e| {
            eprintln!("{e}");
            1
        });
    std::process::exit(code);
}

/// Stack size of the thread that parses arguments and runs the command.
const MAIN_STACK: usize = 16 << 20;

fn cli_main() -> i32 {
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
                return 1;
            }
        },
    };
    let ctx = Ctx { l: Localizer::new(&lang), json: cli.json, paths };
    // The daemon's API dispatcher is a large future; 8 MiB worker stacks
    // (tokio's default is 2 MiB) leave room for it in every build profile.
    let rt = match tokio::runtime::Builder::new_multi_thread().thread_stack_size(8 << 20).enable_all().build() {
        Ok(r) => r,
        Err(e) => {
            eprintln!("{e}");
            return 1;
        }
    };
    rt.block_on(async {
        match run(&ctx, cli.cmd).await {
            Ok(()) => 0,
            Err(e) => {
                eprintln!("{e}");
                1
            }
        }
    })
}

/// Opens a URL in the person's browser.
fn open_browser(url: &str) -> std::io::Result<()> {
    #[cfg(target_os = "macos")]
    let mut c = std::process::Command::new("open");
    #[cfg(target_os = "windows")]
    let mut c = {
        let mut c = std::process::Command::new("rundll32");
        c.arg("url.dll,FileProtocolHandler");
        c
    };
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    let mut c = std::process::Command::new("xdg-open");
    let status = c
        .arg(url)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()?;
    if status.success() { Ok(()) } else { Err(std::io::Error::other("no browser")) }
}

/// Local time offset, read in `main` before any thread starts (the only time
/// it can be read soundly on Unix).
static UTC_OFFSET: std::sync::OnceLock<i64> = std::sync::OnceLock::new();

async fn tui(ctx: &Ctx) -> anyhow::Result<()> {
    let port = client::ensure_daemon(&ctx.paths, true).await.map_err(|e| anyhow::anyhow!(e.to_string()))?;
    let token = std::fs::read_to_string(ctx.paths.token())?;
    let (conn, events) = polyph_tui::conn::connect(port, &token).await?;
    let cfg = polyph_daemon::config::UserConfig::load(&ctx.paths).unwrap_or_default();
    let mode = if cfg.theme == "light" { polyph_tui::theme::Mode::Light } else { polyph_tui::theme::Mode::Dark };
    let theme = polyph_tui::theme::Theme::new(polyph_tui::theme::Depth::detect(), mode);
    let mut app = polyph_tui::App::new(Localizer::new(ctx.l.lang()), theme);
    app.utc_offset = UTC_OFFSET.get().copied().unwrap_or(0);
    polyph_tui::run(app, conn, events).await
}

async fn run(ctx: &Ctx, cmd: Option<Cmd>) -> anyhow::Result<()> {
    let Some(cmd) = cmd else {
        use std::io::IsTerminal;
        // In a terminal: the TUI. Piped or scripted: help.
        if std::io::stdout().is_terminal() && std::io::stdin().is_terminal() {
            return tui(ctx).await;
        }
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
        // Started by agents (Polyph's, or the person's own); speaks MCP on stdin/stdout.
        Cmd::Mcp { client, room } => setup::mcp(ctx, client, room).await,
        Cmd::Connect { clients, http, yes } => setup::connect(ctx, &clients, http, yes).await.map(|_| ()),
        Cmd::Disconnect { clients, yes } => setup::disconnect(ctx, &clients, yes).await,
        Cmd::Start { room, me, yes, no_connect } => setup::start(ctx, room, me, yes, no_connect).await,
        Cmd::Doctor => setup::doctor(ctx).await,
        Cmd::Trust { revoke, yes } => trust(ctx, revoke, yes),
        Cmd::Hook { cmd: HookCmd::PreEdit { client } } => {
            let code = hook::pre_edit(ctx, &client).await;
            if code != 0 {
                std::process::exit(code);
            }
            Ok(())
        }
        Cmd::Sync { room, all } => repo::sync(ctx, room, all).await,
        Cmd::Brief { cmd, room } => knowledge::brief(ctx, cmd, room).await,
        Cmd::Repo { cmd, room } => repo::repo_cmd(ctx, cmd, room).await,
        Cmd::Search { query, room } => knowledge::search(ctx, &query, room).await,
        Cmd::Catchup { room } => knowledge::catchup(ctx, room).await,
        Cmd::Areas { path } => knowledge::areas(ctx, path).await,
        Cmd::Feedback { surface, print } => feedback::run(ctx, surface, print).await,
        Cmd::Update { check, version, force } => update::run(ctx, check, version, force).await,
        Cmd::Uninstall { purge, yes } => update::uninstall(ctx, purge, yes).await,
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
        Cmd::Host { quick, lan, no_approval, name, bridge, mode } => {
            if mode != "distributed" && mode != "hosted" {
                anyhow::bail!(tr!(l, "host-bad-mode", mode = mode.as_str()));
            }
            let name = name.unwrap_or_default();
            let room = call(&mut api, "room.create", json!({ "name": name, "quick": quick || name.is_empty(), "require_approval": !no_approval, "lan": lan, "bridge": bridge })).await?;
            let rid = room["id"].as_str().unwrap_or_default().to_string();
            let shared = if mode == "hosted" {
                let dir = std::env::current_dir()?;
                Some(call(&mut api, "mirror.host", json!({ "room": rid, "dir": dir })).await?)
            } else {
                None
            };
            // A room made in a checkout belongs to it, and remembers where the code lives.
            if let Ok(repo) = polyph_git::Repo::open(&std::env::current_dir()?) {
                call(&mut api, "folder.link", json!({ "room": rid, "dir": repo.root() })).await?;
                if !room["quick"].as_bool().unwrap_or(false) {
                    repo::remember(ctx, &mut api, &rid, repo.root()).await;
                }
            }
            let inv = call(&mut api, "room.invite", json!({ "room": rid })).await?;
            if let Some(s) = &shared
                && !ctx.json
            {
                ctx.out(tr!(
                    l,
                    "hosted-sharing",
                    dir = s["root"].as_str().unwrap_or_default(),
                    files = s["files"].as_u64().unwrap_or(0)
                ));
            }
            if ctx.json {
                ctx.print_json(&json!({ "room": room, "invite": inv, "hosted": shared }));
            } else {
                if room["quick"].as_bool() == Some(true) {
                    ctx.out(l.t("quick-created"));
                } else {
                    ctx.out(tr!(l, "room-created", name = room["name"].as_str().unwrap_or_default()));
                }
                print_invite(ctx, &inv);
            }
        }
        Cmd::Join { code, me: name, relay, lan, clone, no_clone } => {
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
            if let Some(rid) = r["id"].as_str() {
                let choice = if clone {
                    Some(true)
                } else if no_clone {
                    Some(false)
                } else {
                    None
                };
                repo::after_join(ctx, &mut api, rid, choice).await?;
            }
        }
        Cmd::Web { no_open } => {
            let v = call(&mut api, "web.link", json!({})).await?;
            let port = client::daemon_port(&ctx.paths).unwrap_or(0);
            let url = format!("http://127.0.0.1:{port}/app/#code={}", v["code"].as_str().unwrap_or_default());
            if ctx.json {
                ctx.print_json(&json!({ "url": url }));
            } else if no_open || open_browser(&url).is_err() {
                ctx.out(tr!(l, "web-link", url = url.as_str()));
            } else {
                ctx.out(l.t("web-opened"));
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
        Cmd::Phone { room, base } => {
            let room = ctx.room(&mut api, room).await?;
            let v = call(&mut api, "phone.pair", json!({ "room": room, "base": base })).await?;
            if ctx.json {
                ctx.print_json(&v);
            } else {
                let link = v["link"].as_str().unwrap_or_default();
                ctx.out(l.t("phone-scan"));
                if let Some(qr) = polyph_daemon::phone::qr_text(link) {
                    ctx.out(qr);
                }
                ctx.out(tr!(l, "phone-link", url = link, code = v["code"].as_str().unwrap_or_default()));
                ctx.out(l.t("phone-approve"));
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
                    if s["connected"] != true
                        && let Ok(info) = call(&mut api, "room.get", json!({ "room": s["room"] })).await
                        && let Some(m) = info["relay_error"]["message"].as_str()
                    {
                        ctx.out(format!("  {}", tr!(l, "relay-error-last", message = m)));
                        ctx.out(format!(
                            "  {}",
                            setup::relay_fix(l, info["relay_error"]["kind"].as_str().unwrap_or_default())
                        ));
                    }
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
        Cmd::Plugin { cmd: PluginCmd::List } => {
            let v = call(&mut api, "plugin.list", json!({})).await?;
            if ctx.json {
                ctx.print_json(&v);
            } else if v.as_array().is_none_or(|a| a.is_empty()) {
                ctx.out(l.t("plugins-empty"));
            } else {
                for p in v.as_array().into_iter().flatten() {
                    let hooks: Vec<&str> =
                        p["hooks"].as_array().into_iter().flatten().filter_map(|x| x.as_str()).collect();
                    let hosts: Vec<&str> =
                        p["hosts"].as_array().into_iter().flatten().filter_map(|x| x.as_str()).collect();
                    ctx.out(tr!(
                        l,
                        "plugin-line",
                        name = p["name"].as_str().unwrap_or_default(),
                        version = p["version"].as_str().unwrap_or_default(),
                        description = p["description"].as_str().unwrap_or_default()
                    ));
                    if p["tools"] == true {
                        ctx.out(format!("    {}", l.t("plugin-gives-tools")));
                    }
                    if !hooks.is_empty() {
                        ctx.out(format!("    {}", tr!(l, "plugin-hooks", events = hooks.join(", "))));
                    }
                    if !hosts.is_empty() {
                        ctx.out(format!("    {}", tr!(l, "plugin-hosts", hosts = hosts.join(", "))));
                    }
                }
            }
        }
        Cmd::Plugin { cmd: PluginCmd::Add { path, replace } } => {
            let path = polyph_git::canonical(&path)?;
            let v = call(&mut api, "plugin.add", json!({ "path": path, "replace": replace })).await?;
            if ctx.json {
                ctx.print_json(&v);
            } else {
                ctx.out(tr!(l, "plugin-added", name = v["name"].as_str().unwrap_or_default()));
                let hosts: Vec<&str> = v["hosts"].as_array().into_iter().flatten().filter_map(|x| x.as_str()).collect();
                if !hosts.is_empty() {
                    ctx.out(tr!(l, "plugin-hosts", hosts = hosts.join(", ")));
                }
            }
        }
        Cmd::Plugin { cmd: PluginCmd::Remove { name } } => {
            call(&mut api, "plugin.remove", json!({ "name": name })).await?;
            ctx.out(l.t("plugin-removed"));
        }
        Cmd::Skills { cmd } => {
            let cwd = std::env::current_dir()?;
            match cmd {
                SkillsCmd::List => {
                    let v = call(&mut api, "skills.list", json!({ "cwd": cwd })).await?;
                    if ctx.json {
                        ctx.print_json(&v);
                    } else if v.as_array().is_none_or(|a| a.is_empty()) {
                        ctx.out(l.t("skills-empty"));
                    } else {
                        for s in v.as_array().into_iter().flatten() {
                            let source = l.t(&format!("skill-source-{}", s["source"].as_str().unwrap_or("project")));
                            ctx.out(tr!(
                                l,
                                "skill-line",
                                name = s["name"].as_str().unwrap_or_default(),
                                source = source,
                                description = s["description"].as_str().unwrap_or_default()
                            ));
                        }
                    }
                }
                SkillsCmd::Show { name } => {
                    let v = call(&mut api, "skills.show", json!({ "cwd": cwd, "name": name })).await?;
                    if ctx.json {
                        ctx.print_json(&v);
                    } else {
                        ctx.out(v["text"].as_str().unwrap_or_default());
                    }
                }
                SkillsCmd::Add { path } => {
                    let from = polyph_git::canonical(&path)?;
                    let v = call(&mut api, "skills.add", json!({ "cwd": cwd, "from": from })).await?;
                    ctx.out(tr!(l, "skill-added", name = v["name"].as_str().unwrap_or_default()));
                }
            }
        }
        Cmd::Template { cmd: TemplateCmd::List } => {
            let v = call(&mut api, "template.list", json!({})).await?;
            if ctx.json {
                ctx.print_json(&v);
            } else {
                for t in v.as_array().into_iter().flatten() {
                    let name = t["name"].as_str().unwrap_or_default();
                    // Built-in descriptions come from the locale files.
                    let description = if t["builtin"] == true {
                        l.t(&format!("template-desc-{name}"))
                    } else {
                        t["description"].as_str().unwrap_or_default().to_string()
                    };
                    ctx.out(tr!(l, "template-line", name = name, description = description));
                }
            }
        }
        Cmd::Template { cmd: TemplateCmd::Apply { name } } => {
            let dir = std::env::current_dir()?;
            let v = call(&mut api, "template.apply", json!({ "name": name, "dir": dir })).await?;
            if ctx.json {
                ctx.print_json(&v);
            } else {
                for f in v["written"].as_array().into_iter().flatten() {
                    ctx.out(tr!(l, "template-wrote", file = f.as_str().unwrap_or_default()));
                }
                for f in v["kept"].as_array().into_iter().flatten() {
                    ctx.out(tr!(l, "template-kept", file = f.as_str().unwrap_or_default()));
                }
            }
        }
        Cmd::AgentHost { cmd: HostCmd::Invite { room } } => {
            let room = ctx.room(&mut api, room).await?;
            let v = call(&mut api, "host.invite", json!({ "room": room })).await?;
            if ctx.json {
                ctx.print_json(&v);
            } else {
                ctx.out(l.t("host-invite"));
                ctx.out(format!("    {}", v["command"].as_str().unwrap_or_default()));
                ctx.out(l.t("host-invite-approve"));
            }
        }
        Cmd::AgentHost { cmd: HostCmd::Join { code, member, relay, workdir, name } } => {
            if !ctx.json {
                ctx.out(l.t("join-waiting"));
            }
            let v = call(
                &mut api,
                "host.join",
                json!({ "code": code, "member": member, "relay": relay, "workdir": workdir, "name": name }),
            )
            .await?;
            if ctx.json {
                ctx.print_json(&v);
            } else {
                ctx.out(tr!(l, "host-joined", name = v["name"].as_str().unwrap_or_default()));
            }
        }
        Cmd::AgentHost { cmd: HostCmd::Workdir { dir } } => {
            let dir = polyph_git::canonical(&dir)?;
            call(&mut api, "config.set", json!({ "key": "host_workdir", "value": dir })).await?;
            ctx.out(l.t("config-set"));
        }
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
        Cmd::Term { cmd } => work::term(ctx, &mut api, cmd).await?,
        Cmd::Thread { cmd } => work::thread(ctx, &mut api, cmd).await?,
        Cmd::Activity { room, limit } => work::activity(ctx, &mut api, room, limit).await?,
        Cmd::Whiteboard { cmd } => work::whiteboard(ctx, &mut api, cmd).await?,
        Cmd::Plan { cmd } => work::plan(ctx, &mut api, cmd).await?,
        Cmd::Task { cmd } => work::task(ctx, &mut api, cmd).await?,
        Cmd::Merge { room } => work::merge(ctx, &mut api, room).await?,
        Cmd::Memory { path, room } => work::memory(ctx, &mut api, path, room).await?,
        Cmd::Github { cmd } => work::github(ctx, &mut api, cmd).await?,
        Cmd::Linear { cmd } => work::linear(ctx, &mut api, cmd).await?,
        Cmd::Login { email, github, server } => account::login(ctx, &mut api, email, github, server).await?,
        Cmd::Logout => account::logout(ctx, &mut api).await?,
        Cmd::Cloud { cmd } => account::cloud(ctx, &mut api, cmd).await?,
        Cmd::Webhook { cmd } => work::webhook(ctx, &mut api, cmd).await?,
        Cmd::Mirror { dir, room } => {
            let room = ctx.room(&mut api, room).await?;
            let v = call(&mut api, "mirror.open", json!({ "room": room, "dir": dir })).await?;
            if ctx.json {
                ctx.print_json(&v);
            } else {
                ctx.out(tr!(l, "mirror-opened", dir = v["root"].as_str().unwrap_or_default()));
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
        Cmd::Daemon { .. }
        | Cmd::Relay { .. }
        | Cmd::Completions { .. }
        | Cmd::Mcp { .. }
        | Cmd::Connect { .. }
        | Cmd::Disconnect { .. }
        | Cmd::Start { .. }
        | Cmd::Doctor
        | Cmd::Trust { .. }
        | Cmd::Feedback { .. }
        | Cmd::Hook { .. }
        | Cmd::Brief { .. }
        | Cmd::Repo { .. }
        | Cmd::Search { .. }
        | Cmd::Catchup { .. }
        | Cmd::Areas { .. }
        | Cmd::Sync { .. }
        | Cmd::Update { .. }
        | Cmd::Uninstall { .. } => {}
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
        AgentCmd::Start { name, task, on: Some(on), preset, prompt, room } => {
            let room = ctx.room(api, room).await?;
            let v = call(
                api,
                "agent.start",
                json!({ "room": room, "kind": name, "on": on, "preset": preset, "task": task, "prompt": prompt }),
            )
            .await?;
            if ctx.json {
                ctx.print_json(&v);
            } else {
                let host: String = v["host"].as_str().unwrap_or_default().chars().take(8).collect();
                ctx.out(tr!(l, "agent-started-on", label = v["label"].as_str().unwrap_or_default(), host = host));
            }
        }
        AgentCmd::Hosts { room } => {
            let room = ctx.room(api, room).await?;
            let v = call(api, "agent.hosts", json!({ "room": room })).await?;
            if ctx.json {
                ctx.print_json(&v);
            } else if v.as_array().is_none_or(|a| a.is_empty()) {
                ctx.out(l.t("hosts-empty"));
            } else {
                for x in v.as_array().into_iter().flatten() {
                    let dev: String = x["device"].as_str().unwrap_or_default().chars().take(8).collect();
                    let state = if x["online"] == true { l.t("host-online") } else { l.t("host-offline") };
                    ctx.out(tr!(
                        l,
                        "host-line",
                        device = dev,
                        state = state,
                        agents = x["agents"].as_u64().unwrap_or(0)
                    ));
                }
            }
        }
        AgentCmd::Start { name, task, on: None, preset, prompt, room } => {
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
                for key in v["warnings"].as_array().into_iter().flatten().filter_map(|k| k.as_str()) {
                    eprintln!("{}", tr!(l, key, sandbox = v["sandbox"].as_str().unwrap_or_default()));
                }
                if let Some(u) = v["untrusted"].as_object() {
                    let ignored: Vec<&str> =
                        u["ignored"].as_array().into_iter().flatten().filter_map(Value::as_str).collect();
                    eprintln!(
                        "{}",
                        tr!(
                            l,
                            "agent-untrusted-project",
                            ignored = ignored.join(", "),
                            file = u["file"].as_str().unwrap_or_default(),
                            dir = u["dir"].as_str().unwrap_or_default()
                        )
                    );
                }
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
        AgentCmd::Message { agent, text, room } => {
            let room = ctx.room(api, room).await?;
            let v = call(api, "inbox.send", json!({ "room": room, "agent": agent, "text": text })).await?;
            if ctx.json {
                ctx.print_json(&v);
            } else {
                ctx.out(tr!(l, "agent-messaged", label = agent));
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
    if let Some(relay) = inv["relay"].as_str() {
        ctx.out(tr!(l, "invite-link", link = polyph_daemon::phone::join_link(code, relay)));
    }
    ctx.out(tr!(l, "invite-expires", when = ctx.when(inv["expires_at"].as_i64().unwrap_or(0))));
    ctx.out(if inv["reusable"] == true { l.t("invite-reusable") } else { l.t("invite-one-time") });
    if inv["pending"] == true {
        ctx.out(l.t("invite-pending"));
    }
    if let Some(lan) = inv["lan"].as_str() {
        ctx.out(tr!(l, "invite-lan", code = code, url = lan.split('#').next().unwrap_or(lan)));
    }
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
                    // Another device joining as this person says so plainly.
                    let line = if k["own_device"] == true { "knock-line-own-device" } else { "knock-line" };
                    ctx.out(tr!(l, line, id = id.clone(), name = k["display_name"].as_str().unwrap_or_default()));
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
