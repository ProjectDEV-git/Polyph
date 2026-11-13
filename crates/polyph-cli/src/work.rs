//! CLI commands for teamwork features: terminals, threads, activity,
//! whiteboards, planning, tasks and the merge queue, memory notes, and
//! integrations (GitHub, Linear, webhooks). Every human-readable line comes
//! from the locale files.

use std::path::PathBuf;

use clap::Subcommand;
use polyph_i18n::tr;
use serde_json::{Value, json};

use super::{Api, Ctx, call};

#[derive(Subcommand, Debug)]
pub enum TermCmd {
    Open {
        #[arg(long)]
        title: Option<String>,
        #[arg(long)]
        room: Option<String>,
        /// Program and arguments (default: your shell).
        #[arg(last = true)]
        command: Vec<String>,
    },
    List {
        #[arg(long)]
        room: Option<String>,
    },
    Read {
        id: String,
        #[arg(long)]
        room: Option<String>,
    },
    Send {
        id: String,
        text: String,
        /// Do not press Enter after the text.
        #[arg(long)]
        no_enter: bool,
        #[arg(long)]
        room: Option<String>,
    },
    Ask {
        id: String,
        #[arg(long)]
        room: Option<String>,
    },
    Release {
        id: String,
        #[arg(long)]
        room: Option<String>,
    },
    Revoke {
        id: String,
        #[arg(long)]
        room: Option<String>,
    },
    Close {
        id: String,
        #[arg(long)]
        room: Option<String>,
    },
}

#[derive(Subcommand, Debug)]
pub enum ThreadCmd {
    New {
        title: String,
        /// First post.
        #[arg(long)]
        text: Option<String>,
        /// Card the thread is about.
        #[arg(long)]
        card: Option<String>,
        #[arg(long)]
        room: Option<String>,
    },
    List {
        #[arg(long)]
        room: Option<String>,
    },
    Show {
        id: String,
        #[arg(long)]
        room: Option<String>,
    },
    Reply {
        id: String,
        text: String,
        #[arg(long)]
        room: Option<String>,
    },
}

#[derive(Subcommand, Debug)]
pub enum WhiteboardCmd {
    List {
        #[arg(long)]
        room: Option<String>,
    },
    New {
        name: String,
        #[arg(long)]
        room: Option<String>,
    },
}

#[derive(Subcommand, Debug)]
pub enum PlanCmd {
    New {
        goal: String,
        /// Planner agent (default: [planner] agent in polyph.toml).
        #[arg(long)]
        agent: Option<String>,
        #[arg(long)]
        room: Option<String>,
    },
    Accept {
        plan: String,
        #[arg(long)]
        room: Option<String>,
    },
}

#[derive(Subcommand, Debug)]
pub enum TaskCmd {
    Finish {
        /// The agent working on the task.
        agent: String,
        /// Commit message (default: the card's title).
        #[arg(long)]
        message: Option<String>,
    },
    Handoff {
        agent: String,
        /// Start this agent kind on the task right away.
        #[arg(long)]
        to: Option<String>,
        #[arg(long)]
        note: Option<String>,
    },
}

#[derive(Subcommand, Debug)]
pub enum GithubCmd {
    Connect {
        /// GitHub Enterprise API URL.
        #[arg(long)]
        api_url: Option<String>,
    },
    Disconnect,
    Import {
        /// owner/name (default: from the repository here).
        #[arg(long)]
        repo: Option<String>,
        #[arg(long)]
        label: Vec<String>,
        #[arg(long)]
        room: Option<String>,
    },
    Sync {
        #[arg(long)]
        room: Option<String>,
    },
}

#[derive(Subcommand, Debug)]
pub enum LinearCmd {
    Connect {
        #[arg(long)]
        api_url: Option<String>,
    },
    Disconnect,
    Import {
        /// Team key, e.g. ENG.
        team: String,
        #[arg(long)]
        room: Option<String>,
    },
}

#[derive(Subcommand, Debug)]
pub enum WebhookCmd {
    Add {
        url: String,
        /// json, slack or discord.
        #[arg(long, default_value = "json")]
        format: String,
        /// Only these events (repeat): approval, merged, merge_failed, changes_requested, mention, overlap, budget, loop_guard.
        #[arg(long)]
        event: Vec<String>,
        /// Sign deliveries with this secret (JSON format).
        #[arg(long)]
        secret: Option<String>,
    },
    List,
    Test {
        id: String,
    },
    Remove {
        id: String,
    },
}

