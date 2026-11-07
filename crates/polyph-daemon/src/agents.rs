//! Agents on this device: starting them in the sandbox, publishing their
//! transcripts to the room, deciding their requests, usage and budgets.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use polyph_agents::convert::Converter;
use polyph_agents::policy::{self, Action, ClaimState, Preset, Verdict};
use polyph_agents::project::{Loaded, ProjectConfig, TrustStore};
use polyph_agents::registry::{self, AgentConfig, AgentDef};
use polyph_agents::supervisor::{AgentProcess, Answer, CommandWrapper, Output, Request};
use polyph_agents::usage::{Budget, BudgetState, Ledger, Limit};
use polyph_agents::wire::{EnvVariable, McpServerStdio};
use polyph_core::docs::{self, UsageRecord};
use polyph_core::{Effect, RoomEvent};
use polyph_proto::ids::{ObjectId, RoomId};
use polyph_proto::messages::{
    AgentEvent, AgentEventKind, AgentRef, AgentState, ApprovalKind, ApprovalScope, ClaimTarget, Decision, DocName,
    RoomMessage,
};
use polyph_sandbox::proxy::{Allowlist, Decider, Internal, ProxyHandle};
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
    /// The project file's `[prices]`, when they were ignored as untrusted:
    /// dollar budgets cannot be counted, so they stop the agent.
    pub untrusted_prices: Option<Loaded>,
    pub mechanism: Mechanism,
    pub state: Mutex<AgentState>,
    /// Commands and paths approved "for this task".
    pub approved: Mutex<HashSet<String>>,
    pub busy: Mutex<()>,
    /// Secret the agent's Polyph MCP server presents on every call.
    pub mcp_token: String,
    /// The task's worktree and branch, when the agent works on a task.
    pub task_wt: Option<crate::tasks::TaskWorktree>,
    pub helper: bool,
    /// A handoff brief from the card, given with the first prompt.
    pub brief: Mutex<Option<String>>,
    /// Files this agent was just allowed to write through the protocol; the
    /// layer-3 watcher leaves these alone.
    pub acp_writes: parking_lot::Mutex<HashMap<PathBuf, std::time::Instant>>,
    /// Hops of the agent message that started the current turn (0 when a person did).
    pub trigger_hops: std::sync::atomic::AtomicU8,
    _proxy: ProxyHandle,
    tmp: PathBuf,
    pump: JoinHandle<()>,
    waker: JoinHandle<()>,
    /// Claims layer 3: the worktree watcher (see `watch`).
    watcher: parking_lot::Mutex<Option<JoinHandle<()>>>,
}

impl std::fmt::Debug for AgentHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "AgentHandle({})", self.agent.label)
    }
}

