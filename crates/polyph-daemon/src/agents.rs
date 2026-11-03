//! Agents on this device: starting them in the sandbox, publishing their
//! transcripts to the room, deciding their requests, usage and budgets.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use polyph_agents::convert::Converter;
use polyph_agents::policy::{self, Action, ClaimState, Preset, Verdict};
use polyph_agents::project::ProjectConfig;
use polyph_agents::registry::{self, AgentConfig, AgentDef};
use polyph_agents::supervisor::{AgentProcess, Answer, CommandWrapper, Output, Request};
use polyph_agents::usage::{Budget, BudgetState, Ledger};
use polyph_core::docs::{self, UsageRecord};
use polyph_core::{Effect, RoomEvent};
use polyph_proto::ids::{ObjectId, RoomId};
use polyph_proto::messages::{
    AgentEvent, AgentEventKind, AgentRef, AgentState, ApprovalKind, ApprovalScope, ClaimTarget, Decision, DocName,
    RoomMessage,
};
use polyph_sandbox::proxy::{Allowlist, Decider, ProxyHandle};
use polyph_sandbox::{Mechanism, Policy};
use serde::Deserialize;
use serde_json::json;
use tokio::sync::{Mutex, mpsc, oneshot};
use tokio::task::JoinHandle;
use tracing::warn;

use crate::daemon::{ApiError, ApiResult, Shared};
use crate::runtime::{RoomRuntime, apply};

pub struct AgentHandle {
    pub agent: AgentRef,
    pub room: RoomId,
    pub proc: Arc<AgentProcess>,
    pub preset: Preset,
    pub task: Option<ObjectId>,
    pub worktree: PathBuf,
    pub project: ProjectConfig,
    pub mechanism: Mechanism,
    pub state: Mutex<AgentState>,
    /// Commands and paths approved "for this task".
    pub approved: Mutex<HashSet<String>>,
    pub busy: Mutex<()>,
    _proxy: ProxyHandle,
    tmp: PathBuf,
    pump: JoinHandle<()>,
}

impl std::fmt::Debug for AgentHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "AgentHandle({})", self.agent.label)
    }
}

impl Drop for AgentHandle {
    fn drop(&mut self) {
        self.pump.abort();
        let _ = std::fs::remove_dir_all(&self.tmp);
    }
}

/// All agents running on this device, by agent id.
#[derive(Default)]
pub struct Agents {
    pub map: Mutex<HashMap<ObjectId, Arc<AgentHandle>>>,
}

impl std::fmt::Debug for Agents {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Agents")
    }
}

#[derive(Debug, Deserialize)]
pub struct StartParams {
    pub room: String,
    pub kind: String,
    pub cwd: PathBuf,
    pub preset: Option<String>,
    pub task: Option<String>,
    pub prompt: Option<String>,
}

async fn publish(rt: &RoomRuntime, shared: &Shared, agent: &AgentRef, kind: AgentEventKind) {
    let mut ctx = rt.ctx.lock().await;
    ctx.agent_seq += 1;
    let ev = Box::new(AgentEvent { agent: agent.clone(), seq: ctx.agent_seq, at: shared.clock.now(), kind });
    let fx = vec![Effect::Send(RoomMessage::Agent(ev.clone())), Effect::Event(RoomEvent::Agent { event: ev })];
    apply(rt, &mut ctx, shared, fx).await;
}

async fn set_state(h: &AgentHandle, rt: &RoomRuntime, shared: &Shared, state: AgentState, detail: Option<String>) {
    *h.state.lock().await = state;
    publish(rt, shared, &h.agent, AgentEventKind::State { state, detail }).await;
}

/// Asks the room for approval and waits for the answer.
async fn request_approval(
    rt: &RoomRuntime,
    shared: &Shared,
    agent: &AgentRef,
    kind: ApprovalKind,
    task: Option<ObjectId>,
) -> Decision {
    let rx = {
        let mut ctx = rt.ctx.lock().await;
        let now = shared.clock.now();
        let (id, fx) = match ctx.core.request_approval(agent.clone(), kind, task, now) {
            Ok(v) => v,
            Err(_) => return Decision::Deny { note: None },
        };
        let (tx, rx) = oneshot::channel();
        ctx.approval_waiters.insert(id, tx);
        apply(rt, &mut ctx, shared, fx).await;
        rx
    };
    rx.await.unwrap_or(Decision::Deny { note: None })
}