/// How long ago, localized.
fn ago(ctx: &Ctx, at: i64) -> String {
    let secs = (polyph_proto::now() - at).max(0);
    if secs < 60 {
        ctx.l.t("ago-now")
    } else if secs < 3600 {
        tr!(ctx.l, "ago-minutes", n = secs / 60)
    } else if secs < 86_400 {
        tr!(ctx.l, "ago-hours", n = secs / 3600)
    } else {
        tr!(ctx.l, "ago-days", n = secs / 86_400)
    }
}

fn short(id: &str) -> String {
    id.chars().take(8).collect()
}

/// Reads a secret from stdin, so it stays out of shell history.
fn read_token(ctx: &Ctx) -> anyhow::Result<String> {
    eprintln!("{}", ctx.l.t("token-prompt"));
    let mut s = String::new();
    std::io::stdin().read_line(&mut s)?;
    let s = s.trim().to_string();
    if s.is_empty() {
        anyhow::bail!(ctx.l.t("token-empty"));
    }
    Ok(s)
}

fn done(ctx: &Ctx, v: &Value) {
    if ctx.json {
        ctx.print_json(v);
    } else {
        ctx.out(ctx.l.t("done"));
    }
}

pub async fn term(ctx: &Ctx, api: &mut Api, cmd: TermCmd) -> anyhow::Result<()> {
    let l = &ctx.l;
    match cmd {
        TermCmd::Open { title, room, command } => {
            let room = ctx.room(api, room).await?;
            let cwd = std::env::current_dir()?;
            let v =
                call(api, "term.open", json!({ "room": room, "cwd": cwd, "command": command, "title": title })).await?;
            if ctx.json {
                ctx.print_json(&v);
            } else {
                ctx.out(tr!(
                    l,
                    "term-opened",
                    id = short(v["id"].as_str().unwrap_or_default()),
                    title = v["terminal"]["title"].as_str().unwrap_or_default()
                ));
            }
        }
        TermCmd::List { room } => {
            let room = ctx.room(api, room).await?;
            let v = call(api, "term.list", json!({ "room": room })).await?;
            if ctx.json {
                ctx.print_json(&v);
                return Ok(());
            }
            let members = call(api, "room.members", json!({ "room": room })).await.unwrap_or(Value::Null);
            let name = |m: &Value| {
                members
                    .as_array()
                    .and_then(|a| a.iter().find(|x| x["member"] == *m))
                    .and_then(|x| x["name"].as_str())
                    .unwrap_or_default()
                    .to_string()
            };
            let list = v.as_array().cloned().unwrap_or_default();
            if list.is_empty() {
                ctx.out(l.t("nothing-yet"));
            }
            for t in list {
                let status = if t["closed"] == true {
                    tr!(l, "term-status-closed", code = t["exit"].as_i64().map(|c| c.to_string()).unwrap_or_default())
                } else if !t["writer"].is_null() {
                    tr!(l, "term-status-writer", name = name(&t["writer"]))
                } else {
                    l.t("term-status-open")
                };
                ctx.out(tr!(
                    l,
                    "term-line",
                    id = short(t["id"].as_str().unwrap_or_default()),
                    title = t["title"].as_str().unwrap_or_default(),
                    owner = name(&t["owner"]),
                    status = status
                ));
            }
        }
        TermCmd::Read { id, room } => {
            let room = ctx.room(api, room).await?;
            let v = call(api, "term.read", json!({ "room": room, "term": id })).await?;
            if ctx.json {
                ctx.print_json(&v);
            } else {
                // Someone else's terminal output: never let its control
                // sequences reach this terminal (title, clipboard, modes).
                print!("{}", plain_text(v["text"].as_str().unwrap_or_default()));
            }
        }
        TermCmd::Send { id, text, no_enter, room } => {
            let room = ctx.room(api, room).await?;
            let text = if no_enter { text } else { format!("{text}\n") };
            let v = call(api, "term.input", json!({ "room": room, "term": id, "text": text })).await?;
            done(ctx, &v);
        }
        TermCmd::Ask { id, room } => {
            let room = ctx.room(api, room).await?;
            let v = call(api, "term.ask", json!({ "room": room, "term": id })).await?;
            if ctx.json {
                ctx.print_json(&v);
            } else {
                ctx.out(l.t("term-asked"));
            }
        }
        TermCmd::Release { id, room } => term_simple(ctx, api, "term.release", id, room).await?,
        TermCmd::Revoke { id, room } => term_simple(ctx, api, "term.revoke", id, room).await?,
        TermCmd::Close { id, room } => term_simple(ctx, api, "term.close", id, room).await?,
    }
    Ok(())
}