impl Drop for AgentHandle {
    fn drop(&mut self) {
        self.pump.abort();
        self.waker.abort();
        if let Some(w) = self.watcher.lock().take() {
            w.abort();
        }
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
    /// Give a task its own worktree and branch (default: yes, when `task` is
    /// set and `cwd` is in a git repository).
    #[serde(default)]
    pub worktree: Option<bool>,
    /// Polyph's own reviewer and resolver agents: they work in scratch
    /// worktrees and are not subject to room claims.
    #[serde(skip)]
    pub helper: bool,
}

pub(crate) async fn publish(rt: &RoomRuntime, shared: &Shared, agent: &AgentRef, mut kind: AgentEventKind) {
    // Nothing an agent prints, runs or diffs leaves the device unredacted.
    polyph_core::redact::redact_event(&mut kind);
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
pub(crate) async fn request_approval(
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
        Action::Edit { paths, .. } => {
            Some(format!("edit:{}", paths.iter().map(|p| p.to_string_lossy()).collect::<Vec<_>>().join(",")))
        }
        Action::Network { host, port } => Some(format!("net:{host}:{port}")),
        Action::Read { path } => Some(format!("read:{}", path.to_string_lossy())),
        _ => None,
    }
}

/// Asks the arbiter for `targets` and allows the action if all are granted.
async fn claim_and_allow(
    h: &AgentHandle,
    rt: &RoomRuntime,
    shared: &Shared,
    targets: &[ClaimTarget],
    reason: &str,
) -> Answer {
    let rx = {
        let mut ctx = rt.ctx.lock().await;
        let now = shared.clock.now();
        match ctx.core.request_claims(&h.agent, targets, reason, h.task, polyph_core::room::DEFAULT_LEASE, now) {
            Ok((req, fx)) => {
                let (tx, rx) = oneshot::channel();
                ctx.claim_waiters.insert(req, (h.agent.id, tx));
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
                note: Some(format!("claimed by someone else: {}. Use ask_agent to coordinate.", who.join(", "))),
            }
        }
        _ => Answer::Deny { note: Some("the claim arbiter did not answer; try again".into()) },
    }
}

/// With `[claims] granularity = "symbol"`: the symbols a whole-file write
/// changes, as claim targets. None when the write must claim the file
/// (a new file, an unknown language, or changes outside any symbol).
fn symbol_targets(worktree: &Path, path: &Path, new: &str) -> Option<Vec<ClaimTarget>> {
    let rel = path.strip_prefix(worktree).ok()?.to_string_lossy().replace('\\', "/");
    let lang = polyph_merge::symbols::Lang::from_path(&rel)?;
    let old = std::fs::read_to_string(path).ok()?;
    let names = polyph_merge::symbols::changed_symbols(&old, new, lang)?;
    if names.is_empty() {
        return None;
    }
    Some(names.into_iter().map(|n| ClaimTarget { path: rel.clone(), symbol: Some(n) }).collect())
}

/// Decides one request from an agent: policy first, then claims or a human.
async fn decide(h: &AgentHandle, rt: &RoomRuntime, shared: &Shared, action: Action) -> Answer {
    let key = approval_key(&action);
    if let Some(k) = &key
        && h.approved.lock().await.contains(k)
    {
        return Answer::Allow { always: true };
    }
    // Symbol claims: two agents may edit different functions of one file.
    if let Action::Edit { paths, content: Some(new) } = &action
        && paths.len() == 1
        && h.preset != Preset::AskAll
        && !h.helper
        && h.project.symbol_claims()
        && let Some(targets) = symbol_targets(&h.worktree, &paths[0], new)
    {
        let blocked: Vec<String> = {
            let ctx = rt.ctx.lock().await;
            let now = shared.clock.now();
            targets
                .iter()
                .filter_map(|t| ctx.core.claims.holder(t, Some(&h.agent), now))
                .map(|c| format!("{} ({})", c.target, c.owner.label))
                .collect()
        };
        if !blocked.is_empty() {
            return Answer::Deny {
                note: Some(format!("claimed by someone else: {}. Use ask_agent to coordinate.", blocked.join(", "))),
            };
        }
        return claim_and_allow(h, rt, shared, &targets, "editing").await;
    }
    let verdict = {
        let ctx = rt.ctx.lock().await;
        let now = shared.clock.now();
        let me = h.agent.id;
        let claims = &ctx.core.claims;
        let claim = |p: &str| -> ClaimState {
            let t = ClaimTarget { path: p.to_string(), symbol: None };
            if h.helper {
                return ClaimState::Mine;
            }
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
            claim_and_allow(h, rt, shared, &targets, "editing").await
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
                    let edits = match &action {
                        Action::Edit { paths, .. } => paths.clone(),
                        _ => vec![],
                    };
                    let a = decide(&h, &rt, &shared, action).await;
                    if matches!(a, Answer::Allow { .. }) && !edits.is_empty() {
                        let now = std::time::Instant::now();
                        let mut w = h.acp_writes.lock();
                        w.retain(|_, t| now.duration_since(*t) < std::time::Duration::from_secs(60));
                        for p in edits {
                            w.insert(p, now);
                        }
                    }
                    let _ = reply.send(a);
                });
            }
        }
    }
}

/// The project file that decides how agents and checks run. It is read from
/// the person's main checkout, never a task worktree an agent can write, and
/// its security-relevant sections only apply once `polyph trust` recorded this
/// exact content; otherwise they are dropped and the person is told.
pub fn trusted_project(shared: &Shared, dir: &Path) -> anyhow::Result<Loaded> {
    let loaded = load_from_main_checkout(&TrustStore::at(shared.paths.trusted_projects()), dir)?;
    if let Some(u) = untrusted_json(&loaded) {
        warn!("{}", u["message"].as_str().unwrap_or_default());
        shared.emit(crate::events::ApiEvent::global("project_untrusted", u));
    }
    Ok(loaded)
}

