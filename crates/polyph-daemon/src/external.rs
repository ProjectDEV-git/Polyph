//! Agents the person runs themselves (Claude Code, Codex, Cursor, VS Code,
//! Gemini CLI, Windsurf, Zed, OpenCode...) joining a room through Polyph's
//! MCP tools.
//!
//! Such an agent reaches the daemon in one of two ways:
//!
//! - directly: its MCP config starts `polyph mcp`, which, outside a Polyph
//!   sandbox, talks to this daemon's local API with the API token
//!   (`mcp.connect`, then `mcp.call` per tool call);
//! - over a local web address: its MCP config points at
//!   `http://127.0.0.1:<port>/mcp` with a connect key (see `api.rs`), and
//!   each MCP session there is one connection.
//!
//! A connection gets an [`AgentRef`] in the room like any agent Polyph starts:
//! others see it in `team_state`, can message it, and its claims are real
//! claims. It has no sandbox, no task worktree and no turn boundaries, so
//! Polyph cannot commit for it or push messages to it; tool results tell it
//! when messages are waiting instead.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicI64, Ordering};

use polyph_mcp::ToolOutput;
use polyph_proto::ids::{ObjectId, RoomId};
use polyph_proto::messages::{AgentEventKind, AgentRef, AgentState};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tokio::sync::Mutex;

use crate::coord::Caller;
use crate::daemon::{ApiError, ApiResult, Daemon};
use crate::runtime::RoomRuntime;

/// A connection with no tool call for this long is closed.
pub const IDLE_SECS: i64 = 30 * 60;

/// Kinds of client `polyph connect` knows, as they appear in labels.
pub const CLIENTS: &[&str] =
    &["claude-code", "codex", "cursor", "vscode", "gemini", "windsurf", "zed", "opencode", "copilot", "agent"];

#[derive(Debug)]
pub struct Session {
    pub id: String,
    pub caller: Caller,
    pub room: RoomId,
    pub client: String,
    /// "stdio" (through `polyph mcp`) or "http".
    pub via: String,
    last: AtomicI64,
}

#[derive(Debug, Default)]
pub struct External {
    sessions: Mutex<HashMap<String, Arc<Session>>>,
}

#[derive(Debug, Deserialize)]
pub struct ConnectParams {
    /// Room id, prefix or name; default: the room linked to `cwd`.
    pub room: Option<String>,
    /// Where the agent works.
    pub cwd: PathBuf,
    /// Which client: claude-code, codex, cursor...
    #[serde(default = "default_client")]
    pub client: String,
    #[serde(default = "default_via")]
    pub via: String,
}

fn default_client() -> String {
    "agent".into()
}

fn default_via() -> String {
    "stdio".into()
}

/// A name safe for a label: lower-case letters, digits and dashes.
fn clean_client(c: &str) -> String {
    let s: String =
        c.trim().to_lowercase().chars().map(|ch| if ch.is_ascii_alphanumeric() { ch } else { '-' }).take(24).collect();
    let s = s.trim_matches('-').to_string();
    if s.is_empty() { default_client() } else { s }
}

#[derive(Debug, Serialize)]
pub struct SessionInfo {
    pub id: String,
    pub agent: String,
    pub label: String,
    pub room: String,
    pub client: String,
    pub via: String,
    pub cwd: PathBuf,
    pub idle_secs: i64,
}

impl External {
    pub async fn get(&self, id: &str) -> Option<Arc<Session>> {
        self.sessions.lock().await.get(id).cloned()
    }

    pub async fn list(&self) -> Vec<Arc<Session>> {
        self.sessions.lock().await.values().cloned().collect()
    }

    /// Labels in use in a room by connected agents of this device.
    pub async fn labels(&self, room: RoomId) -> Vec<String> {
        self.sessions.lock().await.values().filter(|s| s.room == room).map(|s| s.caller.agent.label.clone()).collect()
    }
}

