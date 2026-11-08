//! The room's repository, and keeping linked folders up to date.
//!
//! A room remembers where its code lives (`repo` in the room settings): the
//! remote URL without any user name, password or token (see
//! `polyph_git::clean_remote`), the branch, and how many files it has, so a
//! teammate who joins in an empty folder can clone it with their own git
//! login, and a large repository gets a blobless clone.
//!
//! Folders linked to rooms (see `folders`) are fetched in the background
//! every `auto_fetch` minutes, and at once when a teammate's merge lands, so
//! `polyph sync` (and agents) see new work without anyone running git fetch.
//! Fetching never changes anyone's files; `sync.run` moves a branch forward
//! only when that loses nothing.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Weak};
use std::time::Duration;

use polyph_proto::messages::DocName;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::daemon::{ApiError, ApiResult, Daemon};
use crate::runtime::{RoomRuntime, apply};

/// Above this many tracked files a joiner gets a blobless clone.
pub const LARGE_REPO_FILES: usize = 20_000;
const SETTING: &str = "repo";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RepoInfo {
    pub url: String,
    #[serde(default)]
    pub branch: Option<String>,
    #[serde(default)]
    pub files: Option<usize>,
}

impl RepoInfo {
    pub fn large(&self) -> bool {
        self.files.is_some_and(|f| f > LARGE_REPO_FILES)
    }
}

pub async fn get(rt: &RoomRuntime) -> Option<RepoInfo> {
    let mut ctx = rt.ctx.lock().await;
    let v = polyph_core::docs::setting(ctx.core.doc(&DocName::Settings), SETTING)?;
    serde_json::from_value(v).ok()
}

fn view(info: Option<RepoInfo>) -> Value {
    match info {
        Some(i) => json!({ "url": i.url, "branch": i.branch, "files": i.files, "large": i.large() }),
        None => Value::Null,
    }
}

pub async fn call(d: &Arc<Daemon>, method: &str, params: Value) -> ApiResult {
    #[derive(Deserialize)]
    struct P {
        room: Option<String>,
        url: Option<String>,
        branch: Option<String>,
        files: Option<usize>,
        dir: Option<PathBuf>,
        #[serde(default)]
        clear: bool,
    }
    let p: P = serde_json::from_value(params).map_err(|e| ApiError::BadRequest(format!("invalid parameters: {e}")))?;
    match method {
        "repo.get" => {
            let rt = d.find(p.room.as_deref().unwrap_or_default()).await?;
            Ok(view(get(&rt).await))
        }
        "repo.set" => {
            let rt = d.find(p.room.as_deref().unwrap_or_default()).await?;
            let value = if p.clear {
                Value::Null
            } else {
                let raw = p.url.ok_or_else(|| ApiError::BadRequest("url is required".into()))?;
                let url = polyph_git::clean_remote(&raw).ok_or_else(|| {
                    ApiError::BadRequest(format!("{raw} is not a remote others can clone (use an https or ssh URL)"))
                })?;
                if let Some(b) = &p.branch {
                    polyph_git::check_branch_name(b).map_err(|e| ApiError::BadRequest(e.to_string()))?;
                }
                serde_json::to_value(RepoInfo { url, branch: p.branch, files: p.files }).unwrap_or_default()
            };
            let mut c = rt.ctx.lock().await;
            if !c.core.role().can_approve_joins() {
                return Err(ApiError::Forbidden("only Owners and Maintainers set the room's repository".into()));
            }
            let fx =
                c.core.change_doc(DocName::Settings, |doc| polyph_core::docs::set_setting(doc, SETTING, &value))?;
            apply(&rt, &mut c, &d.shared, fx).await;
            drop(c);
            Ok(view(get(&rt).await))
        }
        // Fetches now and moves branches forward where safe: one folder, a
        // room's folders, or every linked folder.
        "sync.run" => {
            let dirs = folders_for(d, p.room.as_deref(), p.dir.as_deref()).await?;
            let mut out = vec![];
            for (room, dir) in dirs {
                let r = tokio::task::spawn_blocking({
                    let (dir, home) = (dir.clone(), d.shared.paths.home.clone());
                    move || crate::folders::open_for_network(&home, &dir).and_then(|r| r.sync("origin"))
                })
                .await
                .map_err(|e| ApiError::Failed(e.to_string()))?;
                out.push(match r {
                    Ok(rep) => json!({ "room": room, "dir": dir, "report": rep }),
                    Err(e) => json!({ "room": room, "dir": dir, "error": e.to_string() }),
                });
            }
            Ok(json!(out))
        }
        _ => Err(ApiError::NotFound(format!("unknown method {method}"))),
    }
}