/// The project file from the main checkout of `dir`, with untrusted sections
/// dropped, without telling anyone. For settings read in passing.
pub fn main_checkout_project(shared: &Shared, dir: &Path) -> anyhow::Result<Loaded> {
    load_from_main_checkout(&TrustStore::at(shared.paths.trusted_projects()), dir)
}

pub(crate) fn load_from_main_checkout(store: &TrustStore, dir: &Path) -> anyhow::Result<Loaded> {
    let root = polyph_git::Repo::open(dir).ok().and_then(|r| r.main_root().ok()).unwrap_or_else(|| dir.to_path_buf());
    ProjectConfig::load_trusted(&root, store)
}

/// The daemon's localizer, in the language this person chose.
pub fn localizer(shared: &Shared) -> polyph_i18n::Localizer {
    let lang = shared.config.lock().lang.clone();
    polyph_i18n::Localizer::new(&polyph_i18n::detect(lang.as_deref()))
}

/// `key` with the untrusted file's path, folder and ignored sections; None
/// when the file is trusted or sets nothing that needs trust.
pub fn untrusted_text(l: &polyph_i18n::Localizer, key: &str, loaded: &Loaded) -> Option<String> {
    use polyph_i18n::FluentValue;
    let file = loaded.file.as_ref().filter(|_| !loaded.ignored.is_empty())?;
    let dir = file.parent().unwrap_or(file);
    Some(l.t_args(
        key,
        &[
            ("file", FluentValue::from(file.display().to_string())),
            ("dir", FluentValue::from(dir.display().to_string())),
            ("ignored", FluentValue::from(loaded.ignored.join(", "))),
        ],
    ))
}

/// `loaded` narrowed to the ignored `sections`, for a message about just
/// those; None when none of them was ignored.
pub fn only_ignored(loaded: &Loaded, sections: &[&str]) -> Option<Loaded> {
    let ignored: Vec<&'static str> = loaded.ignored.iter().copied().filter(|s| sections.contains(s)).collect();
    (!ignored.is_empty() && loaded.file.is_some()).then(|| Loaded {
        config: ProjectConfig::default(),
        file: loaded.file.clone(),
        ignored,
    })
}

/// Adds the "run `polyph trust`" hint to an agent start error when the
/// project file had settings that were ignored, since they may be the cause.
fn with_untrusted_hint(l: &polyph_i18n::Localizer, loaded: &Loaded, e: ApiError) -> ApiError {
    let Some(hint) = untrusted_text(l, "project-untrusted-hint", loaded) else {
        return e;
    };
    match e {
        ApiError::BadRequest(m) => ApiError::BadRequest(format!("{m} {hint}")),
        ApiError::NotFound(m) => ApiError::NotFound(format!("{m} {hint}")),
        ApiError::Forbidden(m) => ApiError::Forbidden(format!("{m} {hint}")),
        ApiError::Failed(m) => ApiError::Failed(format!("{m} {hint}")),
    }
}

/// Refuses a worktree that contains Polyph's data directory or lies inside
/// it, and a writable path that contains it: the sandbox would then make the
/// device keys and the trust record visible or writable to the agent.
fn check_data_dir(
    l: &polyph_i18n::Localizer,
    home: &Path,
    worktree: &Path,
    writable: &[PathBuf],
) -> Result<(), ApiError> {
    use polyph_i18n::FluentValue;
    // A path that cannot be resolved cannot be compared: refuse rather than
    // compare the raw path, which a symlink could make look harmless.
    let canon = |p: &Path| {
        std::fs::canonicalize(p).map_err(|e| {
            ApiError::BadRequest(l.t_args(
                "agent-path-unresolved",
                &[("path", FluentValue::from(p.display().to_string())), ("error", FluentValue::from(e.to_string()))],
            ))
        })
    };
    let home = canon(home)?;
    let wt = canon(worktree)?;
    let bad = if home.starts_with(&wt) || wt.starts_with(&home) {
        Some(wt)
    } else {
        let mut found = None;
        for w in writable {
            // A writable path that does not exist is never mounted (`start`
            // drops it), so it cannot expose anything. A dangling link exists
            // as a link and is refused below.
            if std::fs::symlink_metadata(w).is_err_and(|e| e.kind() == std::io::ErrorKind::NotFound) {
                continue;
            }
            let w = canon(w)?;
            if home.starts_with(&w) {
                found = Some(w);
                break;
            }
        }
        found
    };
    match bad {
        None => Ok(()),
        Some(p) => Err(ApiError::BadRequest(l.t_args(
            "agent-data-dir-exposed",
            &[
                ("path", FluentValue::from(p.display().to_string())),
                ("home", FluentValue::from(home.display().to_string())),
            ],
        ))),
    }
}