async fn term_simple(ctx: &Ctx, api: &mut Api, method: &str, id: String, room: Option<String>) -> anyhow::Result<()> {
    let room = ctx.room(api, room).await?;
    let v = call(api, method, json!({ "room": room, "term": id })).await?;
    done(ctx, &v);
    Ok(())
}

pub async fn thread(ctx: &Ctx, api: &mut Api, cmd: ThreadCmd) -> anyhow::Result<()> {
    let l = &ctx.l;
    match cmd {
        ThreadCmd::New { title, text, card, room } => {
            let room = ctx.room(api, room).await?;
            let v =
                call(api, "thread.create", json!({ "room": room, "title": title, "text": text, "card": card })).await?;
            if ctx.json {
                ctx.print_json(&v);
            } else {
                ctx.out(tr!(l, "thread-created", id = short(v["id"].as_str().unwrap_or_default())));
            }
        }
        ThreadCmd::List { room } => {
            let room = ctx.room(api, room).await?;
            let v = call(api, "thread.list", json!({ "room": room })).await?;
            if ctx.json {
                ctx.print_json(&v);
                return Ok(());
            }
            let list = v.as_array().cloned().unwrap_or_default();
            if list.is_empty() {
                ctx.out(l.t("nothing-yet"));
            }
            for t in list {
                ctx.out(tr!(
                    l,
                    "thread-line",
                    id = short(t["thread"]["id"].as_str().unwrap_or_default()),
                    title = t["thread"]["title"].as_str().unwrap_or_default(),
                    posts = t["posts"].as_u64().unwrap_or(0)
                ));
            }
        }
        ThreadCmd::Show { id, room } => {
            let room = ctx.room(api, room).await?;
            let v = call(api, "thread.get", json!({ "room": room, "thread": id })).await?;
            if ctx.json {
                ctx.print_json(&v);
                return Ok(());
            }
            ctx.out(v["thread"]["title"].as_str().unwrap_or_default());
            for p in v["posts"].as_array().into_iter().flatten() {
                let who = p["agent"].as_str().or(p["name"].as_str()).unwrap_or_default();
                ctx.out(tr!(l, "post-line", name = who, text = p["text"].as_str().unwrap_or_default()));
            }
        }
        ThreadCmd::Reply { id, text, room } => {
            let room = ctx.room(api, room).await?;
            let full = call(api, "thread.get", json!({ "room": room, "thread": id })).await?;
            let tid = full["thread"]["id"].as_str().unwrap_or_default().to_string();
            let v = call(api, "chat.send", json!({ "room": room, "text": text, "thread": tid })).await?;
            done(ctx, &v);
        }
    }
    Ok(())
}

pub async fn activity(ctx: &Ctx, api: &mut Api, room: Option<String>, limit: usize) -> anyhow::Result<()> {
    let l = &ctx.l;
    let room = ctx.room(api, room).await?;
    let v = call(api, "activity.list", json!({ "room": room, "limit": limit })).await?;
    if ctx.json {
        ctx.print_json(&v);
        return Ok(());
    }
    let members = call(api, "room.members", json!({ "room": room })).await.unwrap_or(Value::Null);
    let list = v.as_array().cloned().unwrap_or_default();
    if list.is_empty() {
        ctx.out(l.t("nothing-yet"));
    }
    for e in list {
        let who = e["agent"].as_str().map(str::to_string).unwrap_or_else(|| {
            members
                .as_array()
                .and_then(|a| a.iter().find(|x| x["member"] == e["actor"]))
                .and_then(|x| x["name"].as_str())
                .unwrap_or_default()
                .to_string()
        });
        let text = polyph_daemon::notify::render(l, e["summary_key"].as_str().unwrap_or_default(), &e["args"]);
        ctx.out(tr!(l, "activity-line", when = ago(ctx, e["at"].as_i64().unwrap_or(0)), who = who, text = text));
    }
    Ok(())
}