fn approval_key(action: &Action) -> Option<String> {
    match action {
        Action::Execute { command } => Some(format!("exec:{command}")),
        Action::Edit { paths } => {
            Some(format!("edit:{}", paths.iter().map(|p| p.to_string_lossy()).collect::<Vec<_>>().join(",")))
        }
        Action::Network { host, port } => Some(format!("net:{host}:{port}")),
        Action::Read { path } => Some(format!("read:{}", path.to_string_lossy())),
        _ => None,
    }
}

/// Decides one request from an agent: policy first, then claims or a human.
async fn decide(h: &AgentHandle, rt: &RoomRuntime, shared: &Shared, action: Action) -> Answer {
    let key = approval_key(&action);
    if let Some(k) = &key
        && h.approved.lock().await.contains(k)
    {
        return Answer::Allow { always: true };
    }
    let verdict = {
        let ctx = rt.ctx.lock().await;
        let now = shared.clock.now();
        let me = h.agent.id;
        let claims = &ctx.core.claims;
        let claim = |p: &str| -> ClaimState {
            let t = ClaimTarget { path: p.to_string(), symbol: None };
            match claims.holder(&t, None, now) {
                None => ClaimState::Free,
                Some(c) if c.owner.id == me => ClaimState::Mine,
                Some(c) => ClaimState::Others(c.owner.label.clone()),
            }
        };
        let cx = policy::Context {
            worktree: &h.worktree,
            check_commands: &h.project.checks,
            auto_claim: h.project.auto_claim(),
            claim: &claim,
        };
        policy::decide(h.preset, &action, &cx)
    };
    match verdict {
        Verdict::Allow => Answer::Allow { always: false },
        Verdict::Deny(note) => Answer::Deny { note: Some(note) },
        Verdict::ClaimAndAllow(files) => {
            let targets: Vec<ClaimTarget> = files.iter().map(|f| ClaimTarget::parse(f)).collect();
            let rx = {
                let mut ctx = rt.ctx.lock().await;
                let now = shared.clock.now();
                match ctx.core.request_claims(
                    &h.agent,
                    &targets,
                    "editing",
                    h.task,
                    polyph_core::room::DEFAULT_LEASE,
                    now,
                ) {
                    Ok((req, fx)) => {
                        let (tx, rx) = oneshot::channel();
                        ctx.claim_waiters.insert(req, tx);
                        apply(rt, &mut ctx, shared, fx).await;
                        rx
                    }
                    Err(_) => return Answer::Deny { note: Some("could not claim the files".into()) },
                }
            };
            match tokio::time::timeout(Duration::from_secs(15), rx).await {
                Ok(Ok((_granted, denied))) if denied.is_empty() => Answer::Allow { always: false },
                Ok(Ok((_, denied))) => {
                    let who: Vec<String> = denied.iter().map(|c| format!("{} ({})", c.target, c.owner.label)).collect();
                    Answer::Deny {
                        note: Some(format!(
                            "claimed by someone else: {}. Use ask_agent to coordinate.",
                            who.join(", ")
                        )),
                    }
                }
                _ => Answer::Deny { note: Some("the claim arbiter did not answer; try again".into()) },
            }
        }
        Verdict::Ask(kind) => {
            set_state(h, rt, shared, AgentState::WaitingApproval, None).await;
            let d = request_approval(rt, shared, &h.agent, kind, h.task).await;
            set_state(h, rt, shared, AgentState::Running, None).await;
            match d {
                Decision::Approve { scope } => {
                    if scope == ApprovalScope::Task
                        && let Some(k) = key
                    {
                        h.approved.lock().await.insert(k);
                    }
                    Answer::Allow { always: scope == ApprovalScope::Task }
                }
                Decision::Deny { note } => Answer::Deny { note },
            }
        }
    }
}

