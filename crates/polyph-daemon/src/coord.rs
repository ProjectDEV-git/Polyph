//! Coordination for agents on this device: the Polyph MCP tools (claims,
//! team state, inboxes, memory notes), inbox delivery at turn boundaries,
//! and the loop guard that keeps agents from waking each other forever.

use std::collections::{HashMap, VecDeque};
use std::path::{Component, Path};
use std::sync::Arc;
use std::time::Duration;

use polyph_core::InboxMessage;
use polyph_core::claims::normalize_path;
use polyph_core::memory::{self, MemoryNote, Placement};
use polyph_mcp::{ToolOutput, tools};
use polyph_proto::ids::ObjectId;
use polyph_proto::messages::{AgentEventKind, AgentRef, AgentState, ClaimTarget, DocName};
use polyph_sandbox::proxy::InternalRequest;
use serde::Deserialize;
use serde_json::{Value, json};
use tokio::sync::oneshot;

use crate::agents::AgentHandle;
use crate::daemon::Shared;
use crate::runtime::{RoomCtx, RoomRuntime, apply};

/// A message chain stops waking agents once it has passed between agents
/// this many times without a person speaking.
pub const MAX_HOPS: u8 = 4;
/// Messages one agent may send another within [`RATE_WINDOW`] seconds.
pub const MAX_PER_PAIR: usize = 8;
pub const RATE_WINDOW: i64 = 600;
/// Turns an agent may be woken for by other agents within an hour.
pub const MAX_WAKES_PER_HOUR: usize = 12;
/// Unread messages kept per agent; older ones are dropped first.
const INBOX_KEEP: usize = 500;
/// Messages kept after reading, so `reply` can find them.
const READ_KEEP: usize = 200;

/// One local agent's inbox.
#[derive(Debug, Default)]
pub struct Inbox {
    pub unread: VecDeque<InboxMessage>,
    pub read: VecDeque<InboxMessage>,
    /// Things Polyph itself must tell the agent at its next turn (revoked claims).
    pub notices: Vec<String>,
}

impl Inbox {
    fn take_unread(&mut self) -> Vec<InboxMessage> {
        let v: Vec<InboxMessage> = self.unread.drain(..).collect();
        for m in &v {
            self.read.push_back(m.clone());
        }
        while self.read.len() > READ_KEEP {
            self.read.pop_front();
        }
        v
    }
}

/// Per-room coordination state on this device.
#[derive(Debug, Default)]
pub struct Coord {
    pub inboxes: HashMap<ObjectId, Inbox>,
    /// Wakes an idle local agent when another agent writes to it.
    pub wakers: HashMap<ObjectId, tokio::sync::mpsc::UnboundedSender<()>>,
    /// `ask_agent` calls waiting for a reply, by the asked message's id.
    pub reply_waiters: HashMap<ObjectId, oneshot::Sender<InboxMessage>>,
    /// Send times per (from agent, to agent), for the rate limit.
    sent: HashMap<(ObjectId, ObjectId), VecDeque<i64>>,
    /// Wake times per agent.
    wakes: HashMap<ObjectId, VecDeque<i64>>,
    /// Unread messages for agents that are not running here now, by label,
    /// kept on disk so they survive restarts (loaded on first use).
    parked: Option<HashMap<String, Vec<InboxMessage>>>,
}

impl Coord {
    /// Routes a message for a local agent. Returns true if the agent was woken.
    pub fn deliver(&mut self, m: InboxMessage) -> bool {
        if let Some(r) = m.reply_to
            && let Some(tx) = self.reply_waiters.remove(&r)
        {
            match tx.send(m) {
                Ok(()) => return false,
                Err(m) => return self.deliver(m),
            }
        }
        let to = m.to.id;
        let from_agent = m.from_agent.is_some();
        let hops = m.hops;
        let inbox = self.inboxes.entry(to).or_default();
        inbox.unread.push_back(m);
        while inbox.unread.len() > INBOX_KEEP {
            inbox.unread.pop_front();
        }
        // People's messages wait for the next turn: waking someone else's agent
        // with a person's words would be steering, which needs the owner.
        if from_agent
            && hops < MAX_HOPS
            && let Some(w) = self.wakers.get(&to)
        {
            let _ = w.send(());
            return true;
        }
        false
    }

    pub fn notice(&mut self, agent: ObjectId, text: String) {
        self.inboxes.entry(agent).or_default().notices.push(text);
    }

    /// Records a send from one agent to another; false when over the limit.
    fn allow_send(&mut self, from: ObjectId, to: ObjectId, now: i64) -> bool {
        let q = self.sent.entry((from, to)).or_default();
        while q.front().is_some_and(|t| now - t >= RATE_WINDOW) {
            q.pop_front();
        }
        if q.len() >= MAX_PER_PAIR {
            return false;
        }
        q.push_back(now);
        true
    }

    /// Records an automatic wake; false when the agent was woken too often.
    pub fn allow_wake(&mut self, agent: ObjectId, now: i64) -> bool {
        let q = self.wakes.entry(agent).or_default();
        while q.front().is_some_and(|t| now - t >= 3600) {
            q.pop_front();
        }
        if q.len() >= MAX_WAKES_PER_HOUR {
            return false;
        }
        q.push_back(now);
        true
    }
}

fn inbox_file(shared: &Shared, room: polyph_proto::ids::RoomId) -> std::path::PathBuf {
    shared.paths.home.join("inboxes").join(format!("{}.inbox", room.to_hex()))
}

fn load_parked<'a>(ctx: &'a mut RoomCtx, shared: &Shared) -> &'a mut HashMap<String, Vec<InboxMessage>> {
    let room = ctx.core.id;
    ctx.coord.parked.get_or_insert_with(|| {
        let aad = format!("inbox:{}", room.to_hex());
        std::fs::read(inbox_file(shared, room))
            .ok()
            .and_then(|b| shared.key.open(aad.as_bytes(), &b).ok())
            .and_then(|pt| polyph_proto::codec::from_cbor(&pt).ok())
            .unwrap_or_default()
    })
}

/// Writes every unread message of this room's agents to disk: those of
/// running agents and those parked for agents that are not running.
pub fn persist_inboxes(ctx: &mut RoomCtx, shared: &Shared) {
    let mut all = load_parked(ctx, shared).clone();
    for inbox in ctx.coord.inboxes.values() {
        for m in &inbox.unread {
            all.entry(m.to.label.clone()).or_default().push(m.clone());
        }
    }
    all.retain(|_, v| !v.is_empty());
    let room = ctx.core.id;
    let path = inbox_file(shared, room);
    if all.is_empty() {
        let _ = std::fs::remove_file(&path);
        return;
    }
    let aad = format!("inbox:{}", room.to_hex());
    let sealed = polyph_proto::codec::to_cbor(&all).ok().and_then(|b| shared.key.seal(aad.as_bytes(), &b).ok());
    if let Some(sealed) = sealed {
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let _ = crate::paths::write_private(&path, &sealed);
    }
}