pub async fn whiteboard(ctx: &Ctx, api: &mut Api, cmd: WhiteboardCmd) -> anyhow::Result<()> {
    let l = &ctx.l;
    match cmd {
        WhiteboardCmd::List { room } => {
            let room = ctx.room(api, room).await?;
            let v = call(api, "whiteboard.list", json!({ "room": room })).await?;
            if ctx.json {
                ctx.print_json(&v);
                return Ok(());
            }
            let list = v.as_array().cloned().unwrap_or_default();
            if list.is_empty() {
                ctx.out(l.t("nothing-yet"));
            }
            for w in list {
                ctx.out(tr!(
                    l,
                    "whiteboard-line",
                    id = short(w["id"].as_str().unwrap_or_default()),
                    name = w["name"].as_str().unwrap_or_default()
                ));
            }
        }
        WhiteboardCmd::New { name, room } => {
            let room = ctx.room(api, room).await?;
            let v = call(api, "whiteboard.create", json!({ "room": room, "name": name })).await?;
            if ctx.json {
                ctx.print_json(&v);
            } else {
                ctx.out(tr!(l, "whiteboard-created", name = v["name"].as_str().unwrap_or_default()));
            }
        }
    }
    Ok(())
}

pub async fn plan(ctx: &Ctx, api: &mut Api, cmd: PlanCmd) -> anyhow::Result<()> {
    let l = &ctx.l;
    match cmd {
        PlanCmd::New { goal, agent, room } => {
            let room = ctx.room(api, room).await?;
            let cwd = std::env::current_dir()?;
            if !ctx.json {
                ctx.out(l.t("plan-working"));
            }
            let v = call(api, "plan.create", json!({ "room": room, "goal": goal, "cwd": cwd, "kind": agent })).await?;
            if ctx.json {
                ctx.print_json(&v);
                return Ok(());
            }
            let id = short(v["plan"].as_str().unwrap_or_default());
            for c in v["cards"].as_array().into_iter().flatten() {
                ctx.out(tr!(l, "plan-task", title = c["title"].as_str().unwrap_or_default()));
            }
            let title = |id: &Value| {
                v["cards"]
                    .as_array()
                    .and_then(|a| a.iter().find(|c| c["id"] == *id))
                    .and_then(|c| c["title"].as_str())
                    .unwrap_or_default()
                    .to_string()
            };
            for o in v["overlaps"].as_array().into_iter().flatten() {
                let files: Vec<&str> = o["files"].as_array().into_iter().flatten().filter_map(|f| f.as_str()).collect();
                ctx.out(tr!(l, "plan-overlap", a = title(&o["a"]), b = title(&o["b"]), files = files.join(", ")));
            }
            ctx.out(tr!(l, "plan-created", count = v["cards"].as_array().map(|a| a.len()).unwrap_or(0), id = id));
        }
        PlanCmd::Accept { plan, room } => {
            let room = ctx.room(api, room).await?;
            let v = call(api, "plan.accept", json!({ "room": room, "plan": plan })).await?;
            if ctx.json {
                ctx.print_json(&v);
            } else {
                ctx.out(tr!(
                    l,
                    "plan-accepted",
                    count = v["accepted"].as_u64().unwrap_or(0),
                    ready = v["ready"].as_u64().unwrap_or(0)
                ));
            }
        }
    }
    Ok(())
}

pub async fn task(ctx: &Ctx, api: &mut Api, cmd: TaskCmd) -> anyhow::Result<()> {
    let l = &ctx.l;
    match cmd {
        TaskCmd::Finish { agent, message } => {
            let v = call(api, "task.finish", json!({ "agent": agent, "message": message })).await?;
            if ctx.json {
                ctx.print_json(&v);
            } else {
                ctx.out(tr!(
                    l,
                    "task-queued",
                    commits = v["commits"].as_u64().unwrap_or(0),
                    branch = v["branch"].as_str().unwrap_or_default(),
                    into = v["into"].as_str().unwrap_or_default()
                ));
            }
        }
        TaskCmd::Handoff { agent, to, note } => {
            let v = call(api, "agent.handoff", json!({ "agent": agent, "to_kind": to, "note": note })).await?;
            if ctx.json {
                ctx.print_json(&v);
            } else if let Some(label) = v["next"]["label"].as_str() {
                ctx.out(tr!(l, "handoff-to", label = label));
            } else {
                ctx.out(l.t("handoff-left"));
            }
        }
    }
    Ok(())
}

