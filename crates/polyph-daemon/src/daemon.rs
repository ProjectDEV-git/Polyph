//! The daemon: owns the device identity, every room on this device, and
//! the method table behind the local API.

use std::collections::HashMap;
use std::net::IpAddr;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use polyph_core::{RoomCore, SavedRoom};
use polyph_proto::cert::Role;
use polyph_proto::codec::{from_cbor, to_cbor};
use polyph_proto::crdt::{Board, CardView, NewCard};
use polyph_proto::identity::{DeviceIdentity, ctx as sigctx};
use polyph_proto::ids::{MemberId, ObjectId, RoomId};
use polyph_proto::join::{JoinReply, JoinRequest};
use polyph_proto::joincode::JoinCode;
use polyph_proto::messages::{ApprovalScope, Decision, DocName, Surface};
use polyph_proto::pake::{PakeRole, PakeStart, SecureChannel};
use polyph_proto::relay_wire::{ClientFrame, CodeKind, ErrorCode, RelayFrame, end_room_message};
use polyph_proto::status::{StatusApproval, StatusSnapshot};
use polyph_relay::client::{RelayClient, TlsMode};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tokio::sync::{Mutex, broadcast};
use tracing::{debug, info};
use zeroize::Zeroizing;

use crate::clock::Clock;
use crate::config::UserConfig;
use crate::events::ApiEvent;
use crate::paths::{Paths, write_private};
use crate::runtime::{RelayAddr, RoomRuntime, apply, host_code, upload_snapshot};
use crate::secrets::{KeyStore, StorageKey};

/// Errors returned to local API callers. Messages say what to do next.
#[derive(Debug, thiserror::Error)]
pub enum ApiError {
    #[error("{0}")]
    BadRequest(String),
    #[error("{0}")]
    NotFound(String),
    #[error("{0}")]
    Forbidden(String),
    #[error("{0}")]
    Failed(String),
}

impl ApiError {
    pub fn code(&self) -> i64 {
        match self {
            ApiError::BadRequest(_) => -32602,
            ApiError::NotFound(_) => 404,
            ApiError::Forbidden(_) => 403,
            ApiError::Failed(_) => -32000,
        }
    }
}

impl From<std::io::Error> for ApiError {
    fn from(e: std::io::Error) -> Self {
        ApiError::Failed(e.to_string())
    }
}

impl From<anyhow::Error> for ApiError {
    fn from(e: anyhow::Error) -> Self {
        ApiError::Failed(e.to_string())
    }
}

impl From<polyph_core::RoomError> for ApiError {
    fn from(e: polyph_core::RoomError) -> Self {
        match e {
            polyph_core::RoomError::Forbidden => {
                ApiError::Forbidden("your role in this room does not allow that".into())
            }
            polyph_core::RoomError::NotFound(w) => ApiError::NotFound(format!("no such {w}")),
            other => ApiError::Failed(other.to_string()),
        }
    }
}

pub type ApiResult = Result<Value, ApiError>;

#[derive(Debug, Clone)]
pub struct DaemonOptions {
    pub paths: Paths,
    pub clock: Clock,
    pub tick_every: Duration,
    /// Address the LAN relay binds to.
    pub lan_bind: IpAddr,
    /// Advertise LAN rooms over mDNS.
    pub mdns: bool,
    pub surface: Surface,
    /// Keep the storage key in a private file instead of the OS keychain.
    pub file_keystore: bool,
    /// Command line of the sandbox helper (`polyph __sandbox` by default).
    pub sandbox_helper: Vec<String>,
    /// Force a sandbox mechanism instead of picking the best available.
    pub sandbox: Option<polyph_sandbox::Mechanism>,
    /// Command line of the MCP server given to agents (`polyph mcp` by default).
    pub mcp_command: Vec<String>,
    /// D-Bus for the status service (Linux): "off", "session" (default, when
    /// a session bus exists) or a bus address.
    pub dbus: String,
    /// Agents defined by the embedding program (tests add the fake agent).
    pub extra_agents: std::collections::BTreeMap<String, polyph_agents::registry::AgentConfig>,
}

impl DaemonOptions {
    pub fn new(paths: Paths) -> Self {
        Self {
            paths,
            clock: Clock::system(),
            tick_every: Duration::from_secs(1),
            lan_bind: IpAddr::from([0, 0, 0, 0]),
            mdns: true,
            surface: Surface::Cli,
            file_keystore: false,
            sandbox_helper: std::env::current_exe()
                .map(|p| vec![p.to_string_lossy().into_owned(), "__sandbox".into()])
                .unwrap_or_default(),
            sandbox: None,
            dbus: "session".into(),
            mcp_command: std::env::current_exe()
                .map(|p| vec![p.to_string_lossy().into_owned(), "mcp".into()])
                .unwrap_or_default(),
            extra_agents: Default::default(),
        }
    }
}

/// A room's saved form, ready to encrypt and write (holds plaintext, wiped
/// when dropped).
pub struct RoomSnapshot {
    hexid: String,
    /// Taken in order under the room's lock, so a higher number is newer.
    seq: u64,
    bytes: Zeroizing<Vec<u8>>,
}

impl std::fmt::Debug for RoomSnapshot {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "RoomSnapshot({} #{}, {} bytes)", self.hexid, self.seq, self.bytes.len())
    }
}

pub struct Shared {
    pub paths: Paths,
    pub key: StorageKey,
    pub keystore: KeyStore,
    pub identity: Arc<DeviceIdentity>,
    pub config: parking_lot::Mutex<UserConfig>,
    pub clock: Clock,
    pub tick_every: Duration,
    pub opts: DaemonOptions,
    events: broadcast::Sender<ApiEvent>,
    /// The daemon, for work started from deep inside (agents' MCP calls).
    pub daemon: std::sync::OnceLock<std::sync::Weak<Daemon>>,
    /// Runs `finish_task` calls from agents (see `coord::FinishJob`).
    pub finisher: std::sync::OnceLock<tokio::sync::mpsc::UnboundedSender<crate::coord::FinishJob>>,
    save_seq: std::sync::atomic::AtomicU64,
    /// The newest snapshot written per room; held while writing, so writes
    /// never overlap and an older snapshot never replaces a newer one.
    room_writes: parking_lot::Mutex<HashMap<String, u64>>,
}

impl std::fmt::Debug for Shared {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Shared({})", self.identity.device_id().short())
    }
}

impl Shared {
    /// Gives a room engine the daemon's clock.
    pub fn with_clock(&self, mut core: RoomCore) -> RoomCore {
        let c = self.clock.clone();
        core.clock = Some(Arc::new(move || c.now()));
        core
    }

    pub fn emit(&self, e: ApiEvent) {
        let _ = self.events.send(e);
    }

    pub fn save_room(&self, core: &mut RoomCore) -> anyhow::Result<()> {
        let snapshot = self.room_snapshot(core)?;
        self.write_room(&snapshot)
    }

    /// The room's saved form, taken while the room is locked. Encrypting and
    /// writing it ([`Self::write_room`]) can happen after the lock is let go.
    pub fn room_snapshot(&self, core: &mut RoomCore) -> anyhow::Result<RoomSnapshot> {
        let seq = self.save_seq.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        Ok(RoomSnapshot { hexid: core.id.to_hex(), seq, bytes: Zeroizing::new(to_cbor(&core.save())?) })
    }

    pub fn write_room(&self, s: &RoomSnapshot) -> anyhow::Result<()> {
        let mut written = self.room_writes.lock();
        if written.get(&s.hexid).is_some_and(|&w| w > s.seq) {
            return Ok(());
        }
        let sealed = self.key.seal(s.hexid.as_bytes(), &s.bytes)?;
        write_private(&self.paths.room_file(&s.hexid), &sealed)?;
        written.insert(s.hexid.clone(), s.seq);
        Ok(())
    }

    fn load_room(&self, path: &std::path::Path) -> anyhow::Result<RoomCore> {
        let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or_default().to_string();
        let data = std::fs::read(path)?;
        let pt = self.key.open(stem.as_bytes(), &data)?;
        let saved: SavedRoom = from_cbor(&pt)?;
        let mut core = RoomCore::load(self.identity.clone(), saved, self.clock.now())?;
        if core.is_agent_host(&core.me) {
            core.surface = polyph_proto::messages::Surface::AgentHost;
        }
        Ok(core)
    }

    pub fn member_id(&self) -> anyhow::Result<MemberId> {
        let mut cfg = self.config.lock();
        if let Some(m) = cfg.member.as_deref().and_then(|h| MemberId::from_hex(h).ok()) {
            return Ok(m);
        }
        let m = MemberId::random()?;
        cfg.member = Some(m.to_hex());
        cfg.save(&self.paths)?;
        Ok(m)
    }

