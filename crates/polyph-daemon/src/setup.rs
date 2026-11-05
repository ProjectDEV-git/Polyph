//! The first-run checklist's view of this device (`setup.status`) and
//! connecting the person's agents from the app (`clients.plan`,
//! `clients.connect`), the same changes `polyph connect` makes.
//!
//! `clients.connect` writes files in the person's home, so the local API
//! only accepts it with the API token (the desktop app and the CLI), never
//! from a browser session (see `api.rs`).

use std::sync::Arc;

use serde::Deserialize;
use serde_json::{Value, json};

use crate::clients::{self, CLIENTS, Client, Env, Transport};
use crate::daemon::{ApiError, ApiResult, Daemon};

/// The data directory to pass to `polyph mcp` when it is not the default.
fn home_override(d: &Daemon) -> Option<std::path::PathBuf> {
    let default = directories::ProjectDirs::from("dev", "polyph", "polyph").map(|p| p.config_dir().to_path_buf());
    (default.as_deref() != Some(d.shared.paths.home.as_path())).then(|| d.shared.paths.home.clone())
}

fn transport(d: &Daemon) -> Result<Transport, ApiError> {
    let exe = std::env::current_exe().and_then(|e| polyph_git::canonical(&e))?;
    Ok(Transport::stdio(&exe, home_override(d).as_deref()))
}

fn env() -> Result<Env, ApiError> {
    Env::current().ok_or_else(|| ApiError::Failed("could not find the home folder".into()))
}

/// Clients to connect: those named, or every installed one that has no
/// Polyph entry and does not get Polyph from an extension or plugin.
fn targets(env: &Env, ids: &[String]) -> Result<Vec<&'static Client>, ApiError> {
    if ids.is_empty() {
        return Ok(CLIENTS
            .iter()
            .filter(|c| c.installed(env) && !c.registered_by_extension(env) && clients::current(c, env).is_none())
            .collect());
    }
    ids.iter().map(|i| clients::find(i).ok_or_else(|| ApiError::BadRequest(format!("unknown agent {i}")))).collect()
}

#[derive(Debug, Deserialize)]
struct Ids {
    #[serde(default)]
    clients: Vec<String>,
}

pub async fn call(d: &Arc<Daemon>, method: &str, params: Value) -> ApiResult {
    match method {
        "setup.status" => {
            let env = Env::current();
            let list: Vec<Value> = match &env {
                Some(env) => CLIENTS
                    .iter()
                    .map(|c| {
                        let via_extension = c.registered_by_extension(env);
                        json!({
                            "id": c.id,
                            "name": c.name,
                            "installed": c.installed(env) || via_extension,
                            "connected": via_extension || clients::current(c, env).is_some(),
                            "via_extension": via_extension,
                        })
                    })
                    .collect(),
                None => vec![],
            };
            let name = d.shared.config.lock().name.clone().filter(|n| !n.trim().is_empty());
            let rooms = d.room_list().await;
            let folders = crate::folders::load(&d.shared.paths.home);
            let local = d.agents.map.lock().await.values().filter(|h| !h.helper).count();
            let connected = d.external.list().await.len();
            let mut members = 1;
            for rt in &rooms {
                let ctx = rt.ctx.lock().await;
                let mut all: std::collections::HashSet<_> =
                    ctx.core.peers.values().filter_map(|p| p.member()).collect();
                all.insert(ctx.core.my_member);
                members = members.max(all.len());
            }
            Ok(json!({
                "name": name,
                "rooms": rooms.len(),
                "folders": folders,
                "clients": list,
                "agents": local + connected,
                "members": members,
            }))
        }
        "clients.plan" | "clients.connect" => {
            let p: Ids =
                serde_json::from_value(params).map_err(|e| ApiError::BadRequest(format!("invalid parameters: {e}")))?;
            let env = env()?;
            let t = transport(d)?;
            let mut out = vec![];
            for c in targets(&env, &p.clients)? {
                let change = clients::plan_add(c, &env, &t).map_err(|e| ApiError::BadRequest(e.to_string()))?;
                if change.is_noop() {
                    continue;
                }
                let mut v = json!({
                    "client": c.id,
                    "name": c.name,
                    "file": change.file,
                    "before": change.before,
                    "after": change.after,
                    "existed": change.existed,
                    "drops_comments": change.drops_comments,
                });
                if method == "clients.connect" {
                    let backup = clients::apply(&change)?;
                    v["backup"] = json!(backup);
                }
                out.push(v);
            }
            Ok(json!(out))
        }
        _ => Err(ApiError::NotFound(format!("unknown method {method}"))),
    }
}