pub async fn merge(ctx: &Ctx, api: &mut Api, room: Option<String>) -> anyhow::Result<()> {
    let l = &ctx.l;
    let room = ctx.room(api, room).await?;
    let v = call(api, "merge.queue", json!({ "room": room })).await?;
    if ctx.json {
        ctx.print_json(&v);
        return Ok(());
    }
    let list = v.as_array().cloned().unwrap_or_default();
    if list.is_empty() {
        ctx.out(l.t("nothing-yet"));
    }
    for e in list {
        let state = l.t(&format!("merge-state-{}", e["state"].as_str().unwrap_or_default().replace('_', "-")));
        ctx.out(tr!(
            l,
            "merge-line",
            branch = e["branch"].as_str().unwrap_or_default(),
            into = e["into"].as_str().unwrap_or_default(),
            state = state
        ));
        if let Some(d) = e["detail"].as_str().filter(|d| !d.is_empty()) {
            ctx.out(format!("  {d}"));
        }
    }
    Ok(())
}

pub async fn memory(ctx: &Ctx, api: &mut Api, path: Option<String>, room: Option<String>) -> anyhow::Result<()> {
    let l = &ctx.l;
    let room = ctx.room(api, room).await?;
    let v = call(api, "memory.list", json!({ "room": room, "path": path.unwrap_or_default() })).await?;
    if ctx.json {
        ctx.print_json(&v);
        return Ok(());
    }
    let list = v.as_array().cloned().unwrap_or_default();
    if list.is_empty() {
        ctx.out(l.t("nothing-yet"));
    }
    for n in list {
        let at = match n["line"].as_u64() {
            Some(line) => format!("{}:{line}", n["path"].as_str().unwrap_or_default()),
            None => n["path"].as_str().unwrap_or_default().to_string(),
        };
        let by = n["agent"].as_str().unwrap_or_default();
        ctx.out(tr!(l, "memory-line", at = at, text = n["text"].as_str().unwrap_or_default(), by = by));
    }
    Ok(())
}

pub async fn github(ctx: &Ctx, api: &mut Api, cmd: GithubCmd) -> anyhow::Result<()> {
    let l = &ctx.l;
    match cmd {
        GithubCmd::Connect { api_url } => {
            let token = read_token(ctx)?;
            let v = call(api, "github.connect", json!({ "token": token, "api_url": api_url })).await?;
            if ctx.json {
                ctx.print_json(&v);
            } else {
                ctx.out(tr!(l, "github-connected", login = v["login"].as_str().unwrap_or_default()));
            }
        }
        GithubCmd::Disconnect => {
            let v = call(api, "github.disconnect", json!({})).await?;
            done(ctx, &v);
        }
        GithubCmd::Import { repo, label, room } => {
            let room = ctx.room(api, room).await?;
            let cwd: PathBuf = std::env::current_dir()?;
            let v =
                call(api, "github.import", json!({ "room": room, "cwd": cwd, "repo": repo, "labels": label })).await?;
            if ctx.json {
                ctx.print_json(&v);
            } else {
                ctx.out(tr!(l, "imported", count = v["added"].as_array().map(|a| a.len()).unwrap_or(0)));
            }
        }
        GithubCmd::Sync { room } => {
            let room = ctx.room(api, room).await?;
            let v = call(api, "github.sync", json!({ "room": room })).await?;
            if ctx.json {
                ctx.print_json(&v);
            } else {
                ctx.out(tr!(l, "github-synced", count = v["changed"].as_u64().unwrap_or(0)));
            }
        }
    }
    Ok(())
}