/// Parks an agent's unread messages under its label (it is stopping).
pub async fn save_inbox(rt: &RoomRuntime, shared: &Shared, agent: &AgentRef) {
    let mut ctx = rt.ctx.lock().await;
    let unread: Vec<InboxMessage> =
        ctx.coord.inboxes.remove(&agent.id).map(|i| i.unread.into_iter().collect()).unwrap_or_default();
    if !unread.is_empty() {
        load_parked(&mut ctx, shared).entry(agent.label.clone()).or_default().extend(unread);
    }
    persist_inboxes(&mut ctx, shared);
}

/// Hands a starting agent the messages that waited for its label: parked
/// ones and ones that arrived for an earlier agent of that label.
pub async fn restore_inbox(rt: &RoomRuntime, shared: &Shared, agent: &AgentRef) {
    let mut ctx = rt.ctx.lock().await;
    let mut msgs = load_parked(&mut ctx, shared).remove(&agent.label).unwrap_or_default();
    let stale: Vec<ObjectId> = ctx
        .coord
        .inboxes
        .iter()
        .filter(|(id, i)| **id != agent.id && i.unread.iter().any(|m| m.to.label == agent.label))
        .map(|(id, _)| *id)
        .collect();
    for id in stale {
        if let Some(i) = ctx.coord.inboxes.remove(&id) {
            msgs.extend(i.unread);
        }
    }
    if msgs.is_empty() {
        return;
    }
    msgs.sort_by_key(|m| m.at);
    let inbox = ctx.coord.inboxes.entry(agent.id).or_default();
    for mut m in msgs {
        m.to = agent.clone();
        inbox.unread.push_back(m);
    }
    persist_inboxes(&mut ctx, shared);
}

/// Text appended to a prompt at a turn boundary: Polyph notices and unread
/// messages, which are then marked read. Returns the text and the highest hop
/// count among the messages.
pub fn turn_boundary(ctx: &mut RoomCtx, agent: ObjectId) -> (Option<String>, u8) {
    let Some(inbox) = ctx.coord.inboxes.get_mut(&agent) else { return (None, 0) };
    let notices = std::mem::take(&mut inbox.notices);
    let msgs = inbox.take_unread();
    if notices.is_empty() && msgs.is_empty() {
        return (None, 0);
    }
    let hops = msgs.iter().map(|m| m.hops).max().unwrap_or(0);
    let mut s = String::from("[Polyph]");
    for n in notices {
        s.push_str("\n- ");
        s.push_str(&n);
    }
    if !msgs.is_empty() {
        s.push_str("\nMessages for you (answer with the `reply` tool and the message id):");
        for m in &msgs {
            s.push_str(&format!("\n- [{}] from {}: {}", m.id.short(), sender_name(ctx, m), m.text));
        }
    }
    (Some(s), hops)
}

fn sender_name(ctx: &RoomCtx, m: &InboxMessage) -> String {
    match &m.from_agent {
        Some(a) => a.label.clone(),
        None => member_name(ctx, &m.from_member),
    }
}

fn member_name(ctx: &RoomCtx, m: &polyph_proto::ids::MemberId) -> String {
    if *m == ctx.core.my_member {
        return ctx.core.my_profile.display_name.clone();
    }
    ctx.core.peers.values().find(|p| p.member() == Some(*m)).map(|p| p.name()).unwrap_or_else(|| m.short())
}

/// Live agents in the room, from transcripts: (agent, state).
fn live_agents(ctx: &RoomCtx) -> Vec<(AgentRef, Option<AgentState>)> {
    let mut v = vec![];
    for log in ctx.transcripts.values() {
        let Some(first) = log.front() else { continue };
        let mut state = None;
        for e in log.iter().rev() {
            match &e.kind {
                AgentEventKind::State { state: s, .. } => {
                    state = Some(*s);
                    break;
                }
                AgentEventKind::Stopped { .. } => {
                    state = Some(AgentState::Stopped);
                    break;
                }
                _ => {}
            }
        }
        if state != Some(AgentState::Stopped) {
            v.push((first.agent.clone(), state));
        }
    }
    v.sort_by(|a, b| a.0.label.cmp(&b.0.label));
    v
}

/// Finds an agent by label, by owner name (if they run one agent), or by kind (if unique).
fn resolve_agent(ctx: &RoomCtx, me: &AgentRef, to: &str) -> Result<AgentRef, String> {
    let to = to.trim().trim_start_matches('@').to_lowercase();
    let others: Vec<AgentRef> = live_agents(ctx).into_iter().map(|(a, _)| a).filter(|a| a.id != me.id).collect();
    if let Some(a) = others.iter().find(|a| a.label.to_lowercase() == to) {
        return Ok(a.clone());
    }
    let by_owner: Vec<&AgentRef> = others.iter().filter(|a| member_name(ctx, &a.member).to_lowercase() == to).collect();
    let by_kind: Vec<&AgentRef> = others.iter().filter(|a| a.kind.to_lowercase() == to).collect();
    for set in [by_owner, by_kind] {
        match set.len() {
            1 => return Ok(set[0].clone()),
            0 => {}
            _ => {
                let labels: Vec<&str> = set.iter().map(|a| a.label.as_str()).collect();
                return Err(format!("\"{to}\" matches several agents: {}. Use a full label.", labels.join(", ")));
            }
        }
    }
    let labels: Vec<&str> = others.iter().map(|a| a.label.as_str()).collect();
    Err(if labels.is_empty() {
        "no other agent is running in this room".to_string()
    } else {
        format!("no agent \"{to}\". Agents in the room: {}", labels.join(", "))
    })
}

/// Repository-relative form of a path an agent gave, or an error for paths
/// outside its worktree.
pub(crate) fn rel_path(worktree: &Path, p: &str) -> Result<String, String> {
    let (path, symbol) = match p.split_once('#') {
        Some((a, s)) => (a, Some(s)),
        None => (p, None),
    };
    let dir = path.ends_with('/') || path.ends_with('\\');
    let pb = Path::new(path);
    let rel = if pb.is_absolute() {
        match pb.strip_prefix(worktree) {
            Ok(r) => r.to_path_buf(),
            Err(_) => return Err(format!("{p} is outside your worktree")),
        }
    } else {
        pb.to_path_buf()
    };
    if rel.components().any(|c| matches!(c, Component::ParentDir | Component::RootDir | Component::Prefix(_))) {
        return Err(format!("{p} is outside your worktree"));
    }
    let mut s = normalize_path(&rel.to_string_lossy());
    if dir && !s.is_empty() && !s.ends_with('/') {
        s.push('/');
    }
    if s.is_empty() {
        return Err("empty path".into());
    }
    if let Some(sym) = symbol {
        s = format!("{s}#{sym}");
    }
    Ok(s)
}

#[derive(Deserialize)]
struct Call {
    tool: String,
    #[serde(default)]
    args: Value,
}

fn arg<T: for<'de> Deserialize<'de>>(args: &Value) -> Result<T, ToolOutput> {
    serde_json::from_value(args.clone()).map_err(|e| ToolOutput::err(format!("bad arguments: {e}")))
}

