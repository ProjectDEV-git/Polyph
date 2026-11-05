//! What a room knows: `polyph brief`, `polyph search`, `polyph catchup` and
//! `polyph areas`. Texts are in `locales/*/setup.ftl`.

use std::path::PathBuf;

use clap::Subcommand;
use polyph_i18n::tr;
use serde_json::{Value, json};

use crate::{Ctx, call};

#[derive(Subcommand, Debug)]
pub enum BriefCmd {
    /// Print the brief.
    Show,
    /// Replace the brief with a file's contents (`-` reads standard input).
    Set { file: PathBuf },
    /// Edit the brief in your editor.
    Edit,
}

/// The room for this folder, or the one named, or the current one.
async fn room(ctx: &Ctx, api: &mut crate::client::Api, room: Option<String>) -> anyhow::Result<String> {
    if let Some(r) = room {
        return Ok(r);
    }
    let linked = call(api, "folder.find", json!({ "dir": std::env::current_dir()? })).await?;
    match linked["room"].as_str() {
        Some(r) => Ok(r.to_string()),
        None => ctx.room(api, None).await,
    }
}

pub async fn brief(ctx: &Ctx, cmd: Option<BriefCmd>, room_arg: Option<String>) -> anyhow::Result<()> {
    let l = &ctx.l;
    let mut api = ctx.api().await?;
    let room = room(ctx, &mut api, room_arg).await?;
    let current = call(&mut api, "brief.get", json!({ "room": room })).await?;
    match cmd.unwrap_or(BriefCmd::Show) {
        BriefCmd::Show => {
            if ctx.json {
                ctx.print_json(&current);
            } else if let Some(t) = current["text"].as_str() {
                ctx.out(t);
                ctx.out(String::new());
                ctx.out(tr!(l, "brief-by", name = current["by"].as_str().unwrap_or_default()));
            } else {
                ctx.out(l.t("brief-empty"));
            }
        }
        BriefCmd::Set { file } => {
            let text = if file.as_os_str() == "-" {
                let mut s = String::new();
                std::io::Read::read_to_string(&mut std::io::stdin(), &mut s)?;
                s
            } else {
                std::fs::read_to_string(&file)?
            };
            call(&mut api, "brief.set", json!({ "room": room, "text": text })).await?;
            ctx.out(l.t("brief-saved"));
        }
        BriefCmd::Edit => {
            let tmp = tempfile::Builder::new().prefix("polyph-brief-").suffix(".md").tempfile()?;
            let start = current["text"].as_str().map(str::to_string).unwrap_or_else(|| l.t("brief-template"));
            std::fs::write(tmp.path(), &start)?;
            let editor = std::env::var("VISUAL")
                .or_else(|_| std::env::var("EDITOR"))
                .unwrap_or_else(|_| if cfg!(windows) { "notepad".into() } else { "vi".into() });
            let ok = std::process::Command::new(editor).arg(tmp.path()).status().map(|s| s.success()).unwrap_or(false);
            if !ok {
                ctx.out(tr!(l, "editor-failed", path = tmp.path().display().to_string()));
                return Ok(());
            }
            let text = std::fs::read_to_string(tmp.path())?;
            if text.trim() == start.trim() {
                ctx.out(l.t("brief-unchanged"));
                return Ok(());
            }
            call(&mut api, "brief.set", json!({ "room": room, "text": text })).await?;
            ctx.out(l.t("brief-saved"));
        }
    }
    Ok(())
}