    pub fn display_name(&self) -> String {
        self.config.lock().name.clone().unwrap_or_else(|| "Guest".into())
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct RoomInfo {
    pub id: String,
    pub name: String,
    pub quick: bool,
    pub role: Role,
    pub relay: String,
    pub connected: bool,
    /// Why the relay is unreachable, when it is. Absent while connected.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub relay_error: Option<crate::runtime::RelayErrorInfo>,
    pub open: bool,
    pub members_online: usize,
    pub members: usize,
    pub epoch: u64,
    pub lan: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct MemberInfo {
    pub member: String,
    pub name: String,
    pub role: Role,
    pub online: bool,
    pub hue: u16,
    pub devices: usize,
    pub me: bool,
}

pub struct Daemon {
    pub shared: Arc<Shared>,
    pub agents: crate::agents::Agents,
    /// Start requests waiting for this member's agent hosts.
    pub hosts: crate::hosts::Hosts,
    /// Agents the person runs themselves, connected through `polyph mcp`.
    pub external: crate::external::External,
    rooms: Mutex<HashMap<RoomId, Arc<RoomRuntime>>>,
    open: Mutex<Vec<RoomId>>,
    pub shutdown: tokio::sync::Notify,
}

impl std::fmt::Debug for Daemon {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Daemon({:?})", self.shared)
    }
}

fn param<T: for<'de> Deserialize<'de>>(params: &Value) -> Result<T, ApiError> {
    serde_json::from_value(params.clone()).map_err(|e| ApiError::BadRequest(format!("invalid parameters: {e}")))
}

#[derive(Deserialize)]
struct RoomParam {
    room: String,
}

/// A card by id prefix.
fn find_card(c: &mut crate::runtime::RoomCtx, q: &str) -> Result<String, ApiError> {
    let q = q.to_lowercase();
    let hits: Vec<String> = Board::new(c.core.doc(&DocName::Board))
        .all_cards()
        .into_iter()
        .filter(|x| !q.is_empty() && x.id.starts_with(&q))
        .map(|x| x.id)
        .collect();
    match hits.as_slice() {
        [one] => Ok(one.clone()),
        [] => Err(ApiError::NotFound("no such card".into())),
        _ => Err(ApiError::BadRequest("that id matches several cards; give more of it".into())),
    }
}

fn ok<T: Serialize>(v: T) -> ApiResult {
    serde_json::to_value(v).map_err(|e| ApiError::Failed(e.to_string()))
}

impl Daemon {
    pub async fn start(opts: DaemonOptions) -> anyhow::Result<Arc<Self>> {
        let paths = opts.paths.clone();
        paths.ensure()?;
        let (key, keystore) = crate::secrets::storage_key(&paths, opts.file_keystore)?;
        let identity = crate::secrets::identity(&paths, &key)?;
        let config = UserConfig::load(&paths)?;
        let (events, _) = broadcast::channel(4096);
        let shared = Arc::new(Shared {
            paths: paths.clone(),
            key,
            keystore,
            identity,
            config: parking_lot::Mutex::new(config),
            clock: opts.clock.clone(),
            tick_every: opts.tick_every,
            opts,
            events,
            daemon: std::sync::OnceLock::new(),
            finisher: std::sync::OnceLock::new(),
            save_seq: std::sync::atomic::AtomicU64::new(0),
            room_writes: parking_lot::Mutex::new(HashMap::new()),
        });
        let d = Arc::new(Self {
            shared: shared.clone(),
            agents: Default::default(),
            hosts: Default::default(),
            external: Default::default(),
            rooms: Mutex::new(HashMap::new()),
            open: Mutex::new(vec![]),
            shutdown: tokio::sync::Notify::new(),
        });
        let _ = shared.daemon.set(Arc::downgrade(&d));
        tokio::spawn(crate::notify::run(Arc::downgrade(&d), d.subscribe()));
        tokio::spawn(crate::hosts::run(Arc::downgrade(&d), d.subscribe()));
        tokio::spawn(crate::plugins::run(Arc::downgrade(&d), d.subscribe()));
        tokio::spawn(crate::github::run(Arc::downgrade(&d)));
        tokio::spawn(crate::sync::run(Arc::downgrade(&d)));
        {
            // Closes connections of agents that went quiet.
            let weak = Arc::downgrade(&d);
            tokio::spawn(async move {
                loop {
                    tokio::time::sleep(Duration::from_secs(60)).await;
                    let Some(d) = weak.upgrade() else { break };
                    crate::external::reap(&d).await;
                }
            });
        }
        #[cfg(target_os = "linux")]
        {
            let addr = match d.shared.opts.dbus.as_str() {
                "off" => None,
                "session" => std::env::var("DBUS_SESSION_BUS_ADDRESS").ok().map(|_| None),
                other => Some(Some(other.to_string())),
            };
            if let Some(addr) = addr
                && let Err(e) = crate::dbus::serve(&d, addr).await
            {
                // Optional: a desktop without a session bus just has no D-Bus status.
                tracing::info!(error = %e, "D-Bus status service not started");
            }
        }
        {
            let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<crate::coord::FinishJob>();
            let _ = shared.finisher.set(tx);
            let weak = Arc::downgrade(&d);
            tokio::spawn(async move {
                while let Some(job) = rx.recv().await {
                    let Some(d) = weak.upgrade() else { break };
                    tokio::spawn(async move {
                        let r = match d.agent(&job.agent.to_hex()).await {
                            Ok(h) => crate::tasks::finish(d.clone(), h, job.message).await,
                            Err(e) => Err(e),
                        };
                        let _ = job.reply.send(r);
                    });
                }
            });
        }
        // Load every saved room; reopen the ones that were open.
        let open_list: Vec<String> = std::fs::read(paths.home.join("open_rooms.json"))
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default();
        if let Ok(entries) = std::fs::read_dir(paths.rooms()) {
            for e in entries.flatten() {
                if e.path().extension().is_some_and(|x| x == "room") {
                    match shared.load_room(&e.path()) {
                        Ok(mut core) => {
                            core.surface = shared.opts.surface;
                            let relay = RelayAddr::parse(core.relay.as_deref().unwrap_or(&shared.config.lock().relay));
                            let id = core.id;
                            let rt = RoomRuntime::new(shared.with_clock(core), relay);
                            crate::runtime::load_codes(&mut *rt.ctx.lock().await, &shared, shared.clock.now());
                            d.rooms.lock().await.insert(id, rt.clone());
                            if open_list.contains(&id.to_hex()) {
                                rt.start(shared.clone());
                                d.open.lock().await.push(id);
                            }
                        }
                        Err(e) => tracing::warn!(file = %e, "could not load a saved room"),
                    }
                }
            }
        }
        info!(device = %shared.identity.device_id().short(), "daemon started");
        {
            // SEC-13: commits task agents made in their own object stores
            // during a turn the daemon did not finish go into the shared
            // store now; and say once when the daemon's own git writes run
            // without a sandbox.
            let home = shared.paths.home.clone();
            tokio::task::spawn_blocking(move || {
                if polyph_git::confine::confinement() == polyph_git::confine::Confinement::None {
                    tracing::warn!(
                        "no sandbox for Polyph's own git writes on this machine (needs bubblewrap or Seatbelt); \
                         they run unconfined, with the link check alone"
                    );
                }
                for (repo, branch, error) in crate::folders::import_task_branches(&home) {
                    tracing::warn!(repo = %repo.display(), %branch, %error,
                        "could not take a task's commits into the shared object store");
                }
            });
        }
        Ok(d)
    }

    pub fn subscribe(&self) -> broadcast::Receiver<ApiEvent> {
        self.shared.events.subscribe()
    }

    async fn save_open_list(&self) {
        let list: Vec<String> = self.open.lock().await.iter().map(|r| r.to_hex()).collect();
        let _ = std::fs::write(
            self.shared.paths.home.join("open_rooms.json"),
            serde_json::to_vec(&list).unwrap_or_default(),
        );
    }

    /// Open rooms, the current one last.
    pub async fn open_rooms(&self) -> Vec<Arc<RoomRuntime>> {
        let open = self.open.lock().await.clone();
        let rooms = self.rooms.lock().await;
        open.iter().filter_map(|id| rooms.get(id).cloned()).collect()
    }

    /// Every open room.
    pub async fn room_list(&self) -> Vec<Arc<RoomRuntime>> {
        self.rooms.lock().await.values().cloned().collect()
    }

    pub async fn find(&self, q: &str) -> Result<Arc<RoomRuntime>, ApiError> {
        let rooms = self.rooms.lock().await;
        let ql = q.to_lowercase();
        let mut hits = vec![];
        for (id, rt) in rooms.iter() {
            let hexid = id.to_hex();
            let name = rt.ctx.lock().await.core.name.to_lowercase();
            if hexid == ql || (ql.len() >= 4 && hexid.starts_with(&ql)) || name == ql {
                hits.push(rt.clone());
            }
        }
        match hits.len() {
            1 => Ok(hits.remove(0)),
            0 => Err(ApiError::NotFound(format!("no room matches \"{q}\"; run `polyph room list` to see your rooms"))),
            _ => Err(ApiError::BadRequest(format!("\"{q}\" matches several rooms; use more of the room id"))),
        }
    }

    async fn info(&self, rt: &RoomRuntime) -> RoomInfo {
        let open = self.open.lock().await.contains(&rt.id);
        let ctx = rt.ctx.lock().await;
        let now = self.shared.clock.now();
        RoomInfo {
            id: rt.id.to_hex(),
            name: ctx.core.name.clone(),
            quick: ctx.core.quick,
            role: ctx.core.role(),
            relay: ctx.relay.url.clone(),
            connected: ctx.connected,
            relay_error: ctx.relay_error.clone(),
            open,
            members_online: ctx
                .core
                .peers
                .values()
                .filter(|p| p.online(now))
                .filter_map(|p| p.member())
                .collect::<std::collections::HashSet<_>>()
                .len()
                + 1,
            members: ctx
                .core
                .peers
                .values()
                .filter_map(|p| p.member())
                .filter(|m| *m != ctx.core.my_member)
                .collect::<std::collections::HashSet<_>>()
                .len()
                + 1,
            epoch: ctx.core.epoch(),
            lan: ctx.lan.as_ref().map(|l| l.addr.to_string_with_pin()),
        }
    }

    async fn add_and_open(&self, core: RoomCore, relay: RelayAddr) -> Arc<RoomRuntime> {
        let id = core.id;
        let rt = RoomRuntime::new(self.shared.with_clock(core), relay);
        {
            let mut c = rt.ctx.lock().await;
            let _ = self.shared.save_room(&mut c.core);
        }
        rt.start(self.shared.clone());
        self.rooms.lock().await.insert(id, rt.clone());
        let mut open = self.open.lock().await;
        open.retain(|r| *r != id);
        open.push(id);
        drop(open);
        self.save_open_list().await;
        rt
    }

    /// Runs an API call from inside another one as its own task. `call` is one
    /// large state machine; awaiting it inside itself nests two of its poll
    /// frames on one thread stack, which overflowed tokio's 2 MiB workers in
    /// debug builds. A separate task starts again at the top of a stack.
    /// The boxed `dyn Future + Send` return type also breaks the `Send`
    /// inference cycle between `call` and this method.
    pub(crate) fn call_nested(
        self: &Arc<Self>,
        method: &'static str,
        params: Value,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = ApiResult> + Send>> {
        let d = self.clone();
        Box::pin(async move {
            match tokio::spawn(async move { d.call(method, params).await }).await {
                Ok(r) => r,
                Err(e) => Err(ApiError::Failed(format!("{method} did not finish: {e}"))),
            }
        })
    }

    /// Dispatches one local API call.
    pub async fn call(self: &Arc<Self>, method: &str, params: Value) -> ApiResult {
        let now = self.shared.clock.now();
        match method {
            "ping" => {
                // Agents on this device; `polyph update` restarts the daemon only when none are.
                let agents = self.agents.map.lock().await.len();
                ok(
                    json!({ "version": env!("CARGO_PKG_VERSION"), "protocol": polyph_proto::PROTOCOL_VERSION, "agents_running": agents }),
                )
            }
            "whoami" => {
                let cfg = self.shared.config.lock().clone();
                ok(json!({
                    "device": self.shared.identity.device_id().to_hex(),
                    "member": self.shared.member_id()?.to_hex(),
                    "name": cfg.name,
                    "keystore": format!("{:?}", self.shared.keystore).to_lowercase(),
                    "relay": cfg.relay,
                }))
            }
            "shutdown" => {
                self.shutdown.notify_waiters();
                ok(json!({}))
            }
            "config.get" => {
                #[derive(Deserialize)]
                struct P {
                    key: Option<String>,
                }
                let p: P = param(&params)?;
                let cfg = self.shared.config.lock().clone();
                match p.key {
                    Some(k) => ok(json!({ "key": k, "value": cfg.get(&k) })),
                    None => ok(cfg),
                }
            }
            "config.set" => {
                #[derive(Deserialize)]
                struct P {
                    key: String,
                    value: String,
                }
                let p: P = param(&params)?;
                let mut cfg = self.shared.config.lock();
                cfg.set(&p.key, &p.value).map_err(|e| ApiError::BadRequest(e.to_string()))?;
                cfg.save(&self.shared.paths)?;
                ok(json!({}))
            }
            "room.create" => {
                #[derive(Deserialize)]
                struct P {
                    #[serde(default)]
                    name: String,
                    #[serde(default)]
                    quick: bool,
                    #[serde(default = "yes")]
                    require_approval: bool,
                    relay: Option<String>,
                    #[serde(default)]
                    lan: bool,
                    #[serde(default)]
                    bridge: bool,
                }
                fn yes() -> bool {
                    true
                }
                let p: P = param(&params)?;
                if !p.quick && p.name.trim().is_empty() {
                    return Err(ApiError::BadRequest(
                        "a room needs a name, for example `polyph room create payments`".into(),
                    ));
                }
                let member = self.shared.member_id()?;
                let (mut core, _genesis) = RoomCore::create(
                    self.shared.identity.clone(),
                    member,
                    p.name.trim(),
                    &self.shared.display_name(),
                    p.quick,
                    now,
                )?;
                core.surface = self.shared.opts.surface;
                if !p.require_approval {
                    let fx = core.change_doc(DocName::Settings, |d| {
                        polyph_core::docs::set_setting(d, "require_approval", &json!(false))
                    })?;
                    drop(fx);
                }
                let internet = RelayAddr::parse(&p.relay.unwrap_or_else(|| self.shared.config.lock().relay.clone()));
                let (relay, lan) = if p.lan {
                    let lan =
                        crate::lan::start(p.name.trim(), self.shared.opts.lan_bind, self.shared.opts.mdns).await?;
                    (lan.addr.clone(), Some(lan))
                } else {
                    (internet.clone(), None)
                };
                core.relay = Some(relay.to_string_with_pin());
                let chain = core.my_chain.clone();
                let rt = self.add_and_open(core, relay).await;
                if let Some(mut lan) = lan {
                    let fp = lan.fingerprint;
                    if p.bridge {
                        crate::lan::start_bridge(&mut lan, rt.id, chain, self.shared.identity.clone(), internet);
                    }
                    let mut c = rt.ctx.lock().await;
                    c.binding = fp.to_vec();
                    c.lan = Some(lan);
                }
                ok(self.info(&rt).await)
            }
            "room.list" => {
                let rooms: Vec<Arc<RoomRuntime>> = self.rooms.lock().await.values().cloned().collect();
                let mut out = vec![];
                for rt in rooms {
                    out.push(self.info(&rt).await);
                }
                out.sort_by(|a, b| a.name.cmp(&b.name));
                ok(out)
            }
            "room.get" => {
                let p: RoomParam = param(&params)?;
                let rt = self.find(&p.room).await?;
                ok(self.info(&rt).await)
            }
            "room.open" => {
                let p: RoomParam = param(&params)?;
                let rt = self.find(&p.room).await?;
                let mut open = self.open.lock().await;
                if !open.contains(&rt.id) {
                    open.push(rt.id);
                    rt.start(self.shared.clone());
                } else {
                    // Already open: move to the front as the current room.
                    open.retain(|r| *r != rt.id);
                    open.push(rt.id);
                }
                drop(open);
                self.save_open_list().await;
                ok(self.info(&rt).await)
            }
            "room.close" => {
                let p: RoomParam = param(&params)?;
                let rt = self.find(&p.room).await?;
                self.close_room(&rt).await;
                ok(json!({}))
            }
            "room.offline" => {
                #[derive(Deserialize)]
                struct P {
                    room: String,
                    offline: bool,
                }
                let p: P = param(&params)?;
                let rt = self.find(&p.room).await?;
                rt.ctx.lock().await.offline = p.offline;
                rt.kick.notify_one();
                ok(json!({}))
            }
            "room.delete" => {
                let p: RoomParam = param(&params)?;
                let rt = self.find(&p.room).await?;
                self.close_room(&rt).await;
                self.rooms.lock().await.remove(&rt.id);
                let _ = std::fs::remove_file(self.shared.paths.room_file(&rt.id.to_hex()));
                let _ = std::fs::remove_file(self.shared.paths.room_codes_file(&rt.id.to_hex()));
                ok(json!({}))
            }
            "room.end" => {
                #[derive(Deserialize)]
                struct P {
                    room: String,
                    export_dir: Option<PathBuf>,
                }
                let p: P = param(&params)?;
                let rt = self.find(&p.room).await?;
                let exported = {
                    let mut c = rt.ctx.lock().await;
                    if c.core.role() != Role::Owner {
                        return Err(ApiError::Forbidden(
                            "only the room owner can end it; use `polyph leave` instead".into(),
                        ));
                    }
                    let path = match p.export_dir {
                        Some(dir) => Some(crate::export::write(&mut c.core, &dir, now)?),
                        None => None,
                    };
                    let sig = self.shared.identity.sign(sigctx::ROOM_END, &end_room_message(&rt.id, now));
                    if let Some(s) = &c.sender {
                        let _ = s.send(ClientFrame::EndRoom { room: rt.id, at: now, sig }).await;
                    }
                    path
                };
                tokio::time::sleep(Duration::from_millis(200)).await;
                self.close_room(&rt).await;
                self.rooms.lock().await.remove(&rt.id);
                let _ = std::fs::remove_file(self.shared.paths.room_file(&rt.id.to_hex()));
                let _ = std::fs::remove_file(self.shared.paths.room_codes_file(&rt.id.to_hex()));
                ok(json!({ "exported": exported }))
            }
            "room.invite" => {
                #[derive(Deserialize)]
                struct P {
                    room: String,
                    #[serde(default)]
                    reusable: bool,
                    expires_secs: Option<i64>,
                    max_uses: Option<u32>,
                }
                let p: P = param(&params)?;
                let rt = self.find(&p.room).await?;
                // Plain invites never let the joiner claim this member id.
                self.invite(&rt, p.reusable, p.expires_secs, p.max_uses, false).await
            }
            "phone.pair" => {
                // A one-time code plus this person's member id, as a link for
                // their phone (crates/polyph-daemon/src/phone.rs).
                #[derive(Deserialize)]
                struct P {
                    room: String,
                    base: Option<String>,
                }
                let p: P = param(&params)?;
                let rt = self.find(&p.room).await?;
                let relay = rt.ctx.lock().await.relay.clone();
                if relay.pin.is_some() {
                    return Err(ApiError::BadRequest(
                        "this room uses a LAN relay with its own certificate, which phone browsers cannot reach; host it on an internet relay to pair a phone".into(),
                    ));
                }
                // A one-time code for another device of this member.
                let inv = self.invite(&rt, false, None, None, true).await?;
                let code = inv["code"].as_str().unwrap_or_default().to_string();
                let member = self.shared.member_id()?.to_hex();
                let base = p.base.unwrap_or_else(crate::phone::default_base);
                let link = crate::phone::link(&base, &code, &member, &relay.url);
                ok(json!({
                    "code": code,
                    "expires_at": inv["expires_at"],
                    "member": member,
                    "relay": relay.url,
                    "link": link,
                    "svg": crate::phone::qr_svg(&link),
                }))
            }
            "room.revoke_code" => {
                #[derive(Deserialize)]
                struct P {
                    room: String,
                    code: String,
                }
                let p: P = param(&params)?;
                let rt = self.find(&p.room).await?;
                let code = JoinCode::parse(&p.code).map_err(|e| ApiError::BadRequest(e.to_string()))?;
                let mut c = rt.ctx.lock().await;
                c.codes.remove(&code.rendezvous());
                // At once: a closed room has no tick to save it, and a
                // revoked code must not come back after a crash.
                crate::runtime::save_codes(&mut c, &self.shared, now);
                if let Some(s) = &c.sender {
                    let _ = s.send(ClientFrame::RevokeCode { rendezvous: code.rendezvous() }).await;
                }
                ok(json!({}))
            }
            "room.members" => {
                let p: RoomParam = param(&params)?;
                let rt = self.find(&p.room).await?;
                let c = rt.ctx.lock().await;
                let mut by: HashMap<MemberId, MemberInfo> = HashMap::new();
                by.insert(
                    c.core.my_member,
                    MemberInfo {
                        member: c.core.my_member.to_hex(),
                        name: c.core.my_profile.display_name.clone(),
                        role: c.core.role(),
                        online: true,
                        hue: c.core.my_profile.hue,
                        devices: 1,
                        me: true,
                    },
                );
                for p in c.core.peers.values() {
                    let Some(m) = p.member() else { continue };
                    let e = by.entry(m).or_insert(MemberInfo {
                        member: m.to_hex(),
                        name: p.name(),
                        role: p.role(),
                        online: false,
                        hue: p.profile.as_ref().map(|x| x.hue).unwrap_or(u16::MAX),
                        devices: 0,
                        me: false,
                    });
                    if !e.me {
                        e.devices += 1;
                    }
                    e.online |= p.online(now);
                    if e.name.is_empty() {
                        e.name = p.name();
                    }
                    e.role = e.role.max(p.role());
                }
                let mut v: Vec<MemberInfo> = by.into_values().collect();
                v.sort_by(|a, b| b.role.cmp(&a.role).then(a.name.cmp(&b.name)));
                ok(v)
            }
            "room.set_role" | "room.remove" => {
                #[derive(Deserialize)]
                struct P {
                    room: String,
                    member: String,
                    role: Option<Role>,
                }
                let p: P = param(&params)?;
                let rt = self.find(&p.room).await?;
                let mut c = rt.ctx.lock().await;
                let member = resolve_member(&c.core, &p.member)?;
                let fx = if method == "room.remove" {
                    c.core.remove_member(member, now)?
                } else {
                    c.core.change_role(
                        member,
                        p.role.ok_or_else(|| ApiError::BadRequest("role is required".into()))?,
                        now,
                    )?
                };
                apply(&rt, &mut c, &self.shared, fx).await;
                ok(json!({}))
            }
            "room.settings" => {
                #[derive(Deserialize)]
                struct P {
                    room: String,
                    require_approval: Option<bool>,
                }
                let p: P = param(&params)?;
                let rt = self.find(&p.room).await?;
                let mut c = rt.ctx.lock().await;
                if let Some(v) = p.require_approval {
                    if !c.core.role().can_approve_joins() {
                        return Err(ApiError::Forbidden("only Owners and Maintainers change join approval".into()));
                    }
                    let fx = c.core.change_doc(DocName::Settings, |d| {
                        polyph_core::docs::set_setting(d, "require_approval", &json!(v))
                    })?;
                    apply(&rt, &mut c, &self.shared, fx).await;
                }
                let require = c.core.require_approval();
                ok(json!({ "require_approval": require }))
            }
            "knock.list" => {
                let p: RoomParam = param(&params)?;
                let rt = self.find(&p.room).await?;
                let c = rt.ctx.lock().await;
                // `own_device`: another device joining as this member, through
                // a code `phone.pair` or `host.invite` made here.
                let list: Vec<Value> = c
                    .core
                    .knocks
                    .values()
                    .map(|k| {
                        let mut v = serde_json::to_value(k).unwrap_or(Value::Null);
                        if let Some(o) = v.as_object_mut() {
                            o.insert("own_device".into(), c.core.is_own_device_knock(&k.id).into());
                        }
                        v
                    })
                    .collect();
                ok(list)
            }
            "knock.decide" => {
                #[derive(Deserialize)]
                struct P {
                    room: String,
                    knock: String,
                    approve: bool,
                    role: Option<Role>,
                    /// Shown to a denied joiner.
                    note: Option<String>,
                }
                let p: P = param(&params)?;
                let rt = self.find(&p.room).await?;
                let mut c = rt.ctx.lock().await;
                let id = c
                    .core
                    .knocks
                    .keys()
                    .find(|k| k.to_hex().starts_with(&p.knock.to_lowercase()))
                    .copied()
                    .ok_or_else(|| ApiError::NotFound("no such knock".into()))?;
                let role = p.role.unwrap_or(Role::Member);
                let fx = if p.approve {
                    c.core.approve_knock(id, role, now)?
                } else {
                    c.core.deny_knock_with_note(id, p.note)?
                };
                apply(&rt, &mut c, &self.shared, fx).await;
                // The role the new member's certificate carries.
                ok(if p.approve { json!({ "role": role }) } else { json!({}) })
            }
            "join" => self.join(param(&params)?).await,
            "chat.send" => {
                #[derive(Deserialize)]
                struct P {
                    room: String,
                    text: String,
                    thread: Option<String>,
                }
                let p: P = param(&params)?;
                if p.text.trim().is_empty() {
                    return Err(ApiError::BadRequest("message is empty".into()));
                }
                let rt = self.find(&p.room).await?;
                let mut c = rt.ctx.lock().await;
                if !c.core.role().can_write() {
                    return Err(ApiError::Forbidden(
                        "Viewers can read the room but not write in it; an Owner or Maintainer can make you a Member"
                            .into(),
                    ));
                }
                let fx = c.core.send_chat(&p.text, p.thread, None, now)?;
                apply(&rt, &mut c, &self.shared, fx).await;
                ok(json!({}))
            }
            "chat.list" => {
                #[derive(Deserialize)]
                struct P {
                    room: String,
                    limit: Option<usize>,
                }
                let p: P = param(&params)?;
                let rt = self.find(&p.room).await?;
                let mut c = rt.ctx.lock().await;
                let mut v = c.core.chat();
                let names = member_names(&c.core);
                if let Some(l) = p.limit {
                    let skip = v.len().saturating_sub(l);
                    v.drain(..skip);
                }
                let out: Vec<Value> = v.into_iter().map(|e| json!({ "id": e.id, "author": e.author, "name": names.get(&e.author).cloned().unwrap_or_default(), "agent": e.agent, "thread": e.thread, "text": e.text, "at": e.at })).collect();
                ok(out)
            }
            "board.get" => {
                let p: RoomParam = param(&params)?;
                let rt = self.find(&p.room).await?;
                let mut c = rt.ctx.lock().await;
                let doc = c.core.doc(&DocName::Board);
                let b = Board::new(doc);
                let cols = b.columns();
                let cards = b.all_cards();
                ok(
                    json!({ "columns": cols.into_iter().map(|(id, title)| json!({"id": id, "title": title})).collect::<Vec<_>>(), "cards": cards }),
                )
            }
            "board.add" => {
                #[derive(Deserialize)]
                struct P {
                    room: String,
                    title: String,
                    #[serde(default)]
                    description: String,
                    column: Option<String>,
                    #[serde(default)]
                    acceptance: Vec<String>,
                    #[serde(default)]
                    files: Vec<String>,
                }
                let p: P = param(&params)?;
                let rt = self.find(&p.room).await?;
                let mut c = rt.ctx.lock().await;
                let id = ObjectId::random().map_err(|e| ApiError::Failed(e.to_string()))?.to_hex();
                let card = NewCard {
                    id: id.clone(),
                    title: p.title,
                    description: p.description,
                    column: p.column.unwrap_or_else(|| "backlog".into()),
                    created_by: c.core.my_member.to_hex(),
                    created_at: now,
                    acceptance: p.acceptance,
                    files: p.files,
                    ..Default::default()
                };
                let fx = c.core.change_doc(DocName::Board, |d| {
                    let _ = Board::new(d).add_card(&card);
                })?;
                apply(&rt, &mut c, &self.shared, fx).await;
                ok(json!({ "id": id }))
            }
            "board.move" => {
                #[derive(Deserialize)]
                struct P {
                    room: String,
                    card: String,
                    column: String,
                    after: Option<String>,
                }
                let p: P = param(&params)?;
                let rt = self.find(&p.room).await?;
                let mut c = rt.ctx.lock().await;
                let cards: Vec<CardView> = Board::new(c.core.doc(&DocName::Board)).all_cards();
                let card = cards
                    .iter()
                    .find(|x| x.id.starts_with(&p.card))
                    .ok_or_else(|| ApiError::NotFound("no such card".into()))?
                    .id
                    .clone();
                let mut res = Ok(());
                let fx = c.core.change_doc(DocName::Board, |d| {
                    res = Board::new(d).move_card(&card, &p.column, p.after.as_deref(), now)
                })?;
                res.map_err(|e| ApiError::BadRequest(e.to_string()))?;
                apply(&rt, &mut c, &self.shared, fx).await;
                crate::linear::spawn_card_moved(&self.shared, rt.id, card, p.column);
                ok(json!({}))
            }
            "board.update" => {
                #[derive(Deserialize)]
                struct P {
                    room: String,
                    card: String,
                    title: Option<String>,
                    description: Option<String>,
                    /// Empty string clears it.
                    assignee: Option<String>,
                    acceptance: Option<Vec<String>>,
                    files: Option<Vec<String>>,
                    /// Cards (ids or unique prefixes) this one waits for.
                    depends_on: Option<Vec<String>>,
                }
                let p: P = param(&params)?;
                let rt = self.find(&p.room).await?;
                let mut c = rt.ctx.lock().await;
                let card = find_card(&mut c, &p.card)?;
                let deps = match &p.depends_on {
                    Some(d) => Some(d.iter().map(|x| find_card(&mut c, x)).collect::<Result<Vec<String>, _>>()?),
                    None => None,
                };
                let red = |s: &str| polyph_core::redact::redact(s).into_owned();
                let fx = c.core.change_doc(DocName::Board, |d| {
                    let b = Board::new(d);
                    let set = |k: &str, v: yrs::Any| {
                        let _ = b.set_field(&card, k, v, now);
                    };
                    if let Some(t) = &p.title {
                        set(polyph_proto::crdt::card::TITLE, yrs::Any::String(red(t).into()));
                    }
                    if let Some(t) = &p.description {
                        set(polyph_proto::crdt::card::DESCRIPTION, yrs::Any::String(red(t).into()));
                    }
                    if let Some(a) = &p.assignee {
                        set(
                            polyph_proto::crdt::card::ASSIGNEE,
                            if a.is_empty() { yrs::Any::Null } else { yrs::Any::String(a.as_str().into()) },
                        );
                    }
                    if let Some(l) = &p.acceptance {
                        let l: Vec<String> = l.iter().map(|x| red(x)).collect();
                        set(
                            polyph_proto::crdt::card::ACCEPTANCE,
                            yrs::Any::String(serde_json::to_string(&l).unwrap_or_default().into()),
                        );
                    }
                    if let Some(l) = &p.files {
                        set(
                            polyph_proto::crdt::card::FILES,
                            yrs::Any::String(serde_json::to_string(l).unwrap_or_default().into()),
                        );
                    }
                    if let Some(l) = &deps {
                        set(
                            polyph_proto::crdt::card::DEPENDS_ON,
                            yrs::Any::String(serde_json::to_string(l).unwrap_or_default().into()),
                        );
                    }
                })?;
                apply(&rt, &mut c, &self.shared, fx).await;
                ok(json!({ "id": card }))
            }
            "board.remove" => {
                #[derive(Deserialize)]
                struct P {
                    room: String,
                    card: String,
                }
                let p: P = param(&params)?;
                let rt = self.find(&p.room).await?;
                let mut c = rt.ctx.lock().await;
                let card = find_card(&mut c, &p.card)?;
                let fx = c.core.change_doc(DocName::Board, |d| Board::new(d).remove_card(&card))?;
                apply(&rt, &mut c, &self.shared, fx).await;
                ok(json!({ "id": card }))
            }
            "thread.create" => {
                #[derive(Deserialize)]
                struct P {
                    room: String,
                    title: String,
                    card: Option<String>,
                    text: Option<String>,
                }
                let p: P = param(&params)?;
                if p.title.trim().is_empty() {
                    return Err(ApiError::BadRequest("a thread needs a title".into()));
                }
                let rt = self.find(&p.room).await?;
                let mut c = rt.ctx.lock().await;
                let card = match &p.card {
                    Some(q) => Some(find_card(&mut c, q)?),
                    None => None,
                };
                let t = polyph_core::docs::ThreadInfo {
                    id: ObjectId::random().map_err(|e| ApiError::Failed(e.to_string()))?.to_hex(),
                    title: polyph_core::redact::redact(p.title.trim()).into_owned(),
                    card: card.clone(),
                    created_by: c.core.my_member.to_hex(),
                    created_at: now,
                };
                let fx = c.core.change_doc(DocName::Threads, |d| polyph_core::docs::create_thread(d, &t))?;
                apply(&rt, &mut c, &self.shared, fx).await;
                if let Some(card) = &card {
                    let fx = c.core.change_doc(DocName::Board, |d| {
                        let _ = Board::new(d).set_field(
                            card,
                            polyph_proto::crdt::card::THREAD,
                            yrs::Any::String(t.id.as_str().into()),
                            now,
                        );
                    })?;
                    apply(&rt, &mut c, &self.shared, fx).await;
                }
                if let Some(text) = p.text.filter(|x| !x.trim().is_empty()) {
                    let fx = c.core.send_chat(&text, Some(t.id.clone()), None, now)?;
                    apply(&rt, &mut c, &self.shared, fx).await;
                }
                crate::feed::log_locked(
                    &rt,
                    &mut c,
                    &self.shared,
                    "thread_created",
                    None,
                    json!({ "thread": t.id, "title": t.title }),
                )
                .await;
                ok(json!({ "id": t.id }))
            }
            "thread.list" => {
                let p: RoomParam = param(&params)?;
                let rt = self.find(&p.room).await?;
                let mut c = rt.ctx.lock().await;
                let threads = polyph_core::docs::threads(c.core.doc(&DocName::Threads));
                let chat = c.core.chat();
                let out: Vec<Value> = threads
                    .iter()
                    .map(|t| {
                        let posts: Vec<_> =
                            chat.iter().filter(|m| m.thread.as_deref() == Some(t.id.as_str())).collect();
                        json!({ "thread": t, "posts": posts.len(), "last_at": posts.last().map(|m| m.at) })
                    })
                    .collect();
                ok(out)
            }
            "thread.get" => {
                #[derive(Deserialize)]
                struct P {
                    room: String,
                    thread: String,
                }
                let p: P = param(&params)?;
                let rt = self.find(&p.room).await?;
                let mut c = rt.ctx.lock().await;
                let want = p.thread.to_lowercase();
                let t = polyph_core::docs::threads(c.core.doc(&DocName::Threads))
                    .into_iter()
                    .find(|t| t.id.starts_with(&want))
                    .ok_or_else(|| ApiError::NotFound("no such thread".into()))?;
                let names = member_names(&c.core);
                let posts: Vec<Value> = c
                    .core
                    .chat()
                    .into_iter()
                    .filter(|m| m.thread.as_deref() == Some(t.id.as_str()))
                    .map(|m| {
                        let name = names.get(&m.author).cloned().unwrap_or_default();
                        json!({ "id": m.id, "author": m.author, "name": name, "agent": m.agent, "text": m.text, "at": m.at })
                    })
                    .collect();
                ok(json!({ "thread": t, "posts": posts }))
            }
            "integrations.status" => ok(crate::integrations::status(&self.shared)),
            "github.connect" => {
                #[derive(Deserialize)]
                struct P {
                    token: String,
                    api_url: Option<String>,
                }
                let p: P = param(&params)?;
                crate::github::connect(&self.shared, p.token, p.api_url).await
            }
            "github.disconnect" => {
                crate::github::disconnect(&self.shared)?;
                ok(json!({}))
            }
            "github.import" => {
                let p: crate::github::ImportParams = param(&params)?;
                crate::github::import(self, p).await
            }
            "github.sync" => {
                let p: RoomParam = param(&params)?;
                let rt = self.find(&p.room).await?;
                let n = crate::github::sync_room(self, &rt).await?;
                ok(json!({ "changed": n }))
            }
            "linear.connect" => {
                #[derive(Deserialize)]
                struct P {
                    token: String,
                    api_url: Option<String>,
                }
                let p: P = param(&params)?;
                crate::linear::connect(&self.shared, p.token, p.api_url).await
            }
            "cloud.login.start" => crate::cloud::login_start(&self.shared, param(&params)?).await,
            "cloud.login.finish" => crate::cloud::login_finish(&self.shared, param(&params)?).await,
            "cloud.status" => crate::cloud::status(&self.shared).await,
            "cloud.logout" => crate::cloud::logout(&self.shared).await,
            "linear.disconnect" => {
                crate::linear::disconnect(&self.shared)?;
                ok(json!({}))
            }
            "linear.import" => {
                let p: crate::linear::ImportParams = param(&params)?;
                crate::linear::import(self, p).await
            }
            "webhook.add" => {
                let w: crate::notify::Webhook = serde_json::from_value({
                    let mut v = params.clone();
                    v["id"] = json!("");
                    if v.get("format").is_none() {
                        v["format"] = json!("json");
                    }
                    v
                })
                .map_err(|e| ApiError::BadRequest(e.to_string()))?;
                let w = crate::notify::add_webhook(&self.shared, w)?;
                ok(crate::notify::public(&w))
            }
            "webhook.list" => {
                ok(crate::notify::load_webhooks(&self.shared).iter().map(crate::notify::public).collect::<Vec<_>>())
            }
            "webhook.test" => {
                #[derive(Deserialize)]
                struct P {
                    id: String,
                }
                let p: P = param(&params)?;
                crate::notify::test_webhook(&self.shared, &p.id).await?;
                ok(json!({}))
            }
            "webhook.remove" => {
                #[derive(Deserialize)]
                struct P {
                    id: String,
                }
                let p: P = param(&params)?;
                if !crate::notify::remove_webhook(&self.shared, &p.id)? {
                    return Err(ApiError::NotFound("no such webhook".into()));
                }
                ok(json!({}))
            }
            "term.open" => {
                let p: crate::terms::OpenParams = param(&params)?;
                crate::terms::open(self, p).await
            }
            "term.list" => {
                let p: RoomParam = param(&params)?;
                let rt = self.find(&p.room).await?;
                ok(crate::terms::list(&rt).await)
            }
            "term.read" | "term.close" | "term.ask" | "term.release" | "term.revoke" => {
                #[derive(Deserialize)]
                struct P {
                    room: String,
                    term: String,
                }
                let p: P = param(&params)?;
                match method {
                    "term.read" => crate::terms::read(self, &p.room, &p.term).await,
                    "term.close" => crate::terms::close(self, &p.room, &p.term).await,
                    other => crate::terms::control(self, &p.room, &p.term, other.trim_start_matches("term.")).await,
                }
            }
            "term.input" => {
                #[derive(Deserialize)]
                struct P {
                    room: String,
                    term: String,
                    text: Option<String>,
                    data: Option<String>,
                }
                let p: P = param(&params)?;
                let bytes = crate::terms::input_bytes(p.text, p.data)?;
                crate::terms::input(self, &p.room, &p.term, bytes).await
            }
            "term.resize" => {
                #[derive(Deserialize)]
                struct P {
                    room: String,
                    term: String,
                    cols: u16,
                    rows: u16,
                }
                let p: P = param(&params)?;
                crate::terms::resize(self, &p.room, &p.term, p.cols, p.rows).await
            }
            "whiteboard.create" => {
                #[derive(Deserialize)]
                struct P {
                    room: String,
                    name: String,
                }
                let p: P = param(&params)?;
                let rt = self.find(&p.room).await?;
                let mut c = rt.ctx.lock().await;
                let info = crate::boards::create(&mut c, &p.name, now)?;
                let fx = crate::boards::create_effects(&mut c, &info)?;
                apply(&rt, &mut c, &self.shared, fx).await;
                crate::feed::log_locked(
                    &rt,
                    &mut c,
                    &self.shared,
                    "whiteboard_created",
                    None,
                    json!({ "board": info.id, "name": info.name }),
                )
                .await;
                ok(info)
            }
            "whiteboard.list" => {
                let p: RoomParam = param(&params)?;
                let rt = self.find(&p.room).await?;
                let mut c = rt.ctx.lock().await;
                ok(c.core.whiteboards())
            }
            "whiteboard.get" => {
                #[derive(Deserialize)]
                struct P {
                    room: String,
                    board: String,
                }
                let p: P = param(&params)?;
                let rt = self.find(&p.room).await?;
                let mut c = rt.ctx.lock().await;
                let info = crate::boards::find(&mut c, &p.board)?;
                let els = polyph_core::whiteboard::elements(c.core.doc(&DocName::Whiteboard(info.id.clone())));
                let text = polyph_core::whiteboard::describe(&info.name, &els);
                ok(json!({ "board": info, "elements": els, "text": text }))
            }
            "whiteboard.put" | "whiteboard.remove" => {
                #[derive(Deserialize)]
                struct P {
                    room: String,
                    board: String,
                    #[serde(default)]
                    elements: Vec<Value>,
                    #[serde(default)]
                    ids: Vec<String>,
                }
                let p: P = param(&params)?;
                let rt = self.find(&p.room).await?;
                let mut c = rt.ctx.lock().await;
                let info = crate::boards::find(&mut c, &p.board)?;
                let mut n = 0;
                let fx = c.core.change_doc(DocName::Whiteboard(info.id.clone()), |d| {
                    n = if method == "whiteboard.put" {
                        polyph_core::whiteboard::put(d, &p.elements).len()
                    } else {
                        polyph_core::whiteboard::remove(d, &p.ids)
                    };
                })?;
                apply(&rt, &mut c, &self.shared, fx).await;
                ok(json!({ "changed": n }))
            }
            "activity.list" => {
                #[derive(Deserialize)]
                struct P {
                    room: String,
                    since: Option<i64>,
                    #[serde(default)]
                    kinds: Vec<String>,
                    limit: Option<usize>,
                }
                let p: P = param(&params)?;
                let rt = self.find(&p.room).await?;
                let mut c = rt.ctx.lock().await;
                let mut v: Vec<polyph_proto::crdt::ActivityEntry> =
                    polyph_proto::crdt::Activity::new(c.core.doc(&DocName::Activity))
                        .entries()
                        .into_iter()
                        .filter(|e| p.since.is_none_or(|s| e.at >= s))
                        .filter(|e| p.kinds.is_empty() || p.kinds.contains(&e.kind))
                        .map(|mut e| {
                            // Checkpoints are for replay, not for reading.
                            e.checkpoint = e.checkpoint.map(|_| String::new());
                            e
                        })
                        .collect();
                if let Some(l) = p.limit {
                    let skip = v.len().saturating_sub(l);
                    v.drain(..skip);
                }
                ok(v)
            }
            "activity.replay" => {
                #[derive(Deserialize)]
                struct P {
                    room: String,
                    at: i64,
                }
                let p: P = param(&params)?;
                let rt = self.find(&p.room).await?;
                let mut c = rt.ctx.lock().await;
                let entries = polyph_proto::crdt::Activity::new(c.core.doc(&DocName::Activity)).entries();
                ok(polyph_core::activity::replay(&entries, p.at))
            }
            "claims.list" => {
                let p: RoomParam = param(&params)?;
                let rt = self.find(&p.room).await?;
                let c = rt.ctx.lock().await;
                ok(c.core.claims.all())
            }
            "task.finish" => {
                #[derive(Deserialize)]
                struct P {
                    agent: String,
                    message: Option<String>,
                }
                let p: P = param(&params)?;
                let h = self.agent(&p.agent).await?;
                crate::tasks::finish(self.clone(), h, p.message).await
            }
            "plan.create" => {
                let p: crate::plan::PlanParams = param(&params)?;
                crate::plan::create(self.clone(), p).await
            }
            "plan.accept" => {
                #[derive(Deserialize)]
                struct P {
                    room: String,
                    plan: String,
                }
                let p: P = param(&params)?;
                crate::plan::accept(self, &p.room, &p.plan).await
            }
            "agent.handoff" => {
                #[derive(Deserialize)]
                struct P {
                    agent: String,
                    to_kind: Option<String>,
                    note: Option<String>,
                }
                let p: P = param(&params)?;
                let h = self.agent(&p.agent).await?;
                crate::tasks::handoff(self.clone(), h, p.to_kind, p.note).await
            }
            "mirror.host" | "mirror.open" => {
                #[derive(Deserialize)]
                struct P {
                    room: String,
                    dir: PathBuf,
                }
                let p: P = param(&params)?;
                if method == "mirror.host" {
                    crate::mirror::host(self, &p.room, &p.dir).await
                } else {
                    crate::mirror::open(self, &p.room, &p.dir).await
                }
            }
            "mirror.status" => {
                let p: RoomParam = param(&params)?;
                let rt = self.find(&p.room).await?;
                ok(crate::mirror::status(&rt).await)
            }
            "merge.queue" => {
                let p: RoomParam = param(&params)?;
                let rt = self.find(&p.room).await?;
                ok(crate::tasks::queue(&rt).await)
            }
            "memory.list" => {
                #[derive(Deserialize)]
                struct P {
                    room: String,
                    #[serde(default)]
                    path: String,
                }
                let p: P = param(&params)?;
                let rt = self.find(&p.room).await?;
                let mut c = rt.ctx.lock().await;
                let all = polyph_core::memory::notes(c.core.doc(&DocName::Memory));
                ok(polyph_core::memory::notes_for(&all, &p.path))
            }
            "inbox.send" => {
                // A person writes to an agent. The agent reads it at its next turn.
                #[derive(Deserialize)]
                struct P {
                    room: String,
                    agent: String,
                    text: String,
                }
                let p: P = param(&params)?;
                let rt = self.find(&p.room).await?;
                let mut c = rt.ctx.lock().await;
                let want = p.agent.to_lowercase();
                let to = c
                    .transcripts
                    .values()
                    .filter_map(|log| log.front().map(|e| e.agent.clone()))
                    .find(|a| a.id.to_hex().starts_with(&want) || a.label.to_lowercase() == want)
                    .ok_or_else(|| ApiError::NotFound("no such agent".into()))?;
                let (m, fx) = c
                    .core
                    .send_inbox(to, None, None, None, &p.text, 0, now)
                    .map_err(|e| ApiError::BadRequest(e.to_string()))?;
                apply(&rt, &mut c, &self.shared, fx).await;
                ok(json!({ "id": m.id.to_hex() }))
            }
            "approvals.list" => {
                let p: RoomParam = param(&params)?;
                let rt = self.find(&p.room).await?;
                let c = rt.ctx.lock().await;
                ok(c.core.approvals.values().cloned().collect::<Vec<_>>())
            }
            "approve" | "deny" => {
                #[derive(Deserialize)]
                struct P {
                    room: String,
                    id: String,
                    #[serde(default)]
                    task: bool,
                    note: Option<String>,
                }
                let p: P = param(&params)?;
                let rt = self.find(&p.room).await?;
                let mut c = rt.ctx.lock().await;
                let id = c
                    .core
                    .approvals
                    .keys()
                    .find(|k| k.to_hex().starts_with(&p.id.to_lowercase()))
                    .copied()
                    .ok_or_else(|| ApiError::NotFound("no such approval".into()))?;
                let decision = if method == "approve" {
                    Decision::Approve { scope: if p.task { ApprovalScope::Task } else { ApprovalScope::Once } }
                } else {
                    Decision::Deny { note: p.note }
                };
                let what =
                    c.core.approvals.get(&id).map(|a| (a.agent.label.clone(), crate::export::approval_title(&a.kind)));
                let approved = matches!(decision, Decision::Approve { .. });
                let fx = c.core.answer_approval(id, decision, now)?;
                apply(&rt, &mut c, &self.shared, fx).await;
                if let Some((agent, title)) = what {
                    crate::feed::log_locked(
                        &rt,
                        &mut c,
                        &self.shared,
                        "approval",
                        // The person who answered is the actor; the agent is named in the text.
                        None,
                        json!({ "id": id.to_hex(), "approved": approved, "title": title, "agent": agent }),
                    )
                    .await;
                }
                ok(json!({}))
            }
            "status" => ok(self.status().await),
            "lan.discover" => {
                #[derive(Deserialize)]
                struct P {
                    timeout_ms: Option<u64>,
                }
                let p: P = param(&params)?;
                let found = crate::lan::discover(Duration::from_millis(p.timeout_ms.unwrap_or(2000))).await?;
                ok(found)
            }
            "agent.start" if params.get("on").and_then(Value::as_str).is_some_and(|s| !s.is_empty()) => {
                let p: crate::hosts::StartOn = param(&params)?;
                let rt = self.find(&p.room).await?;
                crate::hosts::start_on(self, &rt, p).await
            }
            "agent.hosts" => {
                let p: RoomParam = param(&params)?;
                let rt = self.find(&p.room).await?;
                ok(crate::hosts::list(&rt, now).await)
            }
            "host.invite" => {
                let p: RoomParam = param(&params)?;
                let rt = self.find(&p.room).await?;
                crate::hosts::invite(self, &rt).await
            }
            "host.join" => crate::hosts::join(self, params).await,
            "plugin.list" => ok(crate::plugins::list(&self.shared)),
            "plugin.add" => {
                #[derive(Deserialize)]
                struct P {
                    path: std::path::PathBuf,
                    #[serde(default)]
                    replace: bool,
                }
                let p: P = param(&params)?;
                let i = crate::plugins::store(&self.shared).add(&p.path, p.replace).map_err(ext_err)?;
                ok(
                    json!({ "name": i.manifest.name, "version": i.manifest.version, "hosts": i.manifest.permissions.hosts }),
                )
            }
            "plugin.inspect" => {
                #[derive(Deserialize)]
                struct P {
                    path: std::path::PathBuf,
                }
                let p: P = param(&params)?;
                let m = crate::plugins::inspect(&p.path).map_err(ext_err)?;
                ok(serde_json::to_value(m).unwrap_or_default())
            }
            "plugin.remove" => {
                #[derive(Deserialize)]
                struct P {
                    name: String,
                }
                let p: P = param(&params)?;
                crate::plugins::store(&self.shared).remove(&p.name).map_err(ext_err)?;
                ok(json!({}))
            }
            "skills.list" | "skills.show" | "skills.add" => {
                #[derive(Deserialize)]
                struct P {
                    cwd: Option<std::path::PathBuf>,
                    name: Option<String>,
                    from: Option<std::path::PathBuf>,
                }
                let p: P = param(&params)?;
                let repo = p.cwd.as_deref().map(polyph_git_root);
                let personal = self.shared.paths.home.join("skills");
                match method {
                    "skills.list" => ok(polyph_plugins::skills::discover(repo.as_deref(), &personal)),
                    "skills.show" => {
                        let name = p.name.ok_or_else(|| ApiError::BadRequest("name is required".into()))?;
                        let (s, body) =
                            polyph_plugins::skills::read(repo.as_deref(), &personal, &name).map_err(ext_err)?;
                        ok(json!({ "skill": s, "text": body }))
                    }
                    _ => {
                        let from = p.from.ok_or_else(|| ApiError::BadRequest("from is required".into()))?;
                        let repo = repo.ok_or_else(|| ApiError::BadRequest("cwd is required".into()))?;
                        ok(polyph_plugins::skills::add_to_project(&from, &repo).map_err(ext_err)?)
                    }
                }
            }
            "template.list" => ok(polyph_plugins::templates::list(&self.shared.paths.home.join("templates"))),
            "template.apply" => {
                #[derive(Deserialize)]
                struct P {
                    name: String,
                    dir: std::path::PathBuf,
                }
                let p: P = param(&params)?;
                let repo = polyph_git_root(&p.dir);
                ok(polyph_plugins::templates::apply(&self.shared.paths.home.join("templates"), &p.name, &repo)
                    .map_err(ext_err)?)
            }
            "agent.start" => {
                let p: crate::agents::StartParams = param(&params)?;
                let rt = self.find(&p.room).await?;
                crate::agents::start(self.shared.clone(), &self.agents, rt, p).await
            }
            "agent.prompt" => {
                #[derive(Deserialize)]
                struct P {
                    agent: String,
                    text: String,
                }
                let p: P = param(&params)?;
                if let Some((rt, host, agent)) = self.remote_agent(&p.agent).await {
                    // Runs on one of my agent hosts: the turn shows in the transcript.
                    crate::hosts::prompt_on(&rt, host, agent, &p.text).await;
                    return ok(json!({ "forwarded": true }));
                }
                let h = self.agent(&p.agent).await?;
                let rt = self
                    .rooms
                    .lock()
                    .await
                    .get(&h.room)
                    .cloned()
                    .ok_or_else(|| ApiError::NotFound("room closed".into()))?;
                crate::agents::prompt(&self.shared, &rt, &h, &p.text).await
            }
            "agent.stop" => {
                #[derive(Deserialize)]
                struct P {
                    agent: String,
                }
                let p: P = param(&params)?;
                if let Some((rt, host, agent)) = self.remote_agent(&p.agent).await {
                    crate::hosts::stop_on(&rt, host, agent).await;
                    return ok(json!({ "forwarded": true }));
                }
                let h = self.agent(&p.agent).await?;
                let rt = self
                    .rooms
                    .lock()
                    .await
                    .get(&h.room)
                    .cloned()
                    .ok_or_else(|| ApiError::NotFound("room closed".into()))?;
                crate::agents::stop(&self.shared, &self.agents, &rt, h.agent.id).await
            }
            "agent.list" => {
                let p: RoomParam = param(&params)?;
                let rt = self.find(&p.room).await?;
                let local = self.agents.map.lock().await.clone();
                let c = rt.ctx.lock().await;
                let mut out = vec![];
                for (id, log) in &c.transcripts {
                    let Some(first) = log.front() else { continue };
                    let state = log.iter().rev().find_map(|e| match &e.kind {
                        polyph_proto::messages::AgentEventKind::State { state, .. } => Some(*state),
                        polyph_proto::messages::AgentEventKind::Stopped { .. } => {
                            Some(polyph_proto::messages::AgentState::Stopped)
                        }
                        _ => None,
                    });
                    let host = c.core.is_agent_host(&first.agent.device);
                    out.push(json!({ "id": id.to_hex(), "label": first.agent.label, "kind": first.agent.kind, "member": first.agent.member.to_hex(), "mine": local.contains_key(id), "state": state, "host": host }));
                }
                ok(out)
            }
            "agent.transcript" => {
                #[derive(Deserialize)]
                struct P {
                    room: String,
                    agent: String,
                    after: Option<u64>,
                }
                let p: P = param(&params)?;
                let rt = self.find(&p.room).await?;
                let c = rt.ctx.lock().await;
                let id = c
                    .transcripts
                    .keys()
                    .find(|k| k.to_hex().starts_with(&p.agent.to_lowercase()))
                    .copied()
                    .ok_or_else(|| ApiError::NotFound("no such agent".into()))?;
                let events: Vec<_> =
                    c.transcripts[&id].iter().filter(|e| p.after.is_none_or(|a| e.seq > a)).cloned().collect();
                let calls: Vec<polyph_proto::messages::ToolCall> = {
                    let mut by_id: Vec<polyph_proto::messages::ToolCall> = vec![];
                    for e in &events {
                        if let polyph_proto::messages::AgentEventKind::Tool(t) = &e.kind {
                            match by_id.iter_mut().find(|x| x.id == t.id) {
                                Some(x) => *x = (**t).clone(),
                                None => by_id.push((**t).clone()),
                            }
                        }
                    }
                    by_id
                };
                ok(json!({ "events": events, "summary": polyph_agents::summary::summarize(&calls) }))
            }
            "agents.doctor" => {
                #[derive(Deserialize)]
                struct P {
                    cwd: Option<PathBuf>,
                }
                let p: P = param(&params)?;
                crate::agents::doctor(&self.shared, &p.cwd.unwrap_or_else(|| PathBuf::from(".")))
            }
            "sandbox.test" => {
                let r = polyph_sandbox::selftest::run_all(self.shared.opts.sandbox_helper.clone()).await?;
                ok(r)
            }
            "usage.get" => {
                #[derive(Deserialize)]
                struct P {
                    room: String,
                    cwd: Option<PathBuf>,
                }
                let p: P = param(&params)?;
                let rt = self.find(&p.room).await?;
                // The same trusted load agents enforce budgets with, so the
                // costs shown are the costs counted.
                let loaded = p
                    .cwd
                    .as_deref()
                    .map(|d| crate::agents::main_checkout_project(&self.shared, d))
                    .transpose()?
                    .unwrap_or_default();
                let (l, budgets) = crate::agents::ledger(&rt, &loaded.config.prices).await;
                let states: Vec<Value> =
                    budgets.iter().map(|b| json!({ "budget": b, "used": b.used(&l), "state": b.state(&l) })).collect();
                ok(json!({ "ledger": l, "budgets": states, "untrusted": crate::agents::untrusted_json(&loaded) }))
            }
            "budget.set" => {
                #[derive(Deserialize)]
                struct P {
                    room: String,
                    budgets: Vec<polyph_agents::usage::Budget>,
                }
                let p: P = param(&params)?;
                let rt = self.find(&p.room).await?;
                let mut c = rt.ctx.lock().await;
                if !c.core.role().can_approve_joins() {
                    return Err(ApiError::Forbidden("only Owners and Maintainers set budgets".into()));
                }
                let v = serde_json::to_value(&p.budgets).map_err(|e| ApiError::BadRequest(e.to_string()))?;
                let fx = c.core.change_doc(DocName::Settings, |d| polyph_core::docs::set_setting(d, "budgets", &v))?;
                apply(&rt, &mut c, &self.shared, fx).await;
                ok(json!({}))
            }
            "mcp.connect" => {
                let p: crate::external::ConnectParams = param(&params)?;
                crate::external::connect(self, p).await
            }
            "mcp.call" => {
                #[derive(Deserialize)]
                struct P {
                    session: String,
                    tool: String,
                    #[serde(default)]
                    args: Value,
                }
                let p: P = param(&params)?;
                if polyph_mcp::tools::find(&p.tool).is_none() {
                    return Err(ApiError::BadRequest(format!("unknown tool {}", p.tool)));
                }
                ok(crate::external::call(self, &p.session, &p.tool, &p.args).await?)
            }
            "mcp.disconnect" => {
                #[derive(Deserialize)]
                struct P {
                    session: String,
                }
                let p: P = param(&params)?;
                crate::external::disconnect(self, &p.session, "disconnected").await
            }
            "mcp.sessions" => ok(crate::external::list(self).await),
            "setup.status" | "clients.plan" | "clients.connect" => crate::setup::call(self, method, params).await,
            "claims.check" => {
                let p: crate::external::CheckParams = param(&params)?;
                crate::external::check(self, p).await
            }
            "repo.get" | "repo.set" | "sync.run" => crate::sync::call(self, method, params).await,
            "brief.get" => {
                let p: RoomParam = param(&params)?;
                let rt = self.find(&p.room).await?;
                let mut c = rt.ctx.lock().await;
                match crate::context::brief(&mut c) {
                    Some(b) => ok(b),
                    None => ok(Value::Null),
                }
            }
            "brief.set" => {
                #[derive(Deserialize)]
                struct P {
                    room: String,
                    text: String,
                }
                let p: P = param(&params)?;
                let rt = self.find(&p.room).await?;
                ok(crate::context::set_brief(&rt, &self.shared, &p.text).await?)
            }
            "search" => {
                #[derive(Deserialize)]
                struct P {
                    room: String,
                    query: String,
                    limit: Option<usize>,
                }
                let p: P = param(&params)?;
                let rt = self.find(&p.room).await?;
                let mut c = rt.ctx.lock().await;
                ok(crate::context::search(&mut c, &p.query, p.limit.unwrap_or(20)))
            }
            "recent" => {
                #[derive(Deserialize)]
                struct P {
                    room: String,
                    limit: Option<usize>,
                }
                let p: P = param(&params)?;
                let rt = self.find(&p.room).await?;
                let mut c = rt.ctx.lock().await;
                ok(crate::context::recent(&mut c, p.limit.unwrap_or(20)))
            }
            "areas.get" => {
                #[derive(Deserialize)]
                struct P {
                    dir: PathBuf,
                    path: Option<String>,
                }
                let p: P = param(&params)?;
                let root = crate::folders::root(&p.dir);
                let areas = crate::areas::load(&root);
                match p.path {
                    Some(path) => {
                        let rel = std::path::Path::new(&path)
                            .strip_prefix(&root)
                            .map(|r| r.to_string_lossy().replace('\\', "/"))
                            .unwrap_or(path.replace('\\', "/"));
                        ok(json!({ "path": rel, "area": areas.owner_of(&rel), "source": areas.source }))
                    }
                    None => ok(areas),
                }
            }
            "connect.key" | "connect.revoke" => {
                #[derive(Deserialize)]
                struct P {
                    client: String,
                    dir: Option<PathBuf>,
                }
                let p: P = param(&params)?;
                let home = &self.shared.paths.home;
                if method == "connect.key" {
                    let k = crate::connect_keys::issue(home, &p.client, p.dir.as_deref(), now)?;
                    ok(json!({ "key": k.key, "client": k.client, "dir": k.dir }))
                } else {
                    ok(json!({ "revoked": crate::connect_keys::revoke(home, &p.client, p.dir.as_deref())? }))
                }
            }
            "folder.link" => {
                #[derive(Deserialize)]
                struct P {
                    room: String,
                    dir: PathBuf,
                }
                let p: P = param(&params)?;
                let rt = self.find(&p.room).await?;
                let path = crate::folders::link(&self.shared.paths.home, &rt.id.to_hex(), &p.dir)?;
                ok(json!({ "room": rt.id.to_hex(), "path": path }))
            }
            "folder.unlink" => {
                #[derive(Deserialize)]
                struct P {
                    room: Option<String>,
                    dir: Option<PathBuf>,
                }
                let p: P = param(&params)?;
                let room = match &p.room {
                    Some(r) => Some(self.find(r).await?.id.to_hex()),
                    None => None,
                };
                let n = crate::folders::unlink(&self.shared.paths.home, room.as_deref(), p.dir.as_deref())?;
                ok(json!({ "removed": n }))
            }
            "folder.list" => ok(crate::folders::load(&self.shared.paths.home)),
            "folder.find" => {
                #[derive(Deserialize)]
                struct P {
                    dir: PathBuf,
                }
                let p: P = param(&params)?;
                match crate::folders::room_for(&self.shared.paths.home, &p.dir) {
                    Some(l) => {
                        let name = match self.find(&l.room).await {
                            Ok(rt) => Some(rt.ctx.lock().await.core.name.clone()),
                            Err(_) => None,
                        };
                        ok(json!({ "room": l.room, "path": l.path, "name": name }))
                    }
                    None => ok(Value::Null),
                }
            }
            "room.snapshot" => {
                let p: RoomParam = param(&params)?;
                let rt = self.find(&p.room).await?;
                let mut c = rt.ctx.lock().await;
                upload_snapshot(&mut c, now).await;
                ok(json!({}))
            }
            other => Err(ApiError::NotFound(format!("unknown method {other}"))),
        }
    }

    /// Finds a running agent on this device by id prefix or label.
    /// An agent of mine running on one of my agent hosts (not on this device).
    async fn remote_agent(&self, q: &str) -> Option<(Arc<RoomRuntime>, polyph_proto::ids::DeviceId, ObjectId)> {
        if self.agent(q).await.is_ok() {
            return None;
        }
        let rooms: Vec<Arc<RoomRuntime>> = self.rooms.lock().await.values().cloned().collect();
        for rt in rooms {
            if let Some((host, id)) = crate::hosts::remote_agent(&rt, q).await {
                return Some((rt, host, id));
            }
        }
        None
    }

    pub async fn agent(&self, q: &str) -> Result<Arc<crate::agents::AgentHandle>, ApiError> {
        let ql = q.to_lowercase();
        let map = self.agents.map.lock().await;
        let hits: Vec<_> = map
            .values()
            .filter(|h| h.agent.id.to_hex().starts_with(&ql) || h.agent.label.to_lowercase() == ql)
            .cloned()
            .collect();
        match hits.len() {
            1 => Ok(hits[0].clone()),
            0 => Err(ApiError::NotFound(format!("no agent matches \"{q}\" on this device; see `polyph agent list`"))),
            _ => Err(ApiError::BadRequest(format!("\"{q}\" matches several agents; use more of the id"))),
        }
    }

    async fn close_room(&self, rt: &Arc<RoomRuntime>) {
        {
            let mut c = rt.ctx.lock().await;
            let now = self.shared.clock.now();
            if c.connected {
                upload_snapshot(&mut c, now).await;
                let fx = vec![polyph_core::Effect::Send(polyph_proto::messages::RoomMessage::Goodbye)];
                apply(rt, &mut c, &self.shared, fx).await;
            }
            let _ = self.shared.save_room(&mut c.core);
            c.lan = None;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
        rt.stop();
        self.open.lock().await.retain(|r| *r != rt.id);
        self.save_open_list().await;
    }

    pub async fn status(&self) -> StatusSnapshot {
        let current = self.open.lock().await.last().copied();
        let rt = match current {
            Some(id) => self.rooms.lock().await.get(&id).cloned(),
            None => None,
        };
        let Some(rt) = rt else {
            return StatusSnapshot {
                room: None,
                room_name: None,
                connected: false,
                members_online: 0,
                agents: vec![],
                approvals: vec![],
                budgets: vec![],
            };
        };
        let info = self.info(&rt).await;
        let c = rt.ctx.lock().await;
        let approvals = c
            .core
            .approvals
            .values()
            .map(|a| StatusApproval {
                id: a.id.to_hex(),
                agent: a.agent.label.clone(),
                title: crate::export::approval_title(&a.kind),
                mine: a.agent.member == c.core.my_member,
                created_at: a.created_at,
            })
            .collect();
        drop(c);
        let mut s = StatusSnapshot {
            room: Some(info.id),
            room_name: Some(info.name),
            connected: info.connected,
            members_online: info.members_online as u32,
            agents: vec![],
            approvals,
            budgets: vec![],
        };
        crate::status::fill(self, &rt, &mut s).await;
        s
    }

    /// Joins a room with a code (the joiner side of docs/protocol.md 7.3).
    async fn join(self: &Arc<Self>, p: JoinParams) -> ApiResult {
        let code =
            JoinCode::parse(&p.code).map_err(|e| ApiError::BadRequest(format!("{e}; codes look like A1B2-CDEF")))?;
        let name = p.name.clone().unwrap_or_else(|| self.shared.display_name());
        if p.name.is_some() && self.shared.config.lock().name.is_none() {
            let mut cfg = self.shared.config.lock();
            cfg.name = p.name.clone();
            let _ = cfg.save(&self.shared.paths);
        }
        let mut candidates: Vec<(RelayAddr, bool)> = vec![];
        if let Some(r) = &p.relay {
            let a = RelayAddr::parse(r);
            let lan = p.lan || (a.url.starts_with("wss://") && a.pin.is_none() && is_private_host(&a.url));
            candidates.push((a, lan));
        } else if p.lan {
            for d in crate::lan::discover(Duration::from_millis(1500)).await.unwrap_or_default() {
                candidates.push((RelayAddr { url: d.url, pin: None }, true));
            }
        } else {
            candidates.push((RelayAddr::parse(&self.shared.config.lock().relay), false));
        }
        if candidates.is_empty() {
            return Err(ApiError::NotFound("no Polyph rooms found on this network".into()));
        }
        let mut last =
            ApiError::NotFound("no room is waiting for this code; check it with the person who sent it".into());
        for (addr, lan) in candidates {
            match self.join_via(&code, &name, &addr, lan, p.timeout_secs.unwrap_or(600), &p.capabilities).await {
                Ok(v) => return Ok(v),
                Err(e) => last = e,
            }
        }
        Err(last)
    }

    /// Makes a join code and waits until the relay hosts it. `own_device` is
    /// set only by `phone.pair` and `host.invite`: a knock through the code may
    /// then name this member, as another device of theirs, and still needs a
    /// person to approve it here.
    pub(crate) async fn invite(
        self: &Arc<Self>,
        rt: &RoomRuntime,
        reusable: bool,
        expires_secs: Option<i64>,
        max_uses: Option<u32>,
        own_device: bool,
    ) -> ApiResult {
        let now = self.shared.clock.now();
        let kind = if reusable { CodeKind::Reusable { max_uses } } else { CodeKind::OneTime };
        let ttl = expires_secs.unwrap_or(if reusable { 7 * 86_400 } else { 15 * 60 });
        // A freshly opened or joined room may still be connecting.
        if self.open.lock().await.contains(&rt.id) {
            for _ in 0..200 {
                if rt.ctx.lock().await.connected {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
        }
        for _ in 0..4 {
            let (code, waiter, lan, relay) = {
                let mut c = rt.ctx.lock().await;
                if c.core.quick && reusable {
                    return Err(ApiError::BadRequest("quick sessions only have one-time codes".into()));
                }
                // A knock naming this member is let in only by this device,
                // so a role that cannot approve joins cannot add a device.
                if own_device && !c.core.role().can_approve_joins() {
                    let lang = self.shared.config.lock().lang.clone();
                    let l = polyph_i18n::Localizer::new(&polyph_i18n::detect(lang.as_deref()));
                    return Err(ApiError::Forbidden(l.t("error-add-device-needs-approver")));
                }
                if !c.core.role().can_write() {
                    return Err(ApiError::Forbidden("viewers cannot invite people".into()));
                }
                let (code, waiter) = host_code(&mut c, kind.clone(), now + ttl, own_device).await?;
                crate::runtime::save_codes(&mut c, &self.shared, now);
                // Browsers reach internet relays only, not a LAN relay with its own certificate.
                let relay = c.relay.pin.is_none().then(|| c.relay.url.clone());
                (code, waiter, c.lan.as_ref().map(|l| l.addr.to_string_with_pin()), relay)
            };
            // Wait (without holding the room lock) until the relay hosts it,
            // so a joiner who types the code immediately finds it.
            let hosted = match waiter {
                Some(rx) => tokio::time::timeout(Duration::from_secs(10), rx).await.ok().and_then(|r| r.ok()),
                None => Some(true),
            };
            match hosted {
                Some(true) => {
                    let pending = !rt.ctx.lock().await.connected;
                    return ok(
                        json!({ "code": code.to_string(), "expires_at": now + ttl, "reusable": reusable, "lan": lan, "pending": pending, "relay": relay }),
                    );
                }
                Some(false) => continue,
                None => {
                    return Err(ApiError::Failed(
                        "the relay did not confirm the code; check the connection and try again".into(),
                    ));
                }
            }
        }
        Err(ApiError::Failed("could not find a free code; try again".into()))
    }

    async fn join_via(
        self: &Arc<Self>,
        code: &JoinCode,
        name: &str,
        addr: &RelayAddr,
        lan: bool,
        timeout_secs: u64,
        caps: &[String],
    ) -> ApiResult {
        let deadline = tokio::time::Instant::now() + Duration::from_secs(timeout_secs);
        // The same salt on every attempt: a reconnect takes back the knock
        // the first attempt made, and the name commitment in it.
        let salt = ObjectId::random().map_err(|e| ApiError::Failed(e.to_string()))?.0;
        // Set once a request reached the room; until the reconnect window
        // closes, a lost connection is retried, not reported.
        let mut dropped_at: Option<tokio::time::Instant> = None;
        let mut backoff = JOIN_RETRY_FIRST;
        loop {
            match self.join_attempt(code, name, addr, lan, caps, &salt).await {
                Ok(mut a) => {
                    backoff = JOIN_RETRY_FIRST;
                    loop {
                        let left = deadline.saturating_duration_since(tokio::time::Instant::now()).as_secs();
                        let Some(data) = recv_pake(&mut a.client, a.session, left).await else { break };
                        let reply: JoinReply =
                            from_cbor(&a.channel.open(&data).map_err(|e| ApiError::Failed(e.to_string()))?)
                                .map_err(|e| ApiError::Failed(e.to_string()))?;
                        match reply {
                            JoinReply::Pending => {
                                self.shared.emit(ApiEvent::global("join_pending", json!({ "code": code.first_half() })))
                            }
                            JoinReply::Denied { reason } => {
                                return Err(ApiError::Forbidden(format!("the room declined the request: {reason}")));
                            }
                            JoinReply::Approved(ap) => {
                                let now = self.shared.clock.now();
                                let mut core = RoomCore::from_join(self.shared.identity.clone(), &ap, name, salt, now)?;
                                core.surface = self.shared.opts.surface;
                                let relay = match ap.relay.as_deref() {
                                    Some(r) => RelayAddr::parse(r),
                                    None => RelayAddr {
                                        url: addr.url.clone(),
                                        pin: if lan { a.recorded } else { addr.pin },
                                    },
                                };
                                core.relay = Some(relay.to_string_with_pin());
                                drop(a);
                                let rt = self.add_and_open(core, relay).await;
                                info!(room = %rt.id.short(), "joined room");
                                return ok(self.info(&rt).await);
                            }
                        }
                    }
                    // Waits are counted in whole seconds.
                    if tokio::time::Instant::now() + Duration::from_secs(1) >= deadline {
                        return Err(ApiError::Failed("the join was not approved in time".into()));
                    }
                    // The relay ended the session: this side's connection or
                    // the host's dropped. The host keeps the knock for a while.
                    info!("join connection lost while waiting for approval; reconnecting");
                    dropped_at = Some(tokio::time::Instant::now());
                }
                Err(Attempt::Retry(e)) if dropped_at.is_some() => {
                    debug!(error = ?e, "join reconnect failed; trying again");
                }
                Err(Attempt::Retry(e) | Attempt::Fail(e)) => return Err(e),
            }
            let now = tokio::time::Instant::now();
            let window = dropped_at.map(|t| t + Duration::from_secs(JOIN_RECONNECT_SECS)).unwrap_or(now);
            let until = window.min(deadline);
            if now >= until {
                return Err(ApiError::Failed(
                    "lost the connection to the room while waiting for approval; join again, or ask for a new code if this one was for one use".into(),
                ));
            }
            tokio::time::sleep(backoff.min(until - now)).await;
            backoff = (backoff * 2).min(JOIN_RETRY_MAX);
        }
    }

    /// One pass through the join handshake: proves the code and sends the
    /// join request. Errors a reconnect may get past are `Attempt::Retry`.
    async fn join_attempt(
        &self,
        code: &JoinCode,
        name: &str,
        addr: &RelayAddr,
        lan: bool,
        caps: &[String],
        salt: &[u8; 16],
    ) -> Result<JoinSession, Attempt> {
        let retry = |e: String| Attempt::Retry(ApiError::Failed(e));
        let seen = Arc::new(std::sync::Mutex::new(None));
        let mode = if lan && addr.pin.is_none() { TlsMode::Record(seen.clone()) } else { addr.tls() };
        let mut client = RelayClient::connect_with(&addr.url, &self.shared.identity, mode)
            .await
            .map_err(|e| retry(e.to_string()))?;
        let recorded = seen.lock().ok().and_then(|g| *g).or(addr.pin);
        let binding = if lan { recorded.map(|f| f.to_vec()).unwrap_or_default() } else { vec![] };
        client.send(ClientFrame::JoinCode { rendezvous: code.rendezvous() }).await.map_err(|e| retry(e.to_string()))?;
        let session = loop {
            match client.recv_timeout(Duration::from_secs(15)).await {
                Some(RelayFrame::JoinStarted { session }) => break session,
                Some(RelayFrame::Error { code: c, detail }) => {
                    return Err(match c {
                        // The host may be reconnecting and hosting it again.
                        ErrorCode::CodeUnknown => Attempt::Retry(ApiError::NotFound(
                            "no room is waiting for this code; it may have expired or been used, or the person who made it is offline".into(),
                        )),
                        ErrorCode::CodeLocked => Attempt::Fail(ApiError::Forbidden(
                            "this code is locked for 10 minutes after failed attempts".into(),
                        )),
                        ErrorCode::RateLimited => Attempt::Fail(ApiError::Failed(
                            "too many join attempts from this network; wait a few minutes".into(),
                        )),
                        _ => Attempt::Fail(ApiError::Failed(detail)),
                    });
                }
                Some(_) => continue,
                None => return Err(retry("the relay did not answer".into())),
            }
        };
        let pake = PakeStart::new(code, PakeRole::Joiner, &binding);
        client
            .send(ClientFrame::Pake { session, data: pake.message().to_vec() })
            .await
            .map_err(|e| retry(e.to_string()))?;
        let wrong = || ApiError::Forbidden("the code did not match; check it and try again".into());
        // A session that ends here is a wrong code on a first attempt, or a
        // host that dropped again during a reconnect: `join_via` tells them apart.
        let ended = || Attempt::Retry(wrong());
        let host_msg = recv_pake(&mut client, session, 30).await.ok_or_else(ended)?;
        let fin = pake.finish(&host_msg).map_err(|_| Attempt::Fail(wrong()))?;
        client
            .send(ClientFrame::Pake { session, data: fin.confirmation().to_vec() })
            .await
            .map_err(|e| retry(e.to_string()))?;
        let host_tag = recv_pake(&mut client, session, 30).await.ok_or_else(ended)?;
        let mut channel = fin.verify_peer(&host_tag).map_err(|_| Attempt::Fail(wrong()))?;
        let req = JoinRequest {
            device: self.shared.identity.public(),
            member: self.shared.member_id().map_err(|e| Attempt::Fail(ApiError::Failed(e.to_string())))?,
            display_name: name.to_string(),
            name_salt: salt.to_vec(),
            surface: format!("{:?}", self.shared.opts.surface).to_lowercase(),
            link_device: false,
            capabilities: caps.to_vec(),
        };
        let body = to_cbor(&req)
            .map_err(|e| e.to_string())
            .and_then(|b| channel.seal(&b).map_err(|e| e.to_string()))
            .map_err(|e| Attempt::Fail(ApiError::Failed(e)))?;
        client.send(ClientFrame::Pake { session, data: body }).await.map_err(|e| retry(e.to_string()))?;
        Ok(JoinSession { client, channel, session, recorded })
    }

    /// Stops all rooms, saving state and uploading snapshots.
    pub async fn stop(&self) {
        let rooms: Vec<Arc<RoomRuntime>> = self.rooms.lock().await.values().cloned().collect();
        let sessions = self.external.list().await;
        for rt in rooms {
            for s in sessions.iter().filter(|s| s.room == rt.id) {
                let reason = polyph_proto::messages::AgentEventKind::Stopped { reason: "daemon stopped".into() };
                crate::agents::publish(&rt, &self.shared, &s.caller.agent, reason).await;
            }
            let mut c = rt.ctx.lock().await;
            crate::coord::persist_inboxes(&mut c, &self.shared);
            let now = self.shared.clock.now();
            if c.connected {
                upload_snapshot(&mut c, now).await;
                let fx = vec![polyph_core::Effect::Send(polyph_proto::messages::RoomMessage::Goodbye)];
                apply(&rt, &mut c, &self.shared, fx).await;
            }
            let _ = self.shared.save_room(&mut c.core);
            crate::runtime::save_codes(&mut c, &self.shared, now);
            drop(c);
            rt.stop();
        }
    }
}

#[derive(Debug, Deserialize)]
struct JoinParams {
    code: String,
    name: Option<String>,
    relay: Option<String>,
    #[serde(default)]
    lan: bool,
    timeout_secs: Option<u64>,
    /// Device capabilities to ask for (`agent-host` for an agent server).
    #[serde(default)]
    capabilities: Vec<String>,
}

/// How long a joiner keeps reconnecting after its connection dropped while
/// waiting for approval. Shorter than the host keeps the knock
/// (`runtime::PARK_SECS`).
pub const JOIN_RECONNECT_SECS: u64 = 180;
/// Waits between reconnect attempts, doubling. Every attempt counts against
/// the relay's per-address join limit (20 per 10 minutes), so a window spends
/// about 9 of them.
const JOIN_RETRY_FIRST: Duration = Duration::from_secs(2);
const JOIN_RETRY_MAX: Duration = Duration::from_secs(30);

/// A join request that reached the room, waiting for the answer.
struct JoinSession {
    client: RelayClient,
    channel: SecureChannel,
    session: u64,
    recorded: Option<[u8; 32]>,
}

/// Why a join attempt stopped: worth reconnecting for, or final.
#[derive(Debug)]
enum Attempt {
    Retry(ApiError),
    Fail(ApiError),
}

async fn recv_pake(client: &mut RelayClient, session: u64, secs: u64) -> Option<Vec<u8>> {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(secs);
    loop {
        let left = deadline.saturating_duration_since(tokio::time::Instant::now());
        match client.recv_timeout(left).await? {
            RelayFrame::Pake { session: s, data } if s == session => return Some(data),
            RelayFrame::PakeEnded { session: s, .. } if s == session => return None,
            _ => continue,
        }
    }
}

/// The repository root holding `dir`, or `dir` itself outside a repository.
pub(crate) fn polyph_git_root(dir: &std::path::Path) -> std::path::PathBuf {
    let mut d = dir.to_path_buf();
    loop {
        if d.join(".git").exists() {
            return d;
        }
        if !d.pop() {
            return dir.to_path_buf();
        }
    }
}

fn ext_err(e: polyph_plugins::Error) -> ApiError {
    match e {
        polyph_plugins::Error::NotFound(s) => ApiError::NotFound(s),
        polyph_plugins::Error::Invalid(s) | polyph_plugins::Error::Exists(s) => ApiError::BadRequest(s),
        polyph_plugins::Error::Io(e) => ApiError::Failed(e.to_string()),
    }
}

fn is_private_host(url: &str) -> bool {
    let host = url.trim_start_matches("wss://").split([':', '/']).next().unwrap_or("");
    match host.parse::<IpAddr>() {
        Ok(IpAddr::V4(v4)) => v4.is_private() || v4.is_loopback() || v4.is_link_local(),
        Ok(IpAddr::V6(v6)) => v6.is_loopback() || v6.is_unique_local(),
        Err(_) => host.ends_with(".local"),
    }
}

fn resolve_member(core: &RoomCore, q: &str) -> Result<MemberId, ApiError> {
    let ql = q.to_lowercase();
    let mut hits: Vec<MemberId> = core
        .peers
        .values()
        .filter_map(|p| p.member().map(|m| (m, p.name())))
        .filter(|(m, n)| n.to_lowercase() == ql || m.to_hex().starts_with(&ql))
        .map(|(m, _)| m)
        .collect();
    hits.sort();
    hits.dedup();
    match hits.len() {
        1 => Ok(hits[0]),
        0 => Err(ApiError::NotFound(format!("no member matches \"{q}\"; see `polyph room members`"))),
        _ => Err(ApiError::BadRequest(format!("\"{q}\" matches several members; use their member id"))),
    }
}

pub fn member_names(core: &RoomCore) -> HashMap<String, String> {
    let mut m: HashMap<String, String> =
        core.peers.values().filter_map(|p| p.member().map(|id| (id.to_hex(), p.name()))).collect();
    m.insert(core.my_member.to_hex(), core.my_profile.display_name.clone());
    m
}
