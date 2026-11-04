//! Agent hosts: a person's own server, joined to the room as another device
//! of theirs with the `agent-host` capability, runs agents for them so work
//! continues with the laptop closed. The person's other devices start,
//! prompt and stop agents there with `host` room messages; transcripts,
//! claims and approvals flow through the room like any agent's.
//!
//! A host acts only on requests from devices of its own member. A requester
//! trusts replies only from a device whose certificate makes it a host of
//! the same member.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Weak};
use std::time::Duration;

use polyph_proto::ids::{DeviceId, MemberId, ObjectId};
use polyph_proto::messages::{AgentRef, HostMsg, RoomMessage, Surface};
use serde::Deserialize;
use serde_json::{Value, json};
use tokio::sync::{Mutex, broadcast, oneshot};
use tracing::{debug, warn};

use crate::daemon::{ApiError, ApiResult, Daemon};
use crate::events::ApiEvent;
use crate::runtime::RoomRuntime;

/// Starts waiting for a host's answer this long.
const START_TIMEOUT: Duration = Duration::from_secs(60);

type Waiter = oneshot::Sender<Result<AgentRef, String>>;

/// Start requests waiting for their host's answer.
#[derive(Default)]
pub struct Hosts {
    waiters: Mutex<HashMap<ObjectId, (DeviceId, Waiter)>>,
}

impl std::fmt::Debug for Hosts {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Hosts")
    }
}

/// One agent host as the API lists it.
#[derive(Debug, Clone, serde::Serialize)]
pub struct HostInfo {
    pub device: String,
    pub name: String,
    pub online: bool,
    /// Agents running there now.
    pub agents: usize,
}

/// This member's agent hosts in a room.
pub async fn list(rt: &RoomRuntime, now: i64) -> Vec<HostInfo> {
    let c = rt.ctx.lock().await;
    let me = c.core.my_member;
    let mut out: Vec<HostInfo> = c
        .core
        .peers
        .iter()
        .filter(|(d, p)| p.member() == Some(me) && c.core.is_agent_host(d))
        .map(|(d, p)| {
            let agents = c
                .transcripts
                .values()
                .filter_map(|l| l.back().map(|e| (l.front().map(|f| f.agent.device), &e.kind)))
                .filter(|(dev, kind)| {
                    *dev == Some(*d) && !matches!(kind, polyph_proto::messages::AgentEventKind::Stopped { .. })
                })
                .count();
            HostInfo { device: d.to_hex(), name: p.name(), online: p.online(now), agents }
        })
        .collect();
    out.sort_by(|a, b| b.online.cmp(&a.online).then(a.device.cmp(&b.device)));
    out
}

async fn send(rt: &RoomRuntime, msg: HostMsg) {
    let mut c = rt.ctx.lock().await;
    crate::runtime::send(&mut c, &RoomMessage::Host(msg)).await;
}

/// Picks one of this member's hosts by device id prefix or name.
async fn resolve(rt: &RoomRuntime, q: &str, now: i64) -> Result<DeviceId, ApiError> {
    let hosts = list(rt, now).await;
    let ql = q.to_lowercase();
    let hits: Vec<&HostInfo> = hosts
        .iter()
        .filter(|h| h.device.starts_with(&ql) || h.name.to_lowercase() == ql || (ql == "cloud" && h.online))
        .collect();
    let pick = match hits.as_slice() {
        [one] => *one,
        [] if hosts.is_empty() => {
            return Err(ApiError::NotFound(
                "you have no agent host in this room; add one with `polyph agent-host invite`".into(),
            ));
        }
        [] => {
            return Err(ApiError::NotFound(format!("no agent host of yours matches \"{q}\"; see `polyph agent hosts`")));
        }
        // "cloud" with several online hosts: the least busy one.
        many if ql == "cloud" => many.iter().min_by_key(|h| h.agents).copied().unwrap_or(many[0]),
        _ => return Err(ApiError::BadRequest(format!("\"{q}\" matches several hosts; use more of the device id"))),
    };
    if !pick.online {
        return Err(ApiError::Failed(format!("agent host {} is offline", &pick.device[..8])));
    }
    DeviceId::from_hex(&pick.device).map_err(|e| ApiError::Failed(e.to_string()))
}

#[derive(Debug, Deserialize)]
pub struct StartOn {
    pub room: String,
    pub kind: String,
    pub on: String,
    pub preset: Option<String>,
    pub task: Option<String>,
    pub prompt: Option<String>,
}