async fn pump(
    mut rx: mpsc::Receiver<Output>,
    handle: Arc<tokio::sync::OnceCell<Arc<AgentHandle>>>,
    rt: Arc<RoomRuntime>,
    shared: Arc<Shared>,
    agent: AgentRef,
) {
    while let Some(o) = rx.recv().await {
        match o {
            Output::Event(kind) => publish(&rt, &shared, &agent, kind).await,
            Output::Barrier(tx) => {
                let _ = tx.send(());
            }
            Output::Request(Request::Permission { action, tool: _, reply }) => {
                let (rt, shared, handle) = (rt.clone(), shared.clone(), handle.clone());
                tokio::spawn(async move {
                    // The handle is set right after the process starts.
                    let h = loop {
                        if let Some(h) = handle.get() {
                            break h.clone();
                        }
                        tokio::time::sleep(Duration::from_millis(20)).await;
                    };
                    let a = decide(&h, &rt, &shared, action).await;
                    let _ = reply.send(a);
                });
            }
        }
    }
}

pub fn custom_agents(shared: &Shared, project: &ProjectConfig) -> BTreeMap<String, AgentConfig> {
    let mut m = shared.opts.extra_agents.clone();
    m.extend(project.agents.clone());
    m
}

fn find_def(defs: &[AgentDef], kind: &str) -> Result<AgentDef, ApiError> {
    defs.iter().find(|d| d.name == kind).cloned().ok_or_else(|| {
        let names: Vec<&str> = defs.iter().map(|d| d.name.as_str()).collect();
        ApiError::NotFound(format!("unknown agent \"{kind}\"; known agents: {}", names.join(", ")))
    })
}

pub fn mechanism(shared: &Shared, project: &ProjectConfig) -> Result<Mechanism, ApiError> {
    if let Some(m) = shared.opts.sandbox {
        return Ok(m);
    }
    match project.sandbox.mode.as_deref() {
        Some("off") => Ok(Mechanism::None),
        Some("container") => Ok(Mechanism::Container),
        _ => match polyph_sandbox::best(&shared.opts.sandbox_helper, false) {
            Mechanism::None => Err(ApiError::Failed("no sandbox is available on this machine; run `polyph sandbox test`, install bubblewrap, or set `[sandbox] mode = \"off\"` in polyph.toml to run agents without one".into())),
            m => Ok(m),
        },
    }
}

