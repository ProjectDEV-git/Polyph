//! The room's repository on the command line: remembering it when a room is
//! made from a checkout, offering to clone it when someone joins in an empty
//! folder, and `polyph sync`. Texts are in `locales/*/setup.ftl`.

use std::io::IsTerminal;
use std::path::Path;

use polyph_i18n::tr;
use serde_json::{Value, json};

use crate::client::Api;
use crate::{Ctx, call};

/// Records `root`'s `origin` (without credentials) as the room's repository.
pub async fn remember(ctx: &Ctx, api: &mut Api, room: &str, root: &Path) {
    let Ok(repo) = polyph_git::Repo::open(root) else { return };
    let Some(url) = repo.remote_url("origin").and_then(|u| polyph_git::clean_remote(&u)) else { return };
    let branch = repo.current_branch();
    let files = repo.file_count().ok();
    let r = call(api, "repo.set", json!({ "room": room, "url": url, "branch": branch, "files": files })).await;
    if r.is_ok() && !ctx.json {
        ctx.out(tr!(ctx.l, "repo-remembered", url = url.as_str()));
    }
}

/// `polyph doctor`'s check that this folder is a clone of the room's repository.
pub async fn doctor(ctx: &Ctx, api: &mut Api, room: &str, cwd: &Path) -> Option<Result<String, (String, String)>> {
    let l = &ctx.l;
    let info = call(api, "repo.get", json!({ "room": room })).await.ok()?;
    let want = info["url"].as_str()?.to_string();
    let have = polyph_git::Repo::open(cwd)
        .ok()
        .and_then(|r| r.remote_url("origin"))
        .and_then(|u| polyph_git::clean_remote(&u));
    Some(match have {
        Some(h) if same_repo(&h, &want) => Ok(tr!(l, "doctor-repo-ok", url = want.as_str())),
        Some(h) => Err((
            tr!(l, "doctor-repo-differs", have = h.as_str(), want = want.as_str()),
            format!("git remote set-url origin {want}"),
        )),
        None => Err((tr!(l, "doctor-repo-missing", url = want.as_str()), format!("git clone {want}"))),
    })
}

/// The same repository, whatever the protocol or `.git` suffix.
pub fn same_repo(a: &str, b: &str) -> bool {
    fn key(u: &str) -> String {
        let u = u.trim().trim_end_matches('/').trim_end_matches(".git").to_lowercase();
        let u = u.split_once("://").map(|(_, r)| r.to_string()).unwrap_or(u);
        let u = u.rsplit_once('@').map(|(_, r)| r.to_string()).unwrap_or(u);
        u.replacen(':', "/", 1)
    }
    key(a) == key(b)
}

fn empty_dir(dir: &Path) -> bool {
    std::fs::read_dir(dir).map(|mut d| d.next().is_none()).unwrap_or(false)
}