pub async fn linear(ctx: &Ctx, api: &mut Api, cmd: LinearCmd) -> anyhow::Result<()> {
    let l = &ctx.l;
    match cmd {
        LinearCmd::Connect { api_url } => {
            let token = read_token(ctx)?;
            let v = call(api, "linear.connect", json!({ "token": token, "api_url": api_url })).await?;
            if ctx.json {
                ctx.print_json(&v);
            } else {
                ctx.out(tr!(l, "linear-connected", name = v["login"].as_str().unwrap_or_default()));
            }
        }
        LinearCmd::Disconnect => {
            let v = call(api, "linear.disconnect", json!({})).await?;
            done(ctx, &v);
        }
        LinearCmd::Import { team, room } => {
            let room = ctx.room(api, room).await?;
            let v = call(api, "linear.import", json!({ "room": room, "team": team })).await?;
            if ctx.json {
                ctx.print_json(&v);
            } else {
                ctx.out(tr!(l, "imported", count = v["added"].as_array().map(|a| a.len()).unwrap_or(0)));
            }
        }
    }
    Ok(())
}

pub async fn webhook(ctx: &Ctx, api: &mut Api, cmd: WebhookCmd) -> anyhow::Result<()> {
    let l = &ctx.l;
    match cmd {
        WebhookCmd::Add { url, format, event, secret } => {
            let v =
                call(api, "webhook.add", json!({ "url": url, "format": format, "events": event, "secret": secret }))
                    .await?;
            if ctx.json {
                ctx.print_json(&v);
            } else {
                ctx.out(tr!(l, "webhook-added", id = v["id"].as_str().unwrap_or_default()));
            }
        }
        WebhookCmd::List => {
            let v = call(api, "webhook.list", json!({})).await?;
            if ctx.json {
                ctx.print_json(&v);
                return Ok(());
            }
            let list = v.as_array().cloned().unwrap_or_default();
            if list.is_empty() {
                ctx.out(l.t("nothing-yet"));
            }
            for w in list {
                ctx.out(tr!(
                    l,
                    "webhook-line",
                    id = w["id"].as_str().unwrap_or_default(),
                    format = w["format"].as_str().unwrap_or_default(),
                    url = w["url"].as_str().unwrap_or_default()
                ));
            }
        }
        WebhookCmd::Test { id } => {
            let v = call(api, "webhook.test", json!({ "id": id })).await?;
            if ctx.json {
                ctx.print_json(&v);
            } else {
                ctx.out(l.t("webhook-tested"));
            }
        }
        WebhookCmd::Remove { id } => {
            let v = call(api, "webhook.remove", json!({ "id": id })).await?;
            done(ctx, &v);
        }
    }
    Ok(())
}

/// Terminal output as plain text: drops escape sequences (CSI, OSC, DCS and
/// the rest) and control characters other than newline and tab, and turns
/// CRLF into LF.
pub(crate) fn plain_text(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    let mut chars = raw.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\u{1b}' => match chars.next() {
                // CSI: parameters, then one final byte in @..~.
                Some('[') => {
                    for c in chars.by_ref() {
                        if ('@'..='~').contains(&c) {
                            break;
                        }
                    }
                }
                // OSC, DCS, SOS, PM, APC: until BEL or ESC \.
                Some(']' | 'P' | 'X' | '^' | '_') => {
                    while let Some(c) = chars.next() {
                        if c == '\u{7}' || (c == '\u{1b}' && chars.next_if_eq(&'\\').is_some()) {
                            break;
                        }
                    }
                }
                // Two-byte sequences with an intermediate, such as ESC ( B.
                Some('(' | ')' | '*' | '+' | '#' | '%') => {
                    chars.next();
                }
                _ => {}
            },
            '\r' if chars.peek() == Some(&'\n') => {}
            '\n' | '\t' => out.push(c),
            c if c.is_control() => {}
            c => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod plain_text_tests {
    use super::plain_text;

    #[test]
    fn strips_escape_sequences_and_controls() {
        let raw = "\u{1b}[?2004hbash$ echo hi\r\n\u{1b}[?2004lhi\r\n\u{1b}[1;32mok\u{1b}[0m\u{7}";
        assert_eq!(plain_text(raw), "bash$ echo hi\nhi\nok");
    }

    #[test]
    fn drops_titles_and_clipboard_writes() {
        let raw = "a\u{1b}]0;evil title\u{7}b\u{1b}]52;c;aGk=\u{1b}\\c\u{1b}(Bd";
        assert_eq!(plain_text(raw), "abcd");
    }

    #[test]
    fn keeps_text_tabs_and_unicode() {
        assert_eq!(plain_text("สวัสดี\t👋\n"), "สวัสดี\t👋\n");
    }
}