/// Starts an agent on one of this member's hosts and waits for its answer.
pub async fn start_on(d: &Daemon, rt: &RoomRuntime, p: StartOn) -> ApiResult {
    let now = d.shared.clock.now();
    let host = resolve(rt, &p.on, now).await?;
    let req = ObjectId::random().map_err(|e| ApiError::Failed(e.to_string()))?;
    let task = match p.task.as_deref() {
        Some(t) => Some(ObjectId::from_hex(t).map_err(|_| ApiError::BadRequest("task must be a card id".into()))?),
        None => None,
    };
    let (tx, rx) = oneshot::channel();
    d.hosts.waiters.lock().await.insert(req, (host, tx));
    send(rt, HostMsg::Start { req, host, kind: p.kind, preset: p.preset, task, prompt: p.prompt }).await;
    let answer = tokio::time::timeout(START_TIMEOUT, rx).await;
    d.hosts.waiters.lock().await.remove(&req);
    match answer {
        Ok(Ok(Ok(agent))) => Ok(json!({ "id": agent.id.to_hex(), "label": agent.label, "host": host.to_hex() })),
        Ok(Ok(Err(e))) => Err(ApiError::Failed(format!("the agent host could not start it: {e}"))),
        _ => Err(ApiError::Failed("the agent host did not answer; check that its daemon is running".into())),
    }
}

/// The agent's host, when it runs on one of this member's hosts (not here).
pub async fn remote_agent(rt: &RoomRuntime, q: &str) -> Option<(DeviceId, ObjectId)> {
    let c = rt.ctx.lock().await;
    let ql = q.to_lowercase();
    let a = c
        .transcripts
        .values()
        .filter_map(|l| l.front().map(|e| e.agent.clone()))
        .find(|a| a.id.to_hex().starts_with(&ql) || a.label.to_lowercase() == ql)?;
    (a.member == c.core.my_member && a.device != c.core.me && c.core.is_agent_host(&a.device))
        .then_some((a.device, a.id))
}

pub async fn prompt_on(rt: &RoomRuntime, host: DeviceId, agent: ObjectId, text: &str) {
    send(rt, HostMsg::Prompt { host, agent, text: text.to_string() }).await;
}

pub async fn stop_on(rt: &RoomRuntime, host: DeviceId, agent: ObjectId) {
    send(rt, HostMsg::Stop { host, agent }).await;
}

/// Where a host runs agents: `host_workdir` in its config.
fn workdir(d: &Daemon) -> Option<PathBuf> {
    d.shared.config.lock().host_workdir.clone().map(PathBuf::from).filter(|p| p.is_dir())
}

#[derive(Deserialize)]
struct HostEvent {
    from: DeviceId,
    member: MemberId,
    host: bool,
    msg: HostMsg,
}

/// Handles `host` messages for this daemon until it stops.
pub async fn run(d: Weak<Daemon>, mut rx: broadcast::Receiver<ApiEvent>) {
    loop {
        let ev = match rx.recv().await {
            Ok(e) => e,
            Err(broadcast::error::RecvError::Lagged(n)) => {
                warn!(n, "agent host events lagged");
                continue;
            }
            Err(_) => return,
        };
        if ev.kind() != "host" {
            continue;
        }
        let Some(d) = d.upgrade() else { return };
        let Some(room) = ev.room.clone() else { continue };
        let Ok(h) = serde_json::from_value::<HostEvent>(ev.event.clone()) else { continue };
        let Ok(rt) = d.find(&room).await else { continue };
        tokio::spawn(async move { handle(d, rt, h).await });
    }
}