/// After joining: link this folder when it is a checkout, or, in an empty
/// folder, offer to clone the room's repository here with the person's own
/// git login (blobless for large repositories).
pub async fn after_join(ctx: &Ctx, api: &mut Api, room: &str, clone: Option<bool>) -> anyhow::Result<()> {
    let l = &ctx.l;
    let cwd = std::env::current_dir()?;
    if polyph_git::Repo::open(&cwd).is_ok() {
        call(api, "folder.link", json!({ "room": room, "dir": cwd })).await?;
        if !ctx.json {
            ctx.out(tr!(l, "repo-linked", dir = polyph_daemon::folders::root(&cwd).display().to_string()));
        }
        return Ok(());
    }
    // A room just joined is still catching up on its shared settings.
    let mut info = Value::Null;
    for _ in 0..30 {
        info = call(api, "repo.get", json!({ "room": room })).await.unwrap_or_default();
        if !info.is_null() {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    }
    let Some(url) = info["url"].as_str() else {
        // Asked to clone, but there is nothing to clone: say so.
        if clone == Some(true) && !ctx.json {
            ctx.out(l.t("repo-clone-none"));
        }
        return Ok(());
    };
    if clone == Some(false) || !empty_dir(&cwd) {
        if !ctx.json {
            ctx.out(tr!(l, "repo-clone-hint", url = url));
        }
        return Ok(());
    }
    let large = info["large"] == true;
    if clone != Some(true) {
        if ctx.json || !std::io::stdin().is_terminal() {
            ctx.out(tr!(l, "repo-clone-hint", url = url));
            return Ok(());
        }
        let q = tr!(l, "repo-clone-confirm", url = url, dir = cwd.display().to_string());
        if !crate::setup::confirm_default_yes(&q)? {
            return Ok(());
        }
    }
    if large && !ctx.json {
        ctx.out(tr!(l, "repo-clone-partial", files = info["files"].as_u64().unwrap_or(0)));
    }
    let (u, dir, branch) = (url.to_string(), cwd.clone(), info["branch"].as_str().map(str::to_string));
    tokio::task::spawn_blocking(move || polyph_git::clone(&u, &dir, branch.as_deref(), large))
        .await?
        .map_err(|e| anyhow::anyhow!(tr!(l, "repo-clone-failed", url = url, error = e.to_string())))?;
    call(api, "folder.link", json!({ "room": room, "dir": cwd })).await?;
    if !ctx.json {
        ctx.out(tr!(l, "repo-cloned", url = url, dir = cwd.display().to_string()));
    }
    Ok(())
}

#[derive(clap::Subcommand, Debug)]
pub enum RepoCmd {
    /// Show the room's repository.
    Show,
    /// Set the room's repository (credentials in the URL are removed).
    Set {
        url: String,
        #[arg(long)]
        branch: Option<String>,
    },
    /// Forget the room's repository.
    Clear,
}

/// `polyph repo`: show or change the room's repository.
pub async fn repo_cmd(ctx: &Ctx, cmd: Option<RepoCmd>, room: Option<String>) -> anyhow::Result<()> {
    let l = &ctx.l;
    let mut api = ctx.api().await?;
    let room = match room {
        Some(r) => r,
        None => {
            let linked = call(&mut api, "folder.find", json!({ "dir": std::env::current_dir()? })).await?;
            match linked["room"].as_str() {
                Some(r) => r.to_string(),
                None => ctx.room(&mut api, None).await?,
            }
        }
    };
    let v = match cmd.unwrap_or(RepoCmd::Show) {
        RepoCmd::Show => call(&mut api, "repo.get", json!({ "room": room })).await?,
        RepoCmd::Set { url, branch } => {
            let files = std::env::current_dir()
                .ok()
                .and_then(|d| polyph_git::Repo::open(&d).ok())
                .filter(|r| r.remote_url("origin").is_some_and(|o| same_repo(&o, &url)))
                .and_then(|r| r.file_count().ok());
            call(&mut api, "repo.set", json!({ "room": room, "url": url, "branch": branch, "files": files })).await?
        }
        RepoCmd::Clear => call(&mut api, "repo.set", json!({ "room": room, "clear": true })).await?,
    };
    if ctx.json {
        ctx.print_json(&v);
    } else if let Some(url) = v["url"].as_str() {
        ctx.out(tr!(l, "repo-show", url = url, branch = v["branch"].as_str().unwrap_or("-")));
    } else {
        ctx.out(l.t("repo-none"));
    }
    Ok(())
}

/// `polyph sync`: fetch, and move branches forward where that loses nothing.
pub async fn sync(ctx: &Ctx, room: Option<String>, all: bool) -> anyhow::Result<()> {
    let l = &ctx.l;
    let mut api = ctx.api().await?;
    let params = if all {
        json!({})
    } else if let Some(r) = room {
        json!({ "room": r })
    } else {
        json!({ "dir": std::env::current_dir()? })
    };
    let out = call(&mut api, "sync.run", params).await?;
    if ctx.json {
        ctx.print_json(&out);
        return Ok(());
    }
    let list = out.as_array().cloned().unwrap_or_default();
    if list.is_empty() {
        ctx.out(l.t("sync-nothing"));
    }
    for r in list {
        let dir = r["dir"].as_str().unwrap_or_default();
        if let Some(e) = r["error"].as_str() {
            ctx.out(tr!(l, "sync-failed", dir = dir, error = e));
            continue;
        }
        let rep = &r["report"];
        let branch = rep["branch"].as_str().unwrap_or("HEAD");
        let (ahead, behind) = (rep["ahead"].as_u64().unwrap_or(0), rep["behind"].as_u64().unwrap_or(0));
        let line = if rep["upstream"].is_null() {
            tr!(l, "sync-no-upstream", dir = dir, branch = branch)
        } else if rep["fast_forwarded"] == true {
            tr!(l, "sync-updated", dir = dir, branch = branch, count = behind)
        } else if rep["held"] == "dirty" {
            tr!(l, "sync-held-dirty", dir = dir, branch = branch, count = behind)
        } else if rep["held"] == "diverged" {
            tr!(l, "sync-held-diverged", dir = dir, branch = branch, ahead = ahead, behind = behind)
        } else if ahead > 0 {
            tr!(l, "sync-ahead", dir = dir, branch = branch, count = ahead)
        } else {
            tr!(l, "sync-current", dir = dir, branch = branch)
        };
        ctx.out(line);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::same_repo;

    #[test]
    fn same_repository_any_protocol() {
        assert!(same_repo("git@github.com:o/r.git", "https://github.com/o/r"));
        assert!(same_repo("https://GitHub.com/o/r/", "ssh://git@github.com/o/r.git"));
        assert!(!same_repo("https://github.com/o/r", "https://github.com/o/other"));
    }
}