/// Answers a request that arrived on an agent's proxy for the internal host.
pub async fn internal(
    h: Arc<AgentHandle>,
    rt: Arc<RoomRuntime>,
    shared: Arc<Shared>,
    r: InternalRequest,
) -> (u16, Vec<u8>) {
    let expected = format!("Bearer {}", h.mcp_token);
    if r.auth.as_deref().map(|a| constant_eq(a.as_bytes(), expected.as_bytes())) != Some(true) {
        return (401, br#"{"error":"bad token"}"#.to_vec());
    }
    if r.method != "POST" || r.path != polyph_mcp::CALL_PATH {
        return (404, br#"{"error":"no such endpoint"}"#.to_vec());
    }
    let call: Call = match serde_json::from_slice(&r.body) {
        Ok(c) => c,
        Err(_) => return (400, br#"{"error":"bad request"}"#.to_vec()),
    };
    let out = call_tool(&h.caller(), &rt, &shared, &call.tool, &call.args).await;
    (200, serde_json::to_vec(&out).unwrap_or_default())
}

fn constant_eq(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

pub async fn call_tool(h: &Caller, rt: &RoomRuntime, shared: &Shared, tool: &str, args: &Value) -> ToolOutput {
    let r = match tool {
        tools::CLAIM => claim(h, rt, shared, args).await,
        tools::RELEASE => release(h, rt, shared, args).await,
        tools::CLAIMS => Ok(list_claims(&*rt.ctx.lock().await, shared.clock.now())),
        tools::TEAM => Ok(team_state(&*rt.ctx.lock().await, shared.clock.now())),
        tools::ASK => ask(h, rt, shared, args).await,
        tools::REPLY => reply(h, rt, shared, args).await,
        tools::INBOX => Ok(read_inbox(h, rt, shared).await),
        tools::REMEMBER => remember(h, rt, shared, args).await,
        tools::RECALL => recall(h, rt, args).await,
        tools::FORGET => forget(rt, shared, args).await,
        tools::TASKS => list_tasks(h, rt, args).await,
        tools::CREATE_TASK => create_task(h, rt, shared, args).await,
        tools::MOVE_TASK => move_task(h, rt, shared, args).await,
        tools::FINISH if h.external => finish_external(h, rt, shared, args).await,
        tools::FINISH => finish_task(h, shared, args).await,
        tools::WHITEBOARDS => {
            let mut ctx = rt.ctx.lock().await;
            let all = ctx.core.whiteboards();
            let text = if all.is_empty() {
                "No whiteboards yet.".to_string()
            } else {
                let mut t = String::from("Whiteboards:");
                for w in &all {
                    t.push_str(&format!("\n- [{}] {}", &w.id[..8.min(w.id.len())], w.name));
                }
                t
            };
            Ok(ToolOutput::ok(text, json!({ "whiteboards": all })))
        }
        tools::READ_WB => read_whiteboard(rt, args).await,
        tools::DRAW => draw_diagram(h, rt, shared, args).await,
        tools::SKILLS => Ok(list_skills(h, shared)),
        tools::CONTEXT => crate::context::get_context(h, rt, shared, args).await,
        tools::SEARCH => crate::context::search_tool(&mut *rt.ctx.lock().await, args),
        tools::RECENT => Ok(crate::context::recent_tool(&mut *rt.ctx.lock().await, args)),
        tools::NEXT_TASK => crate::pickup::next_task(h, rt, shared).await,
        tools::READ_SKILL => read_skill(h, shared, args),
        _ => Err(ToolOutput::err(format!("unknown tool {tool}"))),
    };
    r.unwrap_or_else(|e| e)
}

type ToolResult = Result<ToolOutput, ToolOutput>;

/// Who is calling a tool: an agent Polyph started, or one the person runs
/// themselves and connected with `polyph mcp` (see `connect`).
#[derive(Debug, Clone)]
pub struct Caller {
    pub agent: AgentRef,
    pub task: Option<ObjectId>,
    /// Folder paths are checked against and relative to.
    pub worktree: std::path::PathBuf,
    /// Hops of the agent message that started the current turn.
    pub hops: u8,
    /// Connected from outside: no task worktree and no turn boundaries, so
    /// Polyph cannot commit for it or hand it messages between turns.
    pub external: bool,
}

impl AgentHandle {
    pub fn caller(&self) -> Caller {
        Caller {
            agent: self.agent.clone(),
            task: self.task,
            worktree: self.worktree.clone(),
            hops: self.trigger_hops.load(std::sync::atomic::Ordering::Relaxed),
            external: false,
        }
    }
}

async fn claim(h: &Caller, rt: &RoomRuntime, shared: &Shared, args: &Value) -> ToolResult {
    #[derive(Deserialize)]
    struct A {
        paths: Vec<String>,
        reason: String,
    }
    let a: A = arg(args)?;
    if a.paths.is_empty() || a.paths.len() > 100 {
        return Err(ToolOutput::err("give between 1 and 100 paths"));
    }
    let targets: Vec<ClaimTarget> = a
        .paths
        .iter()
        .map(|p| rel_path(&h.worktree, p).map(|s| ClaimTarget::parse(&s)))
        .collect::<Result<_, _>>()
        .map_err(ToolOutput::err)?;
    let (granted, denied) = request_claims(h, rt, shared, &targets, &a.reason).await?;
    let mut text = String::new();
    if !granted.is_empty() {
        let t: Vec<String> = granted.iter().map(|c| c.target.to_string()).collect();
        text.push_str(&format!("Claimed: {}.", t.join(", ")));
    }
    if !denied.is_empty() {
        let t: Vec<String> =
            denied.iter().map(|c| format!("{} (held by {}: {})", c.target, c.owner.label, c.reason)).collect();
        if !text.is_empty() {
            text.push(' ');
        }
        text.push_str(&format!(
            "Not claimed, because someone else holds them: {}. Ask the holder with ask_agent, or work on something else.",
            t.join("; ")
        ));
    }
    // Areas are advice: say when a file is someone else's to look after.
    let areas = crate::areas::load(&h.worktree);
    let mut others: Vec<String> = vec![];
    for c in &granted {
        if let Some(a) = areas.owner_of(&c.target.path)
            && !a.owners.is_empty()
            && !a.owned_by(&h.agent.label)
        {
            others.push(format!("{} is in {}", c.target.path, a.describe()));
        }
    }
    if !others.is_empty() {
        text.push_str(&format!(
            " Note: {}. Tell its owners what you change, with ask_agent or a note.",
            others.join("; ")
        ));
    }
    let data = json!({
        "areas": others,
        "granted": granted.iter().map(|c| c.target.to_string()).collect::<Vec<_>>(),
        "denied": denied.iter().map(|c| json!({ "target": c.target.to_string(), "holder": c.owner.label, "reason": c.reason })).collect::<Vec<_>>(),
    });
    Ok(ToolOutput { text, data, error: false })
}

/// Asks the room's claim arbiter for `targets` and waits for its answer.
pub(crate) async fn request_claims(
    h: &Caller,
    rt: &RoomRuntime,
    shared: &Shared,
    targets: &[ClaimTarget],
    reason: &str,
) -> Result<(Vec<polyph_proto::messages::Claim>, Vec<polyph_proto::messages::Claim>), ToolOutput> {
    let rx = {
        let mut ctx = rt.ctx.lock().await;
        let now = shared.clock.now();
        let (req, fx) = ctx
            .core
            .request_claims(&h.agent, targets, reason, h.task, polyph_core::room::DEFAULT_LEASE, now)
            .map_err(|e| ToolOutput::err(e.to_string()))?;
        let (tx, rx) = oneshot::channel();
        ctx.claim_waiters.insert(req, tx);
        apply(rt, &mut ctx, shared, fx).await;
        rx
    };
    match tokio::time::timeout(Duration::from_secs(15), rx).await {
        Ok(Ok(v)) => Ok(v),
        _ => Err(ToolOutput::err("the claim arbiter did not answer; try again in a moment")),
    }
}

async fn release(h: &Caller, rt: &RoomRuntime, shared: &Shared, args: &Value) -> ToolResult {
    #[derive(Deserialize)]
    struct A {
        paths: Option<Vec<String>>,
    }
    let a: A = arg(args)?;
    let wanted: Option<Vec<ClaimTarget>> = match a.paths {
        Some(p) if !p.is_empty() => Some(
            p.iter()
                .map(|x| rel_path(&h.worktree, x).map(|s| ClaimTarget::parse(&s)))
                .collect::<Result<_, _>>()
                .map_err(ToolOutput::err)?,
        ),
        _ => None,
    };
    let mut ctx = rt.ctx.lock().await;
    let mine: Vec<_> = ctx
        .core
        .claims
        .all()
        .into_iter()
        .filter(|c| c.owner.id == h.agent.id)
        // Releasing everything keeps the task leases from next_task, which
        // finishing the task releases.
        .filter(|c| match &wanted {
            Some(w) => w.contains(&c.target),
            None => !c.target.path.starts_with(crate::pickup::TASK_PREFIX),
        })
        .collect();
    let ids: Vec<ObjectId> = mine.iter().map(|c| c.id).collect();
    let fx = ctx.core.release_claims(&ids);
    apply(rt, &mut ctx, shared, fx).await;
    let names: Vec<String> = mine.iter().map(|c| c.target.to_string()).collect();
    let text = if names.is_empty() {
        "You held none of those.".to_string()
    } else {
        format!("Released: {}.", names.join(", "))
    };
    Ok(ToolOutput::ok(text, json!({ "released": names })))
}

fn list_claims(ctx: &RoomCtx, now: i64) -> ToolOutput {
    let all: Vec<_> = ctx.core.claims.all().into_iter().filter(|c| c.expires_at > now).collect();
    if all.is_empty() {
        return ToolOutput::ok("Nobody holds any claims.", json!({ "claims": [] }));
    }
    let mut text = String::from("Claims:");
    for c in &all {
        text.push_str(&format!(
            "\n- {} held by {} ({}), {} s left",
            c.target,
            c.owner.label,
            c.reason,
            c.expires_at - now
        ));
    }
    let data: Vec<Value> = all
        .iter()
        .map(|c| json!({ "target": c.target.to_string(), "holder": c.owner.label, "reason": c.reason, "expires_in": c.expires_at - now }))
        .collect();
    ToolOutput::ok(text, json!({ "claims": data }))
}

/// The team-state digest an agent reads before working.
pub fn team_state(ctx: &RoomCtx, now: i64) -> ToolOutput {
    let mut people: Vec<(String, String, bool)> =
        vec![(ctx.core.my_profile.display_name.clone(), format!("{:?}", ctx.core.role()).to_lowercase(), true)];
    let mut seen = std::collections::HashSet::new();
    seen.insert(ctx.core.my_member);
    for p in ctx.core.peers.values() {
        let Some(m) = p.member() else { continue };
        let online = p.online(now);
        if seen.insert(m) {
            people.push((p.name(), format!("{:?}", p.role()).to_lowercase(), online));
        } else if online && let Some(e) = people.iter_mut().find(|e| e.0 == p.name()) {
            e.2 = true;
        }
    }
    let claims = ctx.core.claims.all();
    let agents = live_agents(ctx);
    let mut text = format!("Room \"{}\".\nPeople:", ctx.core.name);
    for (name, role, online) in &people {
        text.push_str(&format!("\n- {name} ({role}), {}", if *online { "online" } else { "offline" }));
    }
    text.push_str("\nAgents:");
    if agents.is_empty() {
        text.push_str(" none");
    }
    let mut agent_data = vec![];
    for (a, state) in &agents {
        let held: Vec<String> =
            claims.iter().filter(|c| c.owner.id == a.id && c.expires_at > now).map(|c| c.target.to_string()).collect();
        let state_s = state.map(|s| format!("{s:?}").to_lowercase()).unwrap_or_else(|| "starting".into());
        text.push_str(&format!("\n- {} ({}), {}", a.label, a.kind, state_s));
        if !held.is_empty() {
            text.push_str(&format!(", holds {}", held.join(", ")));
        }
        agent_data.push(json!({ "label": a.label, "kind": a.kind, "state": state_s, "claims": held }));
    }
    let pending = ctx.core.approvals.len();
    if pending > 0 {
        text.push_str(&format!("\n{pending} approval(s) waiting for a person."));
    }
    let data = json!({
        "room": ctx.core.name,
        "people": people.iter().map(|(n, r, o)| json!({ "name": n, "role": r, "online": o })).collect::<Vec<_>>(),
        "agents": agent_data,
        "approvals_pending": pending,
    });
    ToolOutput::ok(text, data)
}

async fn send(
    h: &Caller,
    rt: &RoomRuntime,
    shared: &Shared,
    to: AgentRef,
    reply_to: Option<ObjectId>,
    text: &str,
    waiter: Option<oneshot::Sender<InboxMessage>>,
) -> Result<(InboxMessage, bool), ToolOutput> {
    let hops = h.hops.saturating_add(1);
    let mut ctx = rt.ctx.lock().await;
    let now = shared.clock.now();
    if !ctx.coord.allow_send(h.agent.id, to.id, now) {
        return Err(ToolOutput::err(format!(
            "you have sent {} messages to {} in the last {} minutes; wait for an answer or ask a person",
            MAX_PER_PAIR,
            to.label,
            RATE_WINDOW / 60
        )));
    }
    let (m, fx) = ctx
        .core
        .send_inbox(to, Some(h.agent.clone()), None, reply_to, text, hops, now)
        .map_err(|e| ToolOutput::err(e.to_string()))?;
    if let Some(w) = waiter {
        ctx.coord.reply_waiters.insert(m.id, w);
    }
    apply(rt, &mut ctx, shared, fx).await;
    Ok((m, hops < MAX_HOPS))
}

async fn ask(h: &Caller, rt: &RoomRuntime, shared: &Shared, args: &Value) -> ToolResult {
    #[derive(Deserialize)]
    struct A {
        to: String,
        message: String,
        wait_seconds: Option<u64>,
    }
    let a: A = arg(args)?;
    let to = resolve_agent(&*rt.ctx.lock().await, &h.agent, &a.to).map_err(ToolOutput::err)?;
    let wait = a.wait_seconds.unwrap_or(0).min(300);
    let (tx, rx) = oneshot::channel();
    let (m, wakes) = send(h, rt, shared, to.clone(), None, &a.message, (wait > 0).then_some(tx)).await?;
    let mut text = format!("Sent to {} (message {}).", to.label, m.id.short());
    if !wakes {
        text.push_str(" This exchange has gone back and forth between agents too often, so Polyph will not wake them for it; they will see it at their next turn. Consider asking a person.");
    }
    if wait == 0 {
        text.push_str(" The reply will arrive in your inbox.");
        return Ok(ToolOutput::ok(text, json!({ "id": m.id.to_hex(), "to": to.label })));
    }
    match tokio::time::timeout(Duration::from_secs(wait), rx).await {
        Ok(Ok(r)) => {
            let from = r.from_agent.as_ref().map(|a| a.label.clone()).unwrap_or_default();
            Ok(ToolOutput::ok(
                format!("{text}\nReply from {from}: {}", r.text),
                json!({ "id": m.id.to_hex(), "to": to.label, "reply": { "id": r.id.to_hex(), "text": r.text } }),
            ))
        }
        _ => {
            rt.ctx.lock().await.coord.reply_waiters.remove(&m.id);
            text.push_str(&format!(" No reply within {wait} s; it will arrive in your inbox."));
            Ok(ToolOutput::ok(text, json!({ "id": m.id.to_hex(), "to": to.label, "reply": null })))
        }
    }
}

async fn reply(h: &Caller, rt: &RoomRuntime, shared: &Shared, args: &Value) -> ToolResult {
    #[derive(Deserialize)]
    struct A {
        message_id: String,
        text: String,
    }
    let a: A = arg(args)?;
    let want = a.message_id.trim().trim_start_matches('[').trim_end_matches(']').to_lowercase();
    let original = {
        let ctx = rt.ctx.lock().await;
        ctx.coord.inboxes.get(&h.agent.id).and_then(|i| {
            i.read.iter().chain(i.unread.iter()).rev().find(|m| m.id.to_hex().starts_with(&want)).cloned()
        })
    };
    let Some(original) = original else {
        return Err(ToolOutput::err(format!("no message {} in your inbox", a.message_id)));
    };
    let Some(to) = original.from_agent.clone() else {
        // A person wrote it: answer in the room chat thread of the agent's transcript.
        return Err(ToolOutput::err(
            "that message came from a person; answer them in your normal output, which they can read",
        ));
    };
    let (m, _) = send(h, rt, shared, to.clone(), Some(original.id), &a.text, None).await?;
    Ok(ToolOutput::ok(format!("Replied to {}.", to.label), json!({ "id": m.id.to_hex() })))
}

async fn read_inbox(h: &Caller, rt: &RoomRuntime, shared: &Shared) -> ToolOutput {
    let mut ctx = rt.ctx.lock().await;
    let mut msgs = ctx.coord.inboxes.get_mut(&h.agent.id).map(|i| i.take_unread()).unwrap_or_default();
    let notices = ctx.coord.inboxes.get_mut(&h.agent.id).map(|i| std::mem::take(&mut i.notices)).unwrap_or_default();
    if !msgs.is_empty() {
        persist_inboxes(&mut ctx, shared);
    }
    if msgs.is_empty() && notices.is_empty() {
        return ToolOutput::ok("No new messages.", json!({ "messages": [] }));
    }
    let mut text = String::from("New messages:");
    for n in &notices {
        text.push_str(&format!("\n- [Polyph] {n}"));
    }
    let mut data = vec![];
    msgs.sort_by_key(|m| m.at);
    for m in &msgs {
        let from = sender_name(&ctx, m);
        text.push_str(&format!("\n- [{}] from {from}: {}", m.id.short(), m.text));
        data.push(
            json!({ "id": m.id.to_hex(), "from": from, "text": m.text, "reply_to": m.reply_to.map(|r| r.to_hex()) }),
        );
    }
    ToolOutput::ok(text, json!({ "messages": data }))
}

fn read_text(worktree: &Path, rel: &str) -> Option<String> {
    std::fs::read_to_string(worktree.join(rel)).ok()
}

async fn remember(h: &Caller, rt: &RoomRuntime, shared: &Shared, args: &Value) -> ToolResult {
    #[derive(Deserialize)]
    struct A {
        path: String,
        line: Option<u32>,
        note: String,
    }
    let a: A = arg(args)?;
    if a.note.trim().is_empty() || a.note.len() > memory::MAX_NOTE {
        return Err(ToolOutput::err(format!("a note must be 1 to {} bytes", memory::MAX_NOTE)));
    }
    let path = rel_path(&h.worktree, &a.path).map_err(ToolOutput::err)?;
    let path = path.split('#').next().unwrap_or_default().to_string();
    let anchor = match a.line {
        Some(l) => {
            let content = read_text(&h.worktree, &path)
                .ok_or_else(|| ToolOutput::err(format!("cannot read {path} to pin a line")))?;
            Some(memory::make_anchor(&content, l).ok_or_else(|| ToolOutput::err(format!("{path} has no line {l}")))?)
        }
        None => None,
    };
    let id = ObjectId::random().map_err(|e| ToolOutput::err(e.to_string()))?.to_hex();
    let note = MemoryNote {
        id: id.clone(),
        path: path.clone(),
        line: a.line,
        anchor,
        text: polyph_core::redact::redact(a.note.trim()).into_owned(),
        author: h.agent.member.to_hex(),
        agent: Some(h.agent.label.clone()),
        at: shared.clock.now(),
    };
    let mut ctx = rt.ctx.lock().await;
    let fx = ctx
        .core
        .change_doc(DocName::Memory, |d| memory::put_note(d, &note))
        .map_err(|e| ToolOutput::err(e.to_string()))?;
    apply(rt, &mut ctx, shared, fx).await;
    let at = a.line.map(|l| format!("{path}:{l}")).unwrap_or(path);
    Ok(ToolOutput::ok(format!("Pinned note {} to {at}.", &id[..8]), json!({ "id": id })))
}

/// Notes for `path` placed against the files in `worktree`.
pub fn placed_notes(all: &[MemoryNote], path: &str, worktree: &Path) -> Vec<(MemoryNote, Placement)> {
    memory::notes_for(all, path)
        .into_iter()
        .map(|n| {
            let content = if n.line.is_some() { read_text(worktree, &n.path) } else { None };
            let p = memory::place(&n, content.as_deref());
            (n, p)
        })
        .collect()
}

fn list_skills(h: &Caller, shared: &Shared) -> ToolOutput {
    let all = polyph_plugins::skills::discover(Some(&h.worktree), &shared.paths.home.join("skills"));
    if all.is_empty() {
        return ToolOutput::ok("No skills in this project.", json!({ "skills": [] }));
    }
    let mut text = String::from("Skills (read with read_skill):");
    for s in &all {
        text.push_str(&format!("\n- {}: {}", s.name, s.description));
    }
    ToolOutput::ok(text, json!({ "skills": all }))
}

fn read_skill(h: &Caller, shared: &Shared, args: &Value) -> ToolResult {
    #[derive(Deserialize)]
    struct A {
        name: String,
    }
    let a: A = arg(args)?;
    let (s, body) = polyph_plugins::skills::read(Some(&h.worktree), &shared.paths.home.join("skills"), &a.name)
        .map_err(|e| ToolOutput::err(e.to_string()))?;
    Ok(ToolOutput::ok(body, json!({ "name": s.name, "source": s.source })))
}

async fn recall(h: &Caller, rt: &RoomRuntime, args: &Value) -> ToolResult {
    #[derive(Deserialize)]
    struct A {
        path: Option<String>,
    }
    let a: A = arg(args)?;
    let path = match a.path.as_deref().filter(|p| !p.trim().is_empty()) {
        Some(p) => rel_path(&h.worktree, p).map_err(ToolOutput::err)?.split('#').next().unwrap_or_default().to_string(),
        None => String::new(),
    };
    let all = memory::notes(rt.ctx.lock().await.core.doc(&DocName::Memory));
    let placed = placed_notes(&all, &path, &h.worktree);
    if placed.is_empty() {
        return Ok(ToolOutput::ok("No notes.", json!({ "notes": [] })));
    }
    let mut text = String::from("Notes:");
    let mut data = vec![];
    for (n, p) in &placed {
        let at = match p {
            Placement::File => n.path.clone(),
            Placement::Exact(l) => format!("{}:{l}", n.path),
            Placement::Moved(l) => format!("{}:{l} (moved from line {})", n.path, n.line.unwrap_or(0)),
            Placement::Lost => format!("{} (the pinned line is gone; this note may be out of date)", n.path),
        };
        let by = n.agent.clone().unwrap_or_else(|| n.author.chars().take(8).collect());
        text.push_str(&format!("\n- [{}] {at}, by {by}: {}", &n.id[..n.id.len().min(8)], n.text));
        data.push(json!({ "id": n.id, "path": n.path, "placement": p, "text": n.text, "by": by }));
    }
    Ok(ToolOutput::ok(text, json!({ "notes": data })))
}

async fn forget(rt: &RoomRuntime, shared: &Shared, args: &Value) -> ToolResult {
    #[derive(Deserialize)]
    struct A {
        id: String,
    }
    let a: A = arg(args)?;
    let want = a.id.trim().trim_start_matches('[').trim_end_matches(']').to_lowercase();
    let mut ctx = rt.ctx.lock().await;
    let all = memory::notes(ctx.core.doc(&DocName::Memory));
    let found: Vec<&MemoryNote> = all.iter().filter(|n| !want.is_empty() && n.id.starts_with(&want)).collect();
    let id = match found.as_slice() {
        [n] => n.id.clone(),
        [] => return Err(ToolOutput::err(format!("no note {}", a.id))),
        _ => return Err(ToolOutput::err("that id matches several notes; give more of it")),
    };
    let fx = ctx
        .core
        .change_doc(DocName::Memory, |d| {
            memory::remove_note(d, &id);
        })
        .map_err(|e| ToolOutput::err(e.to_string()))?;
    apply(rt, &mut ctx, shared, fx).await;
    Ok(ToolOutput::ok(format!("Deleted note {}.", &id[..8.min(id.len())]), json!({ "id": id })))
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    #[test]
    fn paths_stay_inside_the_worktree() {
        // An absolute worktree in this platform's own form.
        let w = Path::new(if cfg!(windows) { r"C:\w\repo" } else { "/w/repo" });
        let inside = format!("{}#login", w.join("src").join("a.rs").display());
        assert_eq!(rel_path(w, "src/a.rs").unwrap(), "src/a.rs");
        assert_eq!(rel_path(w, "./src//b/").unwrap(), "src/b/");
        assert_eq!(rel_path(w, &inside).unwrap(), "src/a.rs#login");
        assert!(rel_path(w, "../other/a.rs").is_err());
        assert!(rel_path(w, "/etc/passwd").is_err());
        assert!(rel_path(w, "src/../../x").is_err());
    }

    fn msg(hops: u8, from_agent: bool, reply_to: Option<ObjectId>) -> InboxMessage {
        let a = |n: u8| AgentRef {
            id: ObjectId([n; 16]),
            member: polyph_proto::ids::MemberId([n; 16]),
            device: polyph_proto::ids::DeviceId([n; 32]),
            kind: "fake".into(),
            label: format!("a{n}"),
        };
        InboxMessage {
            id: ObjectId::random().unwrap(),
            to: a(1),
            from_member: a(2).member,
            from_agent: from_agent.then(|| a(2)),
            thread: None,
            reply_to,
            text: "hi".into(),
            at: 0,
            hops,
        }
    }

    #[test]
    fn loop_guard() {
        let mut c = Coord::default();
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        c.wakers.insert(ObjectId([1; 16]), tx);
        assert!(c.deliver(msg(1, true, None)));
        assert!(!c.deliver(msg(MAX_HOPS, true, None)), "too many hops: queued, not woken");
        assert!(!c.deliver(msg(0, false, None)), "people's messages wait for the next turn");
        assert_eq!(c.inboxes[&ObjectId([1; 16])].unread.len(), 3);
        assert!(rx.try_recv().is_ok() && rx.try_recv().is_err());
        // A reply someone is waiting for goes to the waiter, not the inbox.
        let asked = ObjectId([9; 16]);
        let (wtx, mut wrx) = oneshot::channel();
        c.reply_waiters.insert(asked, wtx);
        c.deliver(msg(2, true, Some(asked)));
        assert!(wrx.try_recv().is_ok());
        assert_eq!(c.inboxes[&ObjectId([1; 16])].unread.len(), 3);
        // Rate limits.
        let (a, b) = (ObjectId([1; 16]), ObjectId([2; 16]));
        for _ in 0..MAX_PER_PAIR {
            assert!(c.allow_send(a, b, 100));
        }
        assert!(!c.allow_send(a, b, 100));
        assert!(c.allow_send(a, b, 100 + RATE_WINDOW));
        for _ in 0..MAX_WAKES_PER_HOUR {
            assert!(c.allow_wake(a, 0));
        }
        assert!(!c.allow_wake(a, 10));
    }
}

async fn list_tasks(h: &Caller, rt: &RoomRuntime, args: &Value) -> ToolResult {
    #[derive(Deserialize)]
    struct A {
        column: Option<String>,
    }
    let a: A = arg(args)?;
    let cards = {
        let mut ctx = rt.ctx.lock().await;
        polyph_proto::crdt::Board::new(ctx.core.doc(&DocName::Board)).all_cards()
    };
    let mine = h.task.map(|t| t.to_hex());
    let shown: Vec<_> = cards
        .iter()
        .filter(|c| !c.draft)
        .filter(|c| match &a.column {
            Some(col) => &c.column == col,
            None => c.column != "done",
        })
        .collect();
    if shown.is_empty() {
        return Ok(ToolOutput::ok("No tasks.", json!({ "tasks": [] })));
    }
    let short = |id: &str| id.chars().take(8).collect::<String>();
    let title_of = |id: &str| cards.iter().find(|c| c.id == id).map(|c| c.title.clone()).unwrap_or_else(|| short(id));
    let mut text = String::from("Tasks:");
    let mut data = vec![];
    for c in &shown {
        let me = mine.as_deref() == Some(c.id.as_str());
        text.push_str(&format!("\n- [{}] {} ({})", short(&c.id), c.title, c.column));
        if let Some(a) = &c.assignee {
            text.push_str(&format!(", {a}"));
        }
        if me {
            text.push_str(", your task");
        }
        let waiting: Vec<String> = c
            .depends_on
            .iter()
            .filter(|d| cards.iter().find(|x| &x.id == *d).is_none_or(|x| x.column != "done"))
            .map(|d| title_of(d))
            .collect();
        if !waiting.is_empty() {
            text.push_str(&format!(", waits for: {}", waiting.join("; ")));
        }
        if !c.files.is_empty() {
            text.push_str(&format!(", files: {}", c.files.join(", ")));
        }
        data.push(json!({ "id": c.id, "title": c.title, "column": c.column, "assignee": c.assignee, "depends_on": c.depends_on, "files": c.files, "mine": me }));
    }
    Ok(ToolOutput::ok(text, json!({ "tasks": data })))
}

async fn create_task(h: &Caller, rt: &RoomRuntime, shared: &Shared, args: &Value) -> ToolResult {
    #[derive(Deserialize)]
    struct A {
        title: String,
        #[serde(default)]
        description: String,
        #[serde(default)]
        acceptance: Vec<String>,
        #[serde(default)]
        files: Vec<String>,
    }
    let a: A = arg(args)?;
    if a.title.trim().is_empty() || a.title.len() > 200 {
        return Err(ToolOutput::err("a title must be 1 to 200 bytes"));
    }
    let red = |s: &str| polyph_core::redact::redact(s).into_owned();
    let id = ObjectId::random().map_err(|e| ToolOutput::err(e.to_string()))?.to_hex();
    let card = polyph_proto::crdt::NewCard {
        id: id.clone(),
        title: red(a.title.trim()),
        description: format!("{}\n\n(added by {})", red(&a.description), h.agent.label).trim().to_string(),
        column: "backlog".into(),
        created_by: h.agent.member.to_hex(),
        created_at: shared.clock.now(),
        acceptance: a.acceptance.iter().map(|x| red(x)).collect(),
        files: a.files,
        kind: Some("task".into()),
        ..Default::default()
    };
    let mut ctx = rt.ctx.lock().await;
    let fx = ctx
        .core
        .change_doc_as(DocName::Board, Some(&h.agent.label), |d| {
            let _ = polyph_proto::crdt::Board::new(d).add_card(&card);
        })
        .map_err(|e| ToolOutput::err(e.to_string()))?;
    apply(rt, &mut ctx, shared, fx).await;
    Ok(ToolOutput::ok(format!("Added task [{}] to the backlog.", &id[..8]), json!({ "id": id })))
}

async fn move_task(h: &Caller, rt: &RoomRuntime, shared: &Shared, args: &Value) -> ToolResult {
    #[derive(Deserialize)]
    struct A {
        id: String,
        column: String,
    }
    let a: A = arg(args)?;
    if !["backlog", "ready", "claimed", "in_progress"].contains(&a.column.as_str()) {
        return Err(ToolOutput::err("agents can move cards to backlog, ready, claimed or in_progress only"));
    }
    let want = a.id.trim().trim_start_matches('[').trim_end_matches(']').to_lowercase();
    let now = shared.clock.now();
    let mut ctx = rt.ctx.lock().await;
    let cards = polyph_proto::crdt::Board::new(ctx.core.doc(&DocName::Board)).all_cards();
    let found: Vec<_> = cards.iter().filter(|c| !want.is_empty() && c.id.starts_with(&want)).collect();
    let c = match found.as_slice() {
        [c] => (*c).clone(),
        [] => return Err(ToolOutput::err(format!("no task {}", a.id))),
        _ => return Err(ToolOutput::err("that id matches several tasks; give more of it")),
    };
    if c.assignee.as_deref().is_some_and(|x| x != h.agent.label) {
        return Err(ToolOutput::err(format!("that task belongs to {}", c.assignee.unwrap_or_default())));
    }
    if c.column == "review" || c.column == "done" {
        return Err(ToolOutput::err("that task is in review or done; the merge queue moves it"));
    }
    let fx = ctx
        .core
        .change_doc_as(DocName::Board, Some(&h.agent.label), |d| {
            let b = polyph_proto::crdt::Board::new(d);
            let last = b.cards_in(&a.column).last().map(|x| x.id.clone());
            let _ = b.move_card(&c.id, &a.column, last.as_deref(), now);
        })
        .map_err(|e| ToolOutput::err(e.to_string()))?;
    apply(rt, &mut ctx, shared, fx).await;
    Ok(ToolOutput::ok(format!("Moved \"{}\" to {}.", c.title, a.column), json!({ "id": c.id, "column": a.column })))
}

async fn finish_task(h: &Caller, shared: &Shared, args: &Value) -> ToolResult {
    #[derive(Deserialize)]
    struct A {
        summary: String,
    }
    let a: A = arg(args)?;
    // Through the daemon's worker: finishing can start reviewer agents, whose
    // own MCP handlers lead back here, so this cannot call it directly.
    let tx = shared.finisher.get().ok_or_else(|| ToolOutput::err("Polyph is starting or shutting down"))?;
    let (reply, rx) = oneshot::channel();
    tx.send(FinishJob { agent: h.agent.id, message: Some(a.summary), reply })
        .map_err(|_| ToolOutput::err("Polyph is shutting down"))?;
    let r = rx
        .await
        .map_err(|_| ToolOutput::err("Polyph is shutting down"))?
        .map_err(|e| ToolOutput::err(e.to_string()))?;
    Ok(ToolOutput::ok(
        format!(
            "Committed {} commit(s) on {} and queued it to merge into {}. Your claims are released. You are done with this task.",
            r["commits"],
            r["branch"].as_str().unwrap_or_default(),
            r["into"].as_str().unwrap_or_default()
        ),
        r,
    ))
}

/// `finish_task` from an agent the person runs: it works in their own
/// checkout, so Polyph does not commit; it releases the claims and records
/// the summary on the task card for whoever comes next.
async fn finish_external(h: &Caller, rt: &RoomRuntime, shared: &Shared, args: &Value) -> ToolResult {
    #[derive(Deserialize)]
    struct A {
        summary: String,
    }
    let a: A = arg(args)?;
    let summary = polyph_core::redact::redact(a.summary.trim()).into_owned();
    let released = {
        let mut ctx = rt.ctx.lock().await;
        let mine: Vec<_> = ctx.core.claims.all().into_iter().filter(|c| c.owner.id == h.agent.id).collect();
        let ids: Vec<ObjectId> = mine.iter().map(|c| c.id).collect();
        let fx = ctx.core.release_claims(&ids);
        apply(rt, &mut ctx, shared, fx).await;
        mine.len()
    };
    let card = match h.task {
        Some(t) => Some(t.to_hex()),
        None => assigned_card(rt, &h.agent.label).await,
    };
    if let Some(card) = &card {
        crate::tasks::update_card(
            rt,
            shared,
            card,
            Some("review"),
            &[(polyph_proto::crdt::card::SUMMARY, Some(summary.clone()))],
        )
        .await;
    }
    crate::feed::log(rt, shared, "task_finished", Some(&h.agent.label), json!({ "card": card, "summary": summary }))
        .await;
    let mut text = format!("Released {released} claim(s).");
    if card.is_some() {
        text.push_str(" Your summary is on the task card, which is now in review.");
    }
    text.push_str(" You work in your own checkout, so commit and push your changes with git as you normally would.");
    Ok(ToolOutput::ok(text, json!({ "released": released, "card": card })))
}

/// The card assigned to an agent label and still open, if exactly one.
pub async fn assigned_card(rt: &RoomRuntime, label: &str) -> Option<String> {
    let mut ctx = rt.ctx.lock().await;
    let cards = polyph_proto::crdt::Board::new(ctx.core.doc(&DocName::Board)).all_cards();
    let mine: Vec<_> = cards
        .into_iter()
        .filter(|c| c.assignee.as_deref() == Some(label) && !["done", "review"].contains(&c.column.as_str()))
        .collect();
    match mine.as_slice() {
        [c] => Some(c.id.clone()),
        _ => None,
    }
}

/// A `finish_task` call handed to the daemon's worker.
#[derive(Debug)]
pub struct FinishJob {
    pub agent: ObjectId,
    pub message: Option<String>,
    pub reply: oneshot::Sender<Result<Value, crate::daemon::ApiError>>,
}

async fn read_whiteboard(rt: &RoomRuntime, args: &Value) -> ToolResult {
    #[derive(Deserialize)]
    struct A {
        board: String,
    }
    let a: A = arg(args)?;
    let mut ctx = rt.ctx.lock().await;
    let info = crate::boards::find(&mut ctx, &a.board).map_err(|e| ToolOutput::err(e.to_string()))?;
    let els = polyph_core::whiteboard::elements(ctx.core.doc(&DocName::Whiteboard(info.id.clone())));
    Ok(ToolOutput::ok(
        polyph_core::whiteboard::describe(&info.name, &els),
        json!({ "id": info.id, "elements": els.len() }),
    ))
}

async fn draw_diagram(h: &Caller, rt: &RoomRuntime, shared: &Shared, args: &Value) -> ToolResult {
    #[derive(Deserialize)]
    struct Node {
        id: String,
        label: String,
    }
    #[derive(Deserialize)]
    struct Edge {
        from: String,
        to: String,
        #[serde(default)]
        label: String,
    }
    #[derive(Deserialize)]
    struct A {
        board: String,
        diagram: String,
        nodes: Vec<Node>,
        #[serde(default)]
        edges: Vec<Edge>,
    }
    let a: A = arg(args)?;
    if a.nodes.is_empty() || a.nodes.len() > 60 || a.edges.len() > 120 {
        return Err(ToolOutput::err("give 1 to 60 nodes and at most 120 edges"));
    }
    let slug: String = a.diagram.chars().filter(|c| c.is_ascii_alphanumeric() || *c == '-').take(40).collect();
    if slug.is_empty() {
        return Err(ToolOutput::err("the diagram id needs letters or digits"));
    }
    let red = |s: &str| polyph_core::redact::redact(s).into_owned();
    let nodes: Vec<(String, String)> = a.nodes.iter().map(|n| (n.id.clone(), red(&n.label))).collect();
    let edges: Vec<(String, String, String)> =
        a.edges.iter().map(|e| (e.from.clone(), e.to.clone(), red(&e.label))).collect();
    let now = shared.clock.now();
    let mut ctx = rt.ctx.lock().await;
    let (info, created) = match crate::boards::find(&mut ctx, &a.board) {
        Ok(i) => (i, false),
        Err(_) => {
            let info = crate::boards::create(&mut ctx, &a.board, now).map_err(|e| ToolOutput::err(e.to_string()))?;
            let fx = crate::boards::create_effects(&mut ctx, &info).map_err(|e| ToolOutput::err(e.to_string()))?;
            apply(rt, &mut ctx, shared, fx).await;
            (info, true)
        }
    };
    let doc = DocName::Whiteboard(info.id.clone());
    let prefix = format!("{slug}-{}", h.agent.id.short());
    // Place a new diagram below what is there; a redraw keeps its place.
    let existing = polyph_core::whiteboard::elements(ctx.core.doc(&doc));
    let old: Vec<String> = existing
        .iter()
        .filter_map(|e| e.get("id").and_then(|i| i.as_str()))
        .filter(|i| i.starts_with(&format!("{prefix}-")))
        .map(str::to_string)
        .collect();
    let origin = match existing
        .iter()
        .find(|e| e.get("id").and_then(|i| i.as_str()) == Some(&format!("{prefix}-{}", nodes[0].0)))
    {
        Some(e) => (e["x"].as_f64().unwrap_or(0.0), e["y"].as_f64().unwrap_or(0.0)),
        None => {
            let bottom = existing
                .iter()
                .map(|e| e["y"].as_f64().unwrap_or(0.0) + e["height"].as_f64().unwrap_or(0.0))
                .fold(f64::NEG_INFINITY, f64::max);
            (0.0, if bottom.is_finite() { bottom + 120.0 } else { 0.0 })
        }
    };
    let els = polyph_core::whiteboard::diagram(&prefix, &nodes, &edges, origin, now as u64);
    let fx = ctx
        .core
        .change_doc(doc, |d| {
            polyph_core::whiteboard::remove(d, &old);
            polyph_core::whiteboard::put(d, &els);
        })
        .map_err(|e| ToolOutput::err(e.to_string()))?;
    apply(rt, &mut ctx, shared, fx).await;
    let verb = if old.is_empty() { "Drew" } else { "Redrew" };
    let board = if created { format!("a new whiteboard \"{}\"", info.name) } else { format!("\"{}\"", info.name) };
    Ok(ToolOutput::ok(
        format!("{verb} {} boxes and {} arrows on {board}.", nodes.len(), edges.len()),
        json!({ "board": info.id, "elements": els.len() }),
    ))
}