/// What an untrusted project file had ignored, for the person to act on.
pub(crate) fn untrusted_json(loaded: &Loaded) -> Option<serde_json::Value> {
    let file = loaded.file.as_ref().filter(|_| !loaded.ignored.is_empty())?;
    let dir = file.parent().unwrap_or(file);
    let message = format!(
        "Ignored {} in {}: this file is not trusted on this device, so agents run with the default sandbox and built-in agent definitions. Review the file, then run `polyph trust` in {} to apply it.",
        loaded.ignored.join(", "),
        file.display(),
        dir.display()
    );
    Some(json!({ "file": file, "dir": dir, "ignored": loaded.ignored, "message": message }))
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
    let cwd = polyph_git::canonical(&p.cwd).map_err(|e| ApiError::BadRequest(format!("{}: {e}", p.cwd.display())))?;
    let task = p.task.as_deref().and_then(|t| ObjectId::from_hex(t).ok());
    // A task gets its own worktree and branch, so agents never share files.
    let task_card = match task {
        Some(t) => crate::tasks::card(&mut *rt.ctx.lock().await, &t.to_hex()),
        None => None,
    };
    let task_wt = match task {
        Some(t) if p.worktree != Some(false) && polyph_git::Repo::open(&cwd).is_ok() => {
            let card = task_card.clone();
            let branch = card.as_ref().and_then(|c| c.branch.clone()).unwrap_or_else(|| {
                polyph_git::task_branch(card.as_ref().map(|c| c.title.as_str()).unwrap_or(""), &t.short())
            });
            Some(crate::tasks::prepare_worktree(cwd.clone(), branch, None).await?)
        }
        _ => None,
    };
    // Settings come from the person's checkout, not the agent's worktree.
    let loaded = trusted_project(&shared, task_wt.as_ref().map(|w| w.repo.as_path()).unwrap_or(&cwd))?;
    let project = loaded.config.clone();
    let worktree = task_wt.as_ref().map(|w| w.path.clone()).unwrap_or(cwd);
    let defs = registry::all(&custom_agents(&shared, &project));
    let l = localizer(&shared);
    let def = find_def(&defs, &p.kind).map_err(|e| with_untrusted_hint(&l, &loaded, e))?;
    let writable: Vec<PathBuf> =
        def.config.writable.iter().map(|w| registry::expand(w)).filter(|p| p.exists()).collect();
    check_data_dir(&l, &shared.paths.home, &worktree, &writable)?;
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

    let mech = mechanism(&shared, &project).map_err(|e| with_untrusted_hint(&l, &loaded, e))?;
    let tmp = std::env::temp_dir().join(format!("polyph-agent-{}", id.short()));
    std::fs::create_dir_all(&tmp)?;
    let once: Arc<tokio::sync::OnceCell<Arc<AgentHandle>>> = Arc::new(tokio::sync::OnceCell::new());

    // Per-agent proxy so network approvals are attributed to this agent.
    let mut allow = Allowlist::default_for_agents();
    allow.hosts.extend(def.config.hosts.iter().cloned());
    allow.hosts.extend(project.sandbox.allow_hosts.iter().cloned());
    // Plugins' tool servers run inside this agent's sandbox, on its proxy.
    allow.hosts.extend(crate::plugins::hosts(&shared));
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
    let mcp_token = ObjectId::random().map_err(|e| ApiError::Failed(e.to_string()))?.to_hex();
    let internal: Internal = {
        let (rt, shared, once) = (rt.clone(), shared.clone(), once.clone());
        Arc::new(move |req| {
            let (rt, shared, once) = (rt.clone(), shared.clone(), once.clone());
            Box::pin(async move {
                match once.get().cloned() {
                    Some(h) => crate::coord::internal(h, rt, shared, req).await,
                    None => (503, br#"{"error":"agent starting"}"#.to_vec()),
                }
            })
        })
    };
    let proxy = polyph_sandbox::proxy::start_with(allow, decider, Some(internal), Some(tmp.join("run"))).await?;
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
    let command = registry::resolve_command(&def.config.command)
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_else(|| def.config.command.clone());
    let mut wrapped = polyph_sandbox::wrap(mech, &policy, &command, &def.config.args)?;
    wrapped.env.extend(def.config.env.iter().map(|(k, v)| (k.clone(), v.clone())));
    let wrap: CommandWrapper = {
        let policy = policy.clone();
        Arc::new(move |prog: &str, args: &[String]| polyph_sandbox::wrap(mech, &policy, prog, args))
    };
    // The Polyph MCP server runs inside the sandbox and reaches the daemon
    // through this agent's proxy, at the address the sandbox exports.
    let mut mcp = vec![];
    if let Some((cmd, args)) = shared.opts.mcp_command.split_first() {
        let proxy_url =
            wrapped.env.iter().find(|(k, _)| k == "HTTP_PROXY").map(|(_, v)| v.clone()).unwrap_or_else(|| proxy.url());
        mcp.push(McpServerStdio {
            name: polyph_mcp::SERVER_NAME.into(),
            command: cmd.clone(),
            args: args.to_vec(),
            env: vec![
                EnvVariable { name: polyph_mcp::ENV_PROXY.into(), value: proxy_url },
                EnvVariable { name: polyph_mcp::ENV_TOKEN.into(), value: mcp_token.clone() },
            ],
        });
    }
    if !p.helper {
        mcp.extend(crate::plugins::mcp_servers(&shared, &tmp.join("tmp")));
    }
    let (tx, rx) = mpsc::channel(1024);
    let pump_task = tokio::spawn(pump(rx, once.clone(), rt.clone(), shared.clone(), agent.clone()));
    let (wake_tx, wake_rx) = mpsc::unbounded_channel();
    let waker_task = tokio::spawn(wake_loop(wake_rx, once.clone(), rt.clone(), shared.clone()));
    let proc = match AgentProcess::start(&wrapped, worktree.clone(), mcp, tx, wrap).await {
        Ok(p) => Arc::new(p),
        Err(e) => {
            waker_task.abort();
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
        untrusted_prices: only_ignored(&loaded, &["prices"]),
        mechanism: mech,
        state: Mutex::new(AgentState::Idle),
        approved: Mutex::new(HashSet::new()),
        busy: Mutex::new(()),
        mcp_token,
        task_wt: task_wt.clone(),
        helper: p.helper,
        brief: Mutex::new(task_card.as_ref().and_then(|c| c.handoff.clone())),
        acp_writes: parking_lot::Mutex::new(HashMap::new()),
        trigger_hops: std::sync::atomic::AtomicU8::new(0),
        _proxy: proxy,
        tmp,
        pump: pump_task,
        waker: waker_task,
        watcher: parking_lot::Mutex::new(None),
    });
    let _ = once.set(handle.clone());
    rt.ctx.lock().await.coord.wakers.insert(id, wake_tx);
    if !p.helper {
        crate::coord::restore_inbox(&rt, &shared, &agent).await;
    }
    if handle.project.watch() && !p.helper {
        *handle.watcher.lock() = crate::watch::spawn(handle.clone(), rt.clone(), shared.clone());
    }
    if let (Some(t), Some(wt)) = (task, &task_wt) {
        crate::tasks::update_card(
            &rt,
            &shared,
            &t.to_hex(),
            Some("in_progress"),
            &[
                ("branch", Some(wt.branch.clone())),
                ("assignee", Some(agent.label.clone())),
                // The brief is now this agent's; nobody else picks it up.
                (polyph_proto::crdt::card::HANDOFF, None),
            ],
        )
        .await;
    }
    agents.map.lock().await.insert(id, handle.clone());
    publish(&rt, &shared, &agent, AgentEventKind::Started { task, model: None, cloud: false }).await;
    if !p.helper {
        crate::feed::log(&rt, &shared, "agent_started", Some(&agent.label), json!({ "kind": agent.kind, "card": task.map(|t| t.to_hex()), "branch": task_wt.as_ref().map(|w| w.branch.clone()) })).await;
    }
    set_state(&handle, &rt, &shared, AgentState::Idle, None).await;
    if let Some(text) = p.prompt {
        let (shared, rt, h) = (shared.clone(), rt.clone(), handle.clone());
        tokio::spawn(async move {
            if let Err(e) = prompt(&shared, &rt, &h, &text).await {
                warn!(error = %e, "initial prompt failed");
            }
        });
    }
    Ok(json!({
        "id": id.to_hex(),
        "label": agent.label,
        "sandbox": mech,
        "preset": preset,
        "worktree": task_wt.as_ref().map(|w| w.path.clone()),
        "branch": task_wt.as_ref().map(|w| w.branch.clone()),
        "untrusted": untrusted_json(&loaded),
    }))
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

/// Whether `member` may run another turn under `budgets`. A dollar budget
/// cannot be counted when the project file's prices were ignored as untrusted
/// (every turn would cost $0), so it stops the agent until `polyph trust`.
fn budget_allows(
    l: &polyph_i18n::Localizer,
    budgets: &[Budget],
    ledger: &Ledger,
    member: &str,
    untrusted_prices: Option<&Loaded>,
) -> Result<(), String> {
    let mine = || budgets.iter().filter(|b| b.applies_to(member));
    if mine().any(|b| b.state(ledger) == BudgetState::Exceeded) {
        return Err(
            "the budget for this room or member is used up; a Maintainer can raise it with `polyph budget set`".into(),
        );
    }
    if let Some(u) = untrusted_prices
        && mine().any(|b| matches!(b.limit, Limit::Dollars(_)))
        && let Some(why) = untrusted_text(l, "budget-prices-untrusted", u)
    {
        return Err(why);
    }
    Ok(())
}

/// Runs turns when other agents write to this one, within the loop guard.
async fn wake_loop(
    mut rx: mpsc::UnboundedReceiver<()>,
    handle: Arc<tokio::sync::OnceCell<Arc<AgentHandle>>>,
    rt: Arc<RoomRuntime>,
    shared: Arc<Shared>,
) {
    while rx.recv().await.is_some() {
        while rx.try_recv().is_ok() {}
        let Some(h) = handle.get().cloned() else { continue };
        let allowed = rt.ctx.lock().await.coord.allow_wake(h.agent.id, shared.clock.now());
        if !allowed {
            shared.emit(crate::events::ApiEvent::room_kind(
                &h.room,
                "loop_guard",
                json!({ "agent": h.agent.id.to_hex(), "label": h.agent.label, "reason": "wakes_per_hour" }),
            ));
            continue;
        }
        if let Err(e) = run_turn(&shared, &rt, &h, None).await {
            warn!(error = %e, agent = %h.agent.label, "turn for new messages failed");
        }
    }
}

/// Sends a prompt, enforcing budgets and recording usage.
pub async fn prompt(shared: &Shared, rt: &RoomRuntime, h: &AgentHandle, text: &str) -> ApiResult {
    run_turn(shared, rt, h, Some(text)).await
}

/// One turn: a person's prompt (`Some`), or a wake for new messages (`None`).
/// Unread messages and Polyph notices are added to the prompt either way.
async fn run_turn(shared: &Shared, rt: &RoomRuntime, h: &AgentHandle, text: Option<&str>) -> ApiResult {
    let member = h.agent.member.to_hex();
    let (l, budgets) = ledger(rt, &h.project.prices).await;
    if let Err(why) = budget_allows(&localizer(shared), &budgets, &l, &member, h.untrusted_prices.as_ref()) {
        set_state(h, rt, shared, AgentState::Blocked, Some("budget".into())).await;
        return Err(ApiError::Forbidden(why));
    }
    let _busy = h.busy.lock().await;
    let (mut extra, hops) = {
        let mut ctx = rt.ctx.lock().await;
        let r = crate::coord::turn_boundary(&mut ctx, h.agent.id);
        if r.0.is_some() {
            crate::coord::persist_inboxes(&mut ctx, shared);
        }
        r
    };
    if let Some(b) = h.brief.lock().await.take() {
        extra = Some(match extra {
            Some(x) => format!("[Polyph] Handoff:\n{b}\n\n{x}"),
            None => format!("[Polyph] Handoff:\n{b}"),
        });
    }
    let woken = text.is_none();
    let text = match (text, extra) {
        (Some(t), Some(x)) => format!("{t}\n\n{x}"),
        (Some(t), None) => t.to_string(),
        (None, Some(x)) => x,
        // Woken, but a turn in between already delivered the messages.
        (None, None) => return Ok(json!({ "stop_reason": null })),
    };
    // A person's prompt starts a fresh exchange; a wake continues the chain.
    h.trigger_hops.store(if woken { hops } else { 0 }, std::sync::atomic::Ordering::Relaxed);
    let text = text.as_str();
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
        ctx.coord.wakers.remove(&id);
        let mine: Vec<ObjectId> =
            ctx.core.claims.all().into_iter().filter(|c| c.owner.id == id).map(|c| c.id).collect();
        let fx = ctx.core.release_claims(&mine);
        apply(rt, &mut ctx, shared, fx).await;
    }
    // Unread messages wait for the next agent with this label.
    crate::coord::save_inbox(rt, shared, &h.agent).await;
    publish(rt, shared, &h.agent, AgentEventKind::Stopped { reason: "stopped".into() }).await;
    if !h.helper {
        crate::feed::log(
            rt,
            shared,
            "agent_stopped",
            Some(&h.agent.label),
            json!({ "card": h.task.map(|t| t.to_hex()) }),
        )
        .await;
    }
    Ok(json!({}))
}

pub fn doctor(shared: &Shared, cwd: &Path) -> ApiResult {
    let project = trusted_project(shared, cwd).unwrap_or_default().config;
    let defs = registry::all(&custom_agents(shared, &project));
    let items = registry::doctor(&defs);
    let sandbox = polyph_sandbox::detect(&shared.opts.sandbox_helper);
    Ok(json!({ "agents": items, "sandbox": sandbox }))
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    /// An agent's task worktree can carry its own polyph.toml; the daemon
    /// reads the person's checkout instead, and only trusted content counts.
    #[test]
    fn project_comes_from_the_main_checkout_and_needs_trust() {
        let t = tempfile::tempdir().unwrap();
        let main = t.path().join("main");
        std::fs::create_dir_all(&main).unwrap();
        let git = |args: &[&str]| {
            assert!(std::process::Command::new("git").args(args).current_dir(&main).status().unwrap().success())
        };
        git(&["init", "-q", "-b", "main"]);
        git(&["config", "core.autocrlf", "false"]);
        std::fs::write(main.join("polyph.toml"), "[sandbox]\nmode = \"off\"\n").unwrap();
        let repo = polyph_git::Repo::open(&main).unwrap();
        repo.commit_all("initial", &polyph_git::Signature::agent("Ann/test", "0")).unwrap();
        let wt = t.path().join("task");
        repo.add_worktree(&wt, "task", "main").unwrap();
        std::fs::write(wt.join("polyph.toml"), "[agents.claude]\ncommand = \"sh\"\n[sandbox]\nmode = \"off\"\n")
            .unwrap();
        let store = TrustStore::at(t.path().join("home/trusted-projects.json"));

        let main_file = polyph_git::canonical(&main).unwrap().join("polyph.toml");
        let l = load_from_main_checkout(&store, &wt).unwrap();
        assert_eq!(l.file.as_deref(), Some(main_file.as_path()));
        assert_eq!(l.ignored, vec!["sandbox"]);
        assert_eq!(l.config.sandbox.mode, None);
        let u = untrusted_json(&l).unwrap();
        assert!(u["message"].as_str().unwrap().contains("polyph trust"));

        store.trust(&polyph_agents::project::ProjectFile::find(&main).unwrap().unwrap()).unwrap();
        let l = load_from_main_checkout(&store, &wt).unwrap();
        assert!(l.ignored.is_empty() && untrusted_json(&l).is_none());
        assert_eq!(l.config.sandbox.mode.as_deref(), Some("off"));
        // The worktree's own agent definition never applies.
        assert!(l.config.agents.is_empty());
    }

    #[test]
    fn start_errors_point_at_polyph_trust_when_settings_were_ignored() {
        let en = polyph_i18n::Localizer::new("en");
        let loaded = Loaded {
            config: ProjectConfig::default(),
            file: Some(PathBuf::from("/repo/polyph.toml")),
            ignored: vec!["agents", "sandbox"],
        };
        let e = with_untrusted_hint(&en, &loaded, ApiError::NotFound("unknown agent \"mine\"".into()));
        let ApiError::NotFound(m) = e else { panic!("variant changed") };
        assert!(m.starts_with("unknown agent \"mine\" "), "{m}");
        assert!(m.contains("agents, sandbox") && m.contains("`polyph trust` in /repo"), "{m}");
        let e = with_untrusted_hint(&en, &loaded, ApiError::Failed("no sandbox is available".into()));
        assert!(e.to_string().contains("polyph trust"));
        // A trusted file, or one that sets nothing needing trust: no hint.
        let quiet = Loaded { ignored: vec![], ..loaded };
        assert_eq!(with_untrusted_hint(&en, &quiet, ApiError::Failed("x".into())).to_string(), "x");
    }

    #[test]
    fn agents_never_get_a_worktree_that_overlaps_the_data_dir() {
        let en = polyph_i18n::Localizer::new("en");
        let t = tempfile::tempdir().unwrap();
        let home = t.path().join("user/.local/share/polyph");
        let repo = t.path().join("user/code/app");
        std::fs::create_dir_all(&home).unwrap();
        std::fs::create_dir_all(&repo).unwrap();
        assert!(check_data_dir(&en, &home, &repo, &[]).is_ok());
        // A worktree that holds the data directory.
        let e = check_data_dir(&en, &home, &t.path().join("user"), &[]).unwrap_err();
        assert!(e.to_string().contains("data directory"), "{e}");
        assert!(check_data_dir(&en, &home, &home, &[]).is_err());
        // A worktree inside it.
        let inside = home.join("rooms");
        std::fs::create_dir_all(&inside).unwrap();
        assert!(check_data_dir(&en, &home, &inside, &[]).is_err());
        // A writable path that holds it.
        assert!(check_data_dir(&en, &home, &repo, &[t.path().join("user/.local")]).is_err());
        assert!(check_data_dir(&en, &home, &repo, &[t.path().join("user/.claude")]).is_ok());
    }

    #[test]
    fn a_path_that_cannot_be_resolved_is_refused_not_compared_raw() {
        let en = polyph_i18n::Localizer::new("en");
        let t = tempfile::tempdir().unwrap();
        let home = t.path().join("home/polyph");
        let repo = t.path().join("code/app");
        std::fs::create_dir_all(&home).unwrap();
        std::fs::create_dir_all(&repo).unwrap();
        let gone = t.path().join("gone");
        let e = check_data_dir(&en, &home, &gone, &[]).unwrap_err();
        assert!(e.to_string().contains("cannot resolve") && e.to_string().contains("gone"), "{e}");
        assert!(check_data_dir(&en, &gone, &repo, &[]).is_err());
        // A writable entry that is a dangling link cannot be checked either.
        #[cfg(unix)]
        {
            let link = t.path().join("dangling");
            std::os::unix::fs::symlink(t.path().join("nowhere"), &link).unwrap();
            assert!(check_data_dir(&en, &home, &repo, &[link]).is_err());
        }
    }

    #[test]
    fn untrusted_prices_stop_dollar_budgets_but_not_token_budgets() {
        use polyph_agents::usage::BudgetScope;
        let en = polyph_i18n::Localizer::new("en");
        let loaded = Loaded {
            config: ProjectConfig::default(),
            file: Some(PathBuf::from("/repo/polyph.toml")),
            ignored: vec!["agents", "prices"],
        };
        let prices = only_ignored(&loaded, &["prices"]);
        assert_eq!(prices.as_ref().unwrap().ignored, vec!["prices"]);
        let dollars = Budget { scope: BudgetScope::Room, limit: Limit::Dollars(5.0) };
        let tokens = Budget { scope: BudgetScope::Room, limit: Limit::Tokens(1000) };
        let l = Ledger::default();
        let why = budget_allows(&en, std::slice::from_ref(&dollars), &l, "m1", prices.as_ref()).unwrap_err();
        assert!(why.contains("dollar budget") && why.contains("`polyph trust` in /repo"), "{why}");
        assert!(budget_allows(&en, std::slice::from_ref(&tokens), &l, "m1", prices.as_ref()).is_ok());
        // Trusted prices, or a dollar budget for someone else: no block.
        assert!(budget_allows(&en, std::slice::from_ref(&dollars), &l, "m1", None).is_ok());
        let other = Budget { scope: BudgetScope::Member("m2".into()), limit: Limit::Dollars(5.0) };
        assert!(budget_allows(&en, &[other], &l, "m1", prices.as_ref()).is_ok());
        assert!(only_ignored(&Loaded { ignored: vec!["agents"], ..loaded }, &["prices"]).is_none());
    }
}