pub async fn start(shared: Arc<Shared>, agents: &Agents, rt: Arc<RoomRuntime>, p: StartParams) -> ApiResult {
    let worktree = p.cwd.canonicalize().map_err(|e| ApiError::BadRequest(format!("{}: {e}", p.cwd.display())))?;
    let project = ProjectConfig::load(&worktree)?;
    let defs = registry::all(&custom_agents(&shared, &project));
    let def = find_def(&defs, &p.kind)?;
    let preset: Preset = match &p.preset {
        Some(s) => s.parse().map_err(ApiError::BadRequest)?,
        None => project.policy.preset,
    };
    let (member, device, my_name, room) = {
        let ctx = rt.ctx.lock().await;
        if !ctx.core.role().can_write() {
            return Err(ApiError::Forbidden("viewers cannot run agents".into()));
        }
        (ctx.core.my_member, ctx.core.me, ctx.core.my_profile.display_name.clone(), ctx.core.id)
    };
    let id = ObjectId::random().map_err(|e| ApiError::Failed(e.to_string()))?;
    let same_kind = agents.map.lock().await.values().filter(|h| h.room == room && h.agent.kind == p.kind).count();
    let label =
        if same_kind == 0 { format!("{my_name}/{}", p.kind) } else { format!("{my_name}/{}{}", p.kind, same_kind + 1) };
    let agent = AgentRef { id, member, device, kind: p.kind.clone(), label };
    let task = p.task.as_deref().and_then(|t| ObjectId::from_hex(t).ok());

    let mech = mechanism(&shared, &project)?;
    let tmp = std::env::temp_dir().join(format!("polyph-agent-{}", id.short()));
    std::fs::create_dir_all(&tmp)?;
    let once: Arc<tokio::sync::OnceCell<Arc<AgentHandle>>> = Arc::new(tokio::sync::OnceCell::new());

    // Per-agent proxy so network approvals are attributed to this agent.
    let mut allow = Allowlist::default_for_agents();
    allow.hosts.extend(def.config.hosts.iter().cloned());
    allow.hosts.extend(project.sandbox.allow_hosts.iter().cloned());
    let decider: Decider = {
        let (rt, shared, once) = (rt.clone(), shared.clone(), once.clone());
        Arc::new(move |host, port| {
            let (rt, shared, once) = (rt.clone(), shared.clone(), once.clone());
            Box::pin(async move {
                let Some(h) = once.get().cloned() else { return false };
                matches!(decide(&h, &rt, &shared, Action::Network { host, port }).await, Answer::Allow { .. })
            })
        })
    };
    let proxy = polyph_sandbox::proxy::start(allow, decider, Some(tmp.join("run"))).await?;
    let writable: Vec<PathBuf> =
        def.config.writable.iter().map(|w| registry::expand(w)).filter(|p| p.exists()).collect();
    let policy = Policy {
        worktree: worktree.clone(),
        writable,
        tmp: Some(tmp.join("tmp")),
        hidden: Policy::default_hidden(&shared.paths.home),
        proxy_tcp: Some(proxy.tcp),
        proxy_unix: proxy.unix.clone(),
        helper: shared.opts.sandbox_helper.clone(),
        container_image: project.sandbox.image.clone(),
    };
    std::fs::create_dir_all(tmp.join("tmp"))?;
    let mut wrapped = polyph_sandbox::wrap(mech, &policy, &def.config.command, &def.config.args)?;
    wrapped.env.extend(def.config.env.iter().map(|(k, v)| (k.clone(), v.clone())));
    let wrap: CommandWrapper = {
        let policy = policy.clone();
        Arc::new(move |prog: &str, args: &[String]| polyph_sandbox::wrap(mech, &policy, prog, args))
    };
    let (tx, rx) = mpsc::channel(1024);
    let pump_task = tokio::spawn(pump(rx, once.clone(), rt.clone(), shared.clone(), agent.clone()));
    let proc = match AgentProcess::start(&wrapped, worktree.clone(), vec![], tx, wrap).await {
        Ok(p) => Arc::new(p),
        Err(e) => {
            pump_task.abort();
            let _ = std::fs::remove_dir_all(&tmp);
            return Err(ApiError::Failed(format!("{e}. Run `polyph agents doctor` to check the {} agent.", p.kind)));
        }
    };
    let handle = Arc::new(AgentHandle {
        agent: agent.clone(),
        room,
        proc,
        preset,
        task,
        worktree: worktree.clone(),
        project,
        mechanism: mech,
        state: Mutex::new(AgentState::Idle),
        approved: Mutex::new(HashSet::new()),
        busy: Mutex::new(()),
        _proxy: proxy,
        tmp,
        pump: pump_task,
    });
    let _ = once.set(handle.clone());
    agents.map.lock().await.insert(id, handle.clone());
    publish(&rt, &shared, &agent, AgentEventKind::Started { task, model: None, cloud: false }).await;
    set_state(&handle, &rt, &shared, AgentState::Idle, None).await;
    if let Some(text) = p.prompt {
        let (shared, rt, h) = (shared.clone(), rt.clone(), handle.clone());
        tokio::spawn(async move {
            if let Err(e) = prompt(&shared, &rt, &h, &text).await {
                warn!(error = %e, "initial prompt failed");
            }
        });
    }
    Ok(json!({ "id": id.to_hex(), "label": agent.label, "sandbox": mech, "preset": preset }))
}

/// The ledger and budgets of a room, from its shared documents.
pub async fn ledger(rt: &RoomRuntime, prices: &polyph_agents::usage::PriceTable) -> (Ledger, Vec<Budget>) {
    let mut ctx = rt.ctx.lock().await;
    let records = docs::usage_records(ctx.core.doc(&DocName::Activity));
    let budgets: Vec<Budget> = docs::setting(ctx.core.doc(&DocName::Settings), "budgets")
        .and_then(|v| serde_json::from_value(v).ok())
        .unwrap_or_default();
    let mut l = Ledger::default();
    for r in &records {
        l.record(prices, &r.member, &r.agent, r.task.as_deref(), r.usage.as_ref());
    }
    (l, budgets)
}