async fn handle(d: Arc<Daemon>, rt: Arc<RoomRuntime>, h: HostEvent) {
    let (me, my_member, i_host) = {
        let c = rt.ctx.lock().await;
        (c.core.me, c.core.my_member, c.core.is_agent_host(&c.core.me))
    };
    // Requests: only for this device, as a host, from its own member.
    let for_me = |host: &DeviceId| *host == me && i_host && h.member == my_member;
    match h.msg {
        HostMsg::Start { req, host, kind, preset, task, prompt } if for_me(&host) => {
            let Some(cwd) = workdir(&d) else {
                let error = "this agent host has no working directory; run `polyph agent-host workdir <dir>` on it";
                send(&rt, HostMsg::Failed { req, error: error.into() }).await;
                return;
            };
            let p = crate::agents::StartParams {
                room: rt.id.to_hex(),
                kind,
                cwd,
                preset,
                task: task.map(|t| t.to_hex()),
                prompt,
                worktree: None,
                helper: false,
            };
            let reply = match crate::agents::start(d.shared.clone(), &d.agents, rt.clone(), p).await {
                Ok(v) => {
                    let id = v["id"].as_str().and_then(|s| ObjectId::from_hex(s).ok());
                    let agent = match id {
                        Some(id) => d.agents.map.lock().await.get(&id).map(|a| a.agent.clone()),
                        None => None,
                    };
                    match agent {
                        Some(agent) => HostMsg::Started { req, agent },
                        None => HostMsg::Failed { req, error: "the agent stopped at once".into() },
                    }
                }
                Err(e) => HostMsg::Failed { req, error: e.to_string() },
            };
            send(&rt, reply).await;
        }
        HostMsg::Prompt { host, agent, text } if for_me(&host) => {
            let Some(a) = d.agents.map.lock().await.get(&agent).cloned() else { return };
            if let Err(e) = crate::agents::prompt(&d.shared, &rt, &a, &text).await {
                debug!(error = %e, "prompt from the agent's owner failed");
            }
        }
        HostMsg::Stop { host, agent } if for_me(&host) => {
            let _ = crate::agents::stop(&d.shared, &d.agents, &rt, agent).await;
        }
        // Replies: only from the host the request went to, a host of my member.
        HostMsg::Started { req, agent } if h.host && h.member == my_member && agent.device == h.from => {
            if let Some((host, tx)) = d.hosts.waiters.lock().await.remove(&req)
                && host == h.from
            {
                let _ = tx.send(Ok(agent));
            }
        }
        HostMsg::Failed { req, error } if h.host && h.member == my_member => {
            if let Some((host, tx)) = d.hosts.waiters.lock().await.remove(&req)
                && host == h.from
            {
                let _ = tx.send(Err(error.chars().take(500).collect()));
            }
        }
        _ => {}
    }
}

/// An enrollment for a new agent host: the command to run on the server.
pub async fn invite(d: &Arc<Daemon>, rt: &RoomRuntime) -> ApiResult {
    let inv = Box::pin(d.call("room.invite", json!({ "room": rt.id.to_hex() }))).await?;
    let code = inv["code"].as_str().unwrap_or_default().to_string();
    let member = d.shared.member_id()?.to_hex();
    let relay = rt.ctx.lock().await.relay.to_string_with_pin();
    let q = |s: &str| format!("'{}'", s.replace('\'', "'\\''"));
    let name = d.shared.display_name();
    let command = format!(
        "polyph agent-host join {code} --member {member} --relay {} --name {} --workdir <dir>",
        q(&relay),
        q(&name)
    );
    Ok(json!({ "code": code, "member": member, "relay": relay, "command": command, "expires_at": inv["expires_at"] }))
}

/// Joins a room as an agent host of `member` (run on the server).
pub async fn join(d: &Arc<Daemon>, p: Value) -> ApiResult {
    #[derive(Deserialize)]
    struct P {
        code: String,
        member: String,
        relay: String,
        workdir: String,
        name: Option<String>,
    }
    let p: P = serde_json::from_value(p).map_err(|e| ApiError::BadRequest(format!("invalid parameters: {e}")))?;
    let member =
        MemberId::from_hex(&p.member).map_err(|_| ApiError::BadRequest("member must be a member id".into()))?;
    let dir = std::fs::canonicalize(&p.workdir).map_err(|e| ApiError::BadRequest(format!("{}: {e}", p.workdir)))?;
    {
        let mut cfg = d.shared.config.lock();
        if let Some(existing) = cfg.member.as_deref()
            && existing != member.to_hex()
            && !cfg.host_rooms.is_empty()
        {
            return Err(ApiError::BadRequest(
                "this daemon already hosts agents for another person; use a separate POLYPH_HOME".into(),
            ));
        }
        cfg.member = Some(member.to_hex());
        cfg.host_workdir = Some(dir.to_string_lossy().into_owned());
        if let Some(n) = &p.name {
            cfg.name = Some(n.clone());
        }
        cfg.save(&d.shared.paths).map_err(|e| ApiError::Failed(e.to_string()))?;
    }
    let mut params = json!({ "code": p.code, "relay": p.relay, "capabilities": [polyph_proto::messages::AGENT_HOST] });
    if let Some(n) = p.name {
        params["name"] = json!(n);
    }
    let v = Box::pin(d.call("join", params)).await?;
    if let Some(id) = v["id"].as_str() {
        {
            let mut cfg = d.shared.config.lock();
            cfg.host_rooms.push(id.to_string());
            cfg.save(&d.shared.paths).map_err(|e| ApiError::Failed(e.to_string()))?;
        }
        if let Ok(rt) = Box::pin(d.find(id)).await {
            rt.ctx.lock().await.core.surface = Surface::AgentHost;
        }
    }
    Ok(v)
}