async fn folders_for(d: &Daemon, room: Option<&str>, dir: Option<&Path>) -> Result<Vec<(String, PathBuf)>, ApiError> {
    let links = crate::folders::load(&d.shared.paths.home);
    if let Some(dir) = dir {
        let l = crate::folders::room_for(&d.shared.paths.home, dir);
        return Ok(vec![(l.map(|l| l.room).unwrap_or_default(), crate::folders::root(dir))]);
    }
    let want = match room {
        Some(r) => Some(d.find(r).await?.id.to_hex()),
        None => None,
    };
    Ok(links.into_iter().filter(|l| want.as_ref().is_none_or(|w| *w == l.room)).map(|l| (l.room, l.path)).collect())
}

/// Fetches every linked folder that has an `origin`, quietly.
async fn fetch_all(d: &Daemon) {
    for l in crate::folders::load(&d.shared.paths.home) {
        let home = d.shared.paths.home.clone();
        let path = l.path.clone();
        let r = tokio::task::spawn_blocking(move || {
            let r = crate::folders::open_for_network(&home, &l.path)?;
            if r.remote_url("origin").is_some() { r.fetch("origin") } else { Ok(()) }
        })
        .await;
        match r {
            Ok(Err(e)) => tracing::warn!(folder = %path.display(), error = %e, "background fetch failed"),
            Err(e) => tracing::warn!(folder = %path.display(), error = %e, "background fetch did not finish"),
            Ok(Ok(())) => {}
        }
    }
}

/// Whether an event says a merge landed since `since`: ours (the merge
/// queue) or a teammate's (a "merged" entry in the room's activity).
async fn merge_landed(weak: &Weak<Daemon>, ev: &crate::events::ApiEvent, since: tokio::time::Instant) -> bool {
    if ev.kind() == "merge_queue" {
        return ev.event.pointer("/entry/state").and_then(|s| s.as_str()) == Some("merged");
    }
    if ev.kind() != "doc_changed" || ev.event.get("doc").and_then(|d| d.as_str()) != Some("activity") {
        return false;
    }
    let (Some(d), Some(room)) = (weak.upgrade(), ev.room.as_deref()) else { return false };
    let Ok(rt) = d.find(room).await else { return false };
    let now = d.shared.clock.now();
    let age = i64::try_from(since.elapsed().as_secs()).unwrap_or(i64::MAX);
    let mut ctx = rt.ctx.lock().await;
    let entries = polyph_proto::crdt::Activity::new(ctx.core.doc(&DocName::Activity)).entries();
    entries.iter().rev().take(5).any(|e| e.kind == "merged" && now - e.at <= age)
}

/// Background fetching: every `auto_fetch` minutes, and soon after a task
/// card reaches done (a merge landed somewhere).
pub async fn run(weak: Weak<Daemon>) {
    let Some(d) = weak.upgrade() else { return };
    let mut events = d.subscribe();
    drop(d);
    let mut last = tokio::time::Instant::now();
    loop {
        let minutes = match weak.upgrade() {
            Some(d) => d.shared.config.lock().auto_fetch,
            None => return,
        };
        let wait = if minutes == 0 { Duration::from_secs(3600) } else { Duration::from_secs(u64::from(minutes) * 60) };
        let merged = tokio::select! {
            _ = tokio::time::sleep_until(last + wait) => false,
            e = events.recv() => match e {
                Ok(ev) => merge_landed(&weak, &ev, last).await,
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => false,
                Err(_) => return,
            },
        };
        let due = tokio::time::Instant::now() >= last + wait;
        if minutes == 0 || !(merged || due) {
            continue;
        }
        let Some(d) = weak.upgrade() else { return };
        fetch_all(&d).await;
        last = tokio::time::Instant::now();
    }
}