/// Sends a prompt, enforcing budgets and recording usage.
pub async fn prompt(shared: &Shared, rt: &RoomRuntime, h: &AgentHandle, text: &str) -> ApiResult {
    let member = h.agent.member.to_hex();
    let (l, budgets) = ledger(rt, &h.project.prices).await;
    if budgets.iter().filter(|b| b.applies_to(&member)).any(|b| b.state(&l) == BudgetState::Exceeded) {
        set_state(h, rt, shared, AgentState::Blocked, Some("budget".into())).await;
        return Err(ApiError::Forbidden(
            "the budget for this room or member is used up; a Maintainer can raise it with `polyph budget set`".into(),
        ));
    }
    let _busy = h.busy.lock().await;
    publish(rt, shared, &h.agent, AgentEventKind::Prompt { text: text.to_string(), from: h.agent.member }).await;
    set_state(h, rt, shared, AgentState::Running, None).await;
    let r = h.proc.prompt(text).await;
    let resp = match r {
        Ok(r) => r,
        Err(e) => {
            set_state(h, rt, shared, AgentState::Error, Some(e.to_string())).await;
            return Err(ApiError::Failed(e.to_string()));
        }
    };
    let subscription = h.project.agents.get(&h.agent.kind).and_then(|c| c.metered).map(|m| !m).unwrap_or(false);
    let usage = resp.usage.as_ref().map(|u| Converter::usage(u, None, subscription));
    if let Some(u) = &usage {
        publish(rt, shared, &h.agent, AgentEventKind::Usage(u.clone())).await;
    }
    {
        let mut ctx = rt.ctx.lock().await;
        let rec = UsageRecord {
            id: ObjectId::random().map(|o| o.to_hex()).unwrap_or_default(),
            member: member.clone(),
            agent: h.agent.label.clone(),
            task: h.task.map(|t| t.to_hex()),
            usage: usage.clone(),
            at: shared.clock.now(),
        };
        if let Ok(fx) = ctx.core.change_doc(DocName::Activity, |d| docs::append_usage(d, &rec)) {
            apply(rt, &mut ctx, shared, fx).await;
        }
    }
    let (l, budgets) = ledger(rt, &h.project.prices).await;
    for b in budgets.iter().filter(|b| b.applies_to(&member)) {
        if let BudgetState::Warning { percent } = b.state(&l) {
            shared.emit(crate::events::ApiEvent::room_kind(
                &h.room,
                "budget_warning",
                json!({ "budget": b, "percent": percent }),
            ));
        }
    }
    set_state(h, rt, shared, AgentState::Idle, None).await;
    Ok(json!({ "stop_reason": resp.stop_reason, "usage": usage }))
}

pub async fn stop(shared: &Shared, agents: &Agents, rt: &RoomRuntime, id: ObjectId) -> ApiResult {
    let h =
        agents.map.lock().await.remove(&id).ok_or_else(|| ApiError::NotFound("no such agent on this device".into()))?;
    h.proc.stop().await;
    {
        let mut ctx = rt.ctx.lock().await;
        let mine: Vec<ObjectId> =
            ctx.core.claims.all().into_iter().filter(|c| c.owner.id == id).map(|c| c.id).collect();
        let fx = ctx.core.release_claims(&mine);
        apply(rt, &mut ctx, shared, fx).await;
    }
    publish(rt, shared, &h.agent, AgentEventKind::Stopped { reason: "stopped".into() }).await;
    Ok(json!({}))
}

pub fn doctor(shared: &Shared, cwd: &Path) -> ApiResult {
    let project = ProjectConfig::load(cwd).unwrap_or_default();
    let defs = registry::all(&custom_agents(shared, &project));
    let items = registry::doctor(&defs);
    let sandbox = polyph_sandbox::detect(&shared.opts.sandbox_helper);
    Ok(json!({ "agents": items, "sandbox": sandbox }))
}