/// Finds the room for a connection: the one named, the one linked to the
/// folder, or the only open one.
pub async fn room_for(d: &Daemon, room: Option<&str>, cwd: &std::path::Path) -> Result<Arc<RoomRuntime>, ApiError> {
    if let Some(r) = room.filter(|r| !r.trim().is_empty()) {
        return d.find(r).await;
    }
    if let Some(l) = crate::folders::room_for(&d.shared.paths.home, cwd) {
        return d.find(&l.room).await;
    }
    let open = d.open_rooms().await;
    match open.as_slice() {
        [only] => Ok(only.clone()),
        [] => Err(ApiError::NotFound(
            "this folder is not linked to a room and no room is open; run `polyph start` here first".into(),
        )),
        _ => Err(ApiError::BadRequest(format!(
            "this folder ({}) is not linked to a room and several rooms are open; run `polyph start` here, \
             or pass the room with `polyph mcp --room <room>`",
            cwd.display()
        ))),
    }
}

pub async fn connect(d: &Arc<Daemon>, p: ConnectParams) -> ApiResult {
    let cwd = polyph_git::canonical(&p.cwd).map_err(|e| ApiError::BadRequest(format!("{}: {e}", p.cwd.display())))?;
    let rt = room_for(d, p.room.as_deref(), &cwd).await?;
    let client = clean_client(&p.client);
    let (member, device, my_name, room) = {
        let ctx = rt.ctx.lock().await;
        if !ctx.core.role().can_write() {
            return Err(ApiError::Forbidden("viewers cannot run agents".into()));
        }
        (ctx.core.my_member, ctx.core.me, ctx.core.my_profile.display_name.clone(), ctx.core.id)
    };
    // Same naming as agents Polyph starts: Ann/codex, Ann/codex2...
    let mut taken: Vec<String> = d.external.labels(room).await;
    taken.extend(d.agents.map.lock().await.values().filter(|h| h.room == room).map(|h| h.agent.label.clone()));
    let base = format!("{my_name}/{client}");
    let label = (1..)
        .map(|n| if n == 1 { base.clone() } else { format!("{base}{n}") })
        .find(|l| !taken.contains(l))
        .unwrap_or(base);
    let id = ObjectId::random().map_err(|e| ApiError::Failed(e.to_string()))?;
    let session = ObjectId::random().map_err(|e| ApiError::Failed(e.to_string()))?.to_hex()
        + &ObjectId::random().map_err(|e| ApiError::Failed(e.to_string()))?.to_hex();
    let agent = AgentRef { id, member, device, kind: client.clone(), label: label.clone() };
    let worktree = crate::folders::root(&cwd);
    let s = Arc::new(Session {
        id: session.clone(),
        caller: Caller { agent: agent.clone(), task: None, worktree: worktree.clone(), hops: 0, external: true },
        room,
        client: client.clone(),
        via: p.via.clone(),
        last: AtomicI64::new(d.shared.clock.now()),
    });
    d.external.sessions.lock().await.insert(session.clone(), s);
    crate::agents::publish(&rt, &d.shared, &agent, AgentEventKind::Started { task: None, model: None, cloud: false })
        .await;
    crate::agents::publish(&rt, &d.shared, &agent, AgentEventKind::State { state: AgentState::Idle, detail: None })
        .await;
    crate::feed::log(&rt, &d.shared, "agent_connected", Some(&label), json!({ "client": client, "via": p.via })).await;
    // Messages that waited for this agent while it (or the daemon) was away.
    crate::coord::restore_inbox(&rt, &d.shared, &agent).await;
    let name = rt.ctx.lock().await.core.name.clone();
    Ok(
        json!({ "session": session, "agent": id.to_hex(), "label": label, "room": rt.id.to_hex(), "room_name": name, "worktree": worktree }),
    )
}