pub async fn search(ctx: &Ctx, query: &[String], room_arg: Option<String>) -> anyhow::Result<()> {
    let l = &ctx.l;
    let mut api = ctx.api().await?;
    let room = room(ctx, &mut api, room_arg).await?;
    let q = query.join(" ");
    let hits = call(&mut api, "search", json!({ "room": room, "query": q })).await?;
    if ctx.json {
        ctx.print_json(&hits);
        return Ok(());
    }
    let list = hits.as_array().cloned().unwrap_or_default();
    if list.is_empty() {
        ctx.out(tr!(l, "search-none", query = q.as_str()));
    }
    for h in list {
        let kind = l.t(&format!("search-kind-{}", h["kind"].as_str().unwrap_or("note")));
        let id: String = h["id"].as_str().unwrap_or_default().chars().take(8).collect();
        ctx.out(format!("{kind} [{id}] {}", h["title"].as_str().unwrap_or_default()));
        ctx.out(format!("    {}", h["snippet"].as_str().unwrap_or_default()));
    }
    Ok(())
}

pub async fn catchup(ctx: &Ctx, room_arg: Option<String>) -> anyhow::Result<()> {
    let l = &ctx.l;
    let mut api = ctx.api().await?;
    let room = room(ctx, &mut api, room_arg).await?;
    let recent = call(&mut api, "recent", json!({ "room": room })).await?;
    let brief = call(&mut api, "brief.get", json!({ "room": room })).await?;
    if ctx.json {
        ctx.print_json(&json!({ "recent": recent, "brief": brief }));
        return Ok(());
    }
    if let Some(b) = brief["text"].as_str() {
        let first: String = b.lines().take(6).collect::<Vec<_>>().join("\n");
        ctx.out(l.t("catchup-brief"));
        ctx.out(first);
        ctx.out(String::new());
    }
    let list = recent.as_array().cloned().unwrap_or_default();
    if list.is_empty() {
        ctx.out(l.t("catchup-nothing"));
        return Ok(());
    }
    ctx.out(l.t("catchup-finished"));
    for c in list {
        let who = c["assignee"].as_str().unwrap_or_default();
        let mut line = format!("- {}", c["title"].as_str().unwrap_or_default());
        if !who.is_empty() {
            line.push_str(&format!(" ({who})"));
        }
        if let Some(s) = c["summary"].as_str() {
            line.push_str(&format!(": {s}"));
        }
        ctx.out(line);
    }
    Ok(())
}

pub async fn areas(ctx: &Ctx, path: Option<String>) -> anyhow::Result<()> {
    let l = &ctx.l;
    let mut api = ctx.api().await?;
    let cwd = std::env::current_dir()?;
    let params = match &path {
        Some(p) => {
            let abs = cwd.join(p);
            json!({ "dir": cwd, "path": abs.to_string_lossy() })
        }
        None => json!({ "dir": cwd }),
    };
    let v: Value = call(&mut api, "areas.get", params).await?;
    if ctx.json {
        ctx.print_json(&v);
        return Ok(());
    }
    match path {
        Some(_) => match v["area"].as_object() {
            Some(a) => ctx.out(tr!(
                l,
                "areas-of",
                path = v["path"].as_str().unwrap_or_default(),
                pattern = a.get("pattern").and_then(|p| p.as_str()).unwrap_or_default(),
                owners = owners(&v["area"])
            )),
            None => ctx.out(tr!(l, "areas-none-for", path = v["path"].as_str().unwrap_or_default())),
        },
        None => {
            let list = v["areas"].as_array().cloned().unwrap_or_default();
            if list.is_empty() {
                ctx.out(l.t("areas-empty"));
                return Ok(());
            }
            ctx.out(tr!(l, "areas-from", file = v["source"].as_str().unwrap_or_default()));
            for a in list {
                let mut line = format!("  {}  {}", a["pattern"].as_str().unwrap_or_default(), owners(&a));
                if let Some(n) = a["note"].as_str() {
                    line.push_str(&format!("  # {n}"));
                }
                ctx.out(line);
            }
        }
    }
    Ok(())
}

fn owners(a: &Value) -> String {
    a["owners"]
        .as_array()
        .map(|o| o.iter().filter_map(|x| x.as_str()).collect::<Vec<_>>().join(" "))
        .unwrap_or_default()
}