pub async fn disconnect(d: &Arc<Daemon>, session: &str, reason: &str) -> ApiResult {
    let Some(s) = d.external.sessions.lock().await.remove(session) else {
        return Ok(json!({ "closed": false }));
    };
    if let Ok(rt) = d.find(&s.room.to_hex()).await {
        // Its claims go with it, as when Polyph stops an agent.
        {
            let mut ctx = rt.ctx.lock().await;
            let ids: Vec<ObjectId> =
                ctx.core.claims.all().into_iter().filter(|c| c.owner.id == s.caller.agent.id).map(|c| c.id).collect();
            let fx = ctx.core.release_claims(&ids);
            crate::runtime::apply(&rt, &mut ctx, &d.shared, fx).await;
        }
        crate::coord::save_inbox(&rt, &d.shared, &s.caller.agent).await;
        crate::agents::publish(&rt, &d.shared, &s.caller.agent, AgentEventKind::Stopped { reason: reason.into() })
            .await;
    }
    Ok(json!({ "closed": true, "label": s.caller.agent.label }))
}

pub async fn call(d: &Arc<Daemon>, session: &str, tool: &str, args: &Value) -> Result<ToolOutput, ApiError> {
    let s = d.external.get(session).await.ok_or_else(|| {
        ApiError::NotFound("this connection to Polyph was closed (idle or daemon restarted); reconnect".into())
    })?;
    s.last.store(d.shared.clock.now(), Ordering::Relaxed);
    let rt = d.find(&s.room.to_hex()).await?;
    let mut out = crate::coord::call_tool(&s.caller, &rt, &d.shared, tool, args).await;
    // No turn boundaries to deliver at: say so in every answer instead.
    if tool != polyph_mcp::tools::INBOX {
        let ctx = rt.ctx.lock().await;
        if let Some(i) = ctx.coord.inboxes.get(&s.caller.agent.id) {
            let n = i.unread.len();
            let notices = i.notices.len();
            if n > 0 || notices > 0 {
                out.text
                    .push_str(&format!("\n\n(Polyph: you have {} unread message(s); call read_inbox.)", n + notices));
            }
        }
    }
    Ok(out)
}

pub async fn list(d: &Arc<Daemon>) -> Vec<SessionInfo> {
    let now = d.shared.clock.now();
    let mut v: Vec<SessionInfo> = d
        .external
        .list()
        .await
        .into_iter()
        .map(|s| SessionInfo {
            id: s.id.chars().take(8).collect(),
            agent: s.caller.agent.id.to_hex(),
            label: s.caller.agent.label.clone(),
            room: s.room.to_hex(),
            client: s.client.clone(),
            via: s.via.clone(),
            cwd: s.caller.worktree.clone(),
            idle_secs: now - s.last.load(Ordering::Relaxed),
        })
        .collect();
    v.sort_by(|a, b| a.label.cmp(&b.label));
    v
}

/// Closes connections idle for longer than [`IDLE_SECS`].
pub async fn reap(d: &Arc<Daemon>) {
    let now = d.shared.clock.now();
    let idle: Vec<String> = d
        .external
        .list()
        .await
        .into_iter()
        .filter(|s| now - s.last.load(Ordering::Relaxed) > IDLE_SECS)
        .map(|s| s.id.clone())
        .collect();
    for id in idle {
        let _ = disconnect(d, &id, "idle").await;
    }
}

/// Every connection, closed (daemon shutting down).
pub async fn close_all(d: &Arc<Daemon>) {
    let ids: Vec<String> = d.external.list().await.into_iter().map(|s| s.id.clone()).collect();
    for id in ids {
        let _ = disconnect(d, &id, "daemon stopped").await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn client_names_are_label_safe() {
        assert_eq!(clean_client("Claude Code"), "claude-code");
        assert_eq!(clean_client("  "), "agent");
        assert_eq!(clean_client("a/b\\c"), "a-b-c");
        assert!(clean_client(&"x".repeat(100)).len() <= 24);
    }
}
