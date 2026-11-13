//! One open room: its engine, its relay connection and the host side of
//! joins that arrive on that connection.

use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use std::time::Duration;

use polyph_core::{Effect, RoomCore, RoomEvent};
use polyph_proto::cert::Role;
use polyph_proto::codec::{from_cbor, to_cbor};
use polyph_proto::ids::{DeviceId, MemberId, ObjectId, RendezvousId, RoomId};
use polyph_proto::join::{JoinReply, JoinRequest};
use polyph_proto::joincode::JoinCode;
use polyph_proto::messages::{Claim, Knock, RoomMessage};
use polyph_proto::pake::{PakeFinished, PakeRole, PakeStart, SecureChannel};
use polyph_proto::relay_wire::{ClientFrame, CodeClosed, CodeKind, RelayFrame};
use polyph_relay::client::{RelayClient, RelaySender, TlsMode};
use tokio::sync::{Mutex, Notify, oneshot};
use tokio::task::JoinHandle;
use tracing::{debug, info, warn};

use crate::daemon::Shared;
use crate::events::ApiEvent;

/// Messages queued while disconnected, oldest dropped first beyond this.
const OUTBOX_MAX: usize = 2000;
/// Failed join attempts on one code before the daemon stops accepting it,
/// mirroring the relay's own limit.
const HOST_MAX_FAILURES: u32 = 5;
const SNAPSHOT_EVERY: i64 = 300;
/// Longest note a denied joiner is sent with `polyph deny --note`.
const MAX_DENY_NOTE_CHARS: usize = 300;
/// How long a knock waits for its joiner to come back after the connection
/// dropped mid-wait. The joiner retries for less than this (see
/// `daemon::JOIN_RECONNECT_SECS`).
pub const PARK_SECS: i64 = 300;
/// Transcript events kept in memory per agent.
pub const TRANSCRIPT_KEEP: usize = 5000;

/// A relay URL plus an optional pinned certificate fingerprint, written as
/// `wss://host:port/ws#sha256=<hex>` in saved state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RelayAddr {
    pub url: String,
    pub pin: Option<[u8; 32]>,
}

impl RelayAddr {
    pub fn parse(s: &str) -> Self {
        match s.split_once("#sha256=") {
            Some((url, fp)) => {
                let pin = hex::decode(fp).ok().and_then(|b| <[u8; 32]>::try_from(b.as_slice()).ok());
                Self { url: url.to_string(), pin }
            }
            None => Self { url: s.to_string(), pin: None },
        }
    }

    pub fn to_string_with_pin(&self) -> String {
        match self.pin {
            Some(p) => format!("{}#sha256={}", self.url, hex::encode(p)),
            None => self.url.clone(),
        }
    }

    pub fn tls(&self) -> TlsMode {
        match self.pin {
            Some(p) => TlsMode::Pinned(p),
            None => TlsMode::Public,
        }
    }
}

#[derive(Debug, Clone)]
pub struct HostedCode {
    pub code: JoinCode,
    pub kind: CodeKind,
    pub expires_at: i64,
    pub failures: u32,
    /// Joins whose code verified on this host.
    pub uses: u32,
    /// Made by `phone.pair` or `host.invite` for another device of this member:
    /// a knock through it may claim this member id. Kept in memory only.
    pub own_device: Option<MemberId>,
    /// A used one-time code hosted again for the one device whose join lost
    /// its connection while waiting. Nobody else may knock through it.
    pub reserved_for: Option<DeviceId>,
}

enum HostStage {
    AwaitPake(PakeStart),
    AwaitConfirm(PakeFinished),
    AwaitRequest(SecureChannel),
    Knocked { channel: SecureChannel, knock: ObjectId },
}

struct HostJoin {
    rendezvous: RendezvousId,
    stage: Option<HostStage>,
    /// The code's own-device member when the joiner arrived.
    own_device: Option<MemberId>,
    /// The device the code was held for when the joiner arrived.
    reserved_for: Option<DeviceId>,
    /// The code this join used up, kept so it can be hosted again for this
    /// joiner if the connection drops before the knock is answered.
    retired: Option<HostedCode>,
}

/// A knock whose joiner lost the connection while waiting for an answer. The
/// joiner reconnects, proves the code again and, with exactly the same
/// request (member id, device keys and name salt), takes the knock back. An
/// answer given meanwhile waits here.
struct Parked {
    knock: ObjectId,
    member: MemberId,
    kx: Vec<u8>,
    name_salt: Vec<u8>,
    rendezvous: RendezvousId,
    since: i64,
    reply: Option<JoinReply>,
}

/// Resolves with (granted, denied) when the acting host answers a claim request.
pub type ClaimWaiter = oneshot::Sender<(Vec<Claim>, Vec<Claim>)>;

pub struct RoomCtx {
    pub core: RoomCore,
    pub sender: Option<RelaySender>,
    pub connected: bool,
    /// The last failed relay connect, cleared once connected.
    pub relay_error: Option<RelayErrorInfo>,
    pub offline: bool,
    pub dirty: bool,
    pub relay: RelayAddr,
    /// Fingerprint bound into join PAKEs on this connection (LAN relays).
    pub binding: Vec<u8>,
    pub codes: HashMap<RendezvousId, HostedCode>,
    joins: HashMap<u64, HostJoin>,
    /// Knocks waiting for their joiner to reconnect, by joiner device.
    parked: HashMap<DeviceId, Parked>,
    /// The codes as last written to disk, to write again only on change.
    saved_codes: Option<Vec<SavedCode>>,
    outbox: VecDeque<Vec<u8>>,
    /// Claim requests an agent is waiting on, by request id: the agent and
    /// where to send the answer.
    pub claim_waiters: HashMap<ObjectId, (ObjectId, ClaimWaiter)>,
    /// Agent tasks waiting for a human to answer an approval.
    pub approval_waiters: HashMap<ObjectId, oneshot::Sender<polyph_proto::messages::Decision>>,
    /// Recent transcript events per agent (ours and teammates'), for follow mode.
    pub transcripts: HashMap<ObjectId, VecDeque<polyph_proto::messages::AgentEvent>>,
    pub agent_seq: u64,
    /// Invites waiting for the relay to confirm (or refuse) a hosted code.
    pub code_waiters: VecDeque<(RendezvousId, oneshot::Sender<bool>)>,
    pub last_snapshot: i64,
    pub needs_snapshot: bool,
    pub connected_at: Option<tokio::time::Instant>,
    pub lan: Option<crate::lan::LanRoom>,
    /// Inboxes, reply waiters and the loop guard for this device's agents.
    pub coord: crate::coord::Coord,
    /// Hosted mode: this device's side of the shared working copy.
    pub mirror: Option<crate::mirror::Mirror>,
    /// Shared terminals: all known, and the ones running here.
    pub terms: crate::terms::Terminals,
}

/// Why the last relay connect attempt failed, for logs, status and doctor.
#[derive(Debug, Clone, serde::Serialize)]
pub struct RelayErrorInfo {
    /// One of certificate, dns, refused, timeout, http-STATUS, rejected, other.
    pub kind: String,
    pub message: String,
}

impl std::fmt::Debug for RoomCtx {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "RoomCtx({:?}, connected {})", self.core, self.connected)
    }
}

pub struct RoomRuntime {
    pub id: RoomId,
    pub ctx: Mutex<RoomCtx>,
    pub kick: Notify,
    tasks: parking_lot::Mutex<Vec<JoinHandle<()>>>,
}

impl std::fmt::Debug for RoomRuntime {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "RoomRuntime({})", self.id.short())
    }
}

impl RoomRuntime {
    pub fn new(core: RoomCore, relay: RelayAddr) -> Arc<Self> {
        let id = core.id;
        Arc::new(Self {
            id,
            ctx: Mutex::new(RoomCtx {
                core,
                sender: None,
                connected: false,
                relay_error: None,
                offline: false,
                dirty: true,
                relay,
                binding: vec![],
                codes: HashMap::new(),
                joins: HashMap::new(),
                parked: HashMap::new(),
                saved_codes: None,
                outbox: VecDeque::new(),
                claim_waiters: HashMap::new(),
                coord: Default::default(),
                mirror: None,
                terms: Default::default(),
                code_waiters: VecDeque::new(),
                approval_waiters: HashMap::new(),
                transcripts: HashMap::new(),
                agent_seq: 0,
                last_snapshot: 0,
                needs_snapshot: true,
                connected_at: None,
                lan: None,
            }),
            kick: Notify::new(),
            tasks: parking_lot::Mutex::new(vec![]),
        })
    }

    pub fn start(self: &Arc<Self>, shared: Arc<Shared>) {
        let a = tokio::spawn(connection_loop(self.clone(), shared.clone()));
        let b = tokio::spawn(tick_loop(self.clone(), shared));
        self.tasks.lock().extend([a, b]);
    }

    pub fn stop(&self) {
        for t in self.tasks.lock().drain(..) {
            t.abort();
        }
    }
}

impl Drop for RoomRuntime {
    fn drop(&mut self) {
        self.stop();
    }
}

/// Seals and publishes `Send` effects, handles events that need daemon
/// action, and forwards everything to local clients.
pub async fn apply(rt: &RoomRuntime, ctx: &mut RoomCtx, shared: &Shared, effects: Vec<Effect>) {
    for e in effects {
        match e {
            Effect::Send(m) => send_msg(ctx, &m).await,
            Effect::Event(RoomEvent::Terminal { from, member, msg }) => {
                // Terminal bytes reach local clients as their own events.
                crate::terms::handle(ctx, shared, from, member, *msg);
            }
            Effect::Event(RoomEvent::Mirror { from, msg }) => {
                // File contents stay out of the local event stream.
                for m in crate::mirror::handle(ctx, shared, from, *msg) {
                    send_msg(ctx, &RoomMessage::Mirror(m)).await;
                }
            }
            Effect::Event(ev) => {
                match &ev {
                    RoomEvent::JoinReady { knock, approved } => {
                        let reply = JoinReply::Approved(approved.clone());
                        finish_knocked_join(ctx, *knock, &reply).await;
                    }
                    RoomEvent::JoinDenied { knock, note } => {
                        // The joiner shows this after "the room declined the request: ".
                        let reason = note
                            .as_deref()
                            .map(str::trim)
                            .filter(|n| !n.is_empty())
                            .map(|n| n.chars().take(MAX_DENY_NOTE_CHARS).collect())
                            .unwrap_or_else(|| "no reason given".to_string());
                        let reply = JoinReply::Denied { reason };
                        finish_knocked_join(ctx, *knock, &reply).await;
                    }
                    RoomEvent::ClaimDecision { request, granted, denied } => {
                        if let Some((agent, tx)) = ctx.claim_waiters.remove(request)
                            && tx.send((granted.clone(), denied.clone())).is_err()
                            && !granted.is_empty()
                            // A retry by the same agent renews these same
                            // claims; leave them to it.
                            && !ctx.claim_waiters.values().any(|(a, _)| *a == agent)
                        {
                            // The asker stopped waiting (it timed out and was
                            // told the claim failed). Give the grant back so
                            // nobody is blocked by a lease no one uses.
                            let ids: Vec<ObjectId> = granted.iter().map(|c| c.id).collect();
                            for fx in ctx.core.release_claims(&ids) {
                                match fx {
                                    Effect::Send(m) => send_msg(ctx, &m).await,
                                    Effect::Event(e) => shared.emit(ApiEvent::room(&rt.id, &e)),
                                }
                            }
                        }
                    }
                    RoomEvent::ApprovalResolved { id, decision, .. } => {
                        if let Some(tx) = ctx.approval_waiters.remove(id) {
                            let _ = tx.send(decision.clone());
                        }
                    }
                    RoomEvent::Inbox { message, for_me: true } => {
                        ctx.coord.deliver(message.clone());
                        // On disk at once, so a restart does not lose it.
                        crate::coord::persist_inboxes(ctx, shared);
                    }
                    RoomEvent::ClaimRevoked { claim, winner } if claim.owner.device == ctx.core.me => {
                        ctx.coord.notice(
                            claim.owner.id,
                            format!(
                                "Your claim on {} was revoked after a network split: {} claimed it first. Check whether your changes to it need reconciling.",
                                claim.target, winner.owner.label
                            ),
                        );
                    }
                    RoomEvent::Agent { event } => {
                        let log = ctx.transcripts.entry(event.agent.id).or_default();
                        log.push_back((**event).clone());
                        while log.len() > TRANSCRIPT_KEEP {
                            log.pop_front();
                        }
                    }
                    RoomEvent::YouWereRemoved | RoomEvent::RoomEnded => {
                        ctx.offline = true;
                    }
                    _ => {}
                }
                let mut api = ApiEvent::room(&rt.id, &ev);
                // Tells the approver this is another device joining as them.
                if let RoomEvent::Knock { knock } = &ev
                    && let Some(o) = api.event.get_mut("knock").and_then(|k| k.as_object_mut())
                {
                    o.insert("own_device".into(), ctx.core.is_own_device_knock(&knock.id).into());
                }
                shared.emit(api);
            }
        }
    }
    ctx.core.commit_rotation();
    ctx.dirty = true;
}

/// Seals and publishes one message (queued while disconnected).
pub async fn send(ctx: &mut RoomCtx, m: &RoomMessage) {
    send_msg(ctx, m).await
}

async fn send_msg(ctx: &mut RoomCtx, m: &RoomMessage) {
    let env = match ctx.core.seal(m) {
        Ok(e) => e.encode(),
        Err(e) => {
            warn!(error = %e, "could not seal message");
            return;
        }
    };
    match (&ctx.sender, ctx.connected) {
        (Some(s), true) => {
            if s.send(ClientFrame::Publish { envelope: env.clone() }).await.is_err() {
                push_outbox(ctx, env);
            }
        }
        _ => push_outbox(ctx, env),
    }
}

fn push_outbox(ctx: &mut RoomCtx, env: Vec<u8>) {
    if ctx.outbox.len() >= OUTBOX_MAX {
        ctx.outbox.pop_front();
    }
    ctx.outbox.push_back(env);
}

async fn finish_knocked_join(ctx: &mut RoomCtx, knock: ObjectId, reply: &JoinReply) {
    let session = ctx
        .joins
        .iter()
        .find(|(_, j)| matches!(&j.stage, Some(HostStage::Knocked { knock: k, .. }) if *k == knock))
        .map(|(s, _)| *s);
    let Some(session) = session else {
        // The joiner is reconnecting: the answer waits for it.
        if let Some(p) = ctx.parked.values_mut().find(|p| p.knock == knock) {
            p.reply = Some(reply.clone());
        }
        return;
    };
    let Some(mut j) = ctx.joins.remove(&session) else { return };
    if let Some(HostStage::Knocked { mut channel, .. }) = j.stage.take() {
        let Ok(body) = to_cbor(reply) else { return };
        if let (Ok(ct), Some(s)) = (channel.seal(&body), &ctx.sender) {
            let _ = s.send(ClientFrame::Pake { session, data: ct }).await;
        }
    }
}

/// Failed relay connects are logged at most this often.
const RELAY_LOG_EVERY: Duration = Duration::from_secs(60);

async fn connection_loop(rt: Arc<RoomRuntime>, shared: Arc<Shared>) {
    let mut backoff = Duration::from_millis(500);
    let mut last_logged: Option<tokio::time::Instant> = None;
    loop {
        let (addr, offline) = {
            let c = rt.ctx.lock().await;
            (c.relay.clone(), c.offline)
        };
        if offline {
            rt.kick.notified().await;
            continue;
        }
        let client = match RelayClient::connect_with(&addr.url, &shared.identity, addr.tls()).await {
            Ok(c) => c,
            Err(e) => {
                let info = RelayErrorInfo { kind: e.failure().code(), message: e.to_string() };
                if last_logged.is_none_or(|t| t.elapsed() >= RELAY_LOG_EVERY) {
                    warn!(room = %rt.id.short(), kind = %info.kind, error = %e, "relay connect failed");
                    last_logged = Some(tokio::time::Instant::now());
                }
                rt.ctx.lock().await.relay_error = Some(info);
                tokio::select! {
                    _ = tokio::time::sleep(backoff) => {}
                    _ = rt.kick.notified() => {}
                }
                backoff = (backoff * 2).min(Duration::from_secs(30));
                continue;
            }
        };
        backoff = Duration::from_millis(500);
        last_logged = None;
        rt.ctx.lock().await.relay_error = None;
        let (sender, mut rx, task) = client.into_parts();
        if let Err(e) = on_connect(&rt, &shared, &sender).await {
            warn!(error = %e, "room setup on connect failed");
        }
        info!(room = %rt.id.short(), "connected to relay");
        shared.emit(ApiEvent::room_kind(&rt.id, "connection", serde_json::json!({ "connected": true })));
        loop {
            tokio::select! {
                f = rx.recv() => match f {
                    Some(f) => handle_frame(&rt, &shared, f).await,
                    None => break,
                },
                _ = rt.kick.notified() => {
                    if rt.ctx.lock().await.offline {
                        break;
                    }
                }
            }
        }
        task.abort();
        {
            let mut c = rt.ctx.lock().await;
            c.sender = None;
            c.connected = false;
            // The relay ended every join on this connection. Knocks keep
            // waiting for their joiners, who reconnect.
            let now = shared.clock.now();
            let joins: Vec<HostJoin> = c.joins.drain().map(|(_, j)| j).collect();
            for j in joins {
                end_join(&mut c, j, now, false).await;
            }
        }
        shared.emit(ApiEvent::room_kind(&rt.id, "connection", serde_json::json!({ "connected": false })));
    }
}

async fn on_connect(rt: &RoomRuntime, shared: &Shared, sender: &RelaySender) -> anyhow::Result<()> {
    let mut ctx = rt.ctx.lock().await;
    let now = shared.clock.now();
    if ctx.core.role() == Role::Owner && ctx.core.my_chain.genesis.signer == shared.identity.device_id() {
        sender.send(ClientFrame::RegisterRoom { genesis: ctx.core.my_chain.genesis.clone() }).await?;
    }
    sender
        .send(ClientFrame::Subscribe {
            room: rt.id,
            chain: Some(Box::new(ctx.core.my_chain.clone())),
            after: ctx.core.cursor,
        })
        .await?;
    ctx.sender = Some(sender.clone());
    ctx.connected = true;
    ctx.connected_at = Some(tokio::time::Instant::now());
    // Re-host live codes: the relay forgets them when the host disconnects.
    for (r, c) in codes_to_rehost(&ctx, now) {
        sender.send(ClientFrame::HostCode { rendezvous: r, kind: c.kind.clone(), expires_at: c.expires_at }).await?;
    }
    let effects = ctx.core.on_connected(now);
    apply(rt, &mut ctx, shared, effects).await;
    let queued: Vec<Vec<u8>> = ctx.outbox.drain(..).collect();
    for env in queued {
        sender.send(ClientFrame::Publish { envelope: env }).await?;
    }
    Ok(())
}

async fn tick_loop(rt: Arc<RoomRuntime>, shared: Arc<Shared>) {
    let mut last_tick = 0i64;
    let mut interval = tokio::time::interval(shared.tick_every);
    loop {
        interval.tick().await;
        let now = shared.clock.now();
        let mut ctx = rt.ctx.lock().await;
        let hue_pending = ctx.core.my_profile.hue == polyph_core::room::NO_HUE;
        if now - last_tick >= polyph_core::election::HEARTBEAT || now < last_tick || hue_pending {
            last_tick = now;
            let effects = ctx.core.tick(now);
            apply(&rt, &mut ctx, &shared, effects).await;
            // Pick a voice hue once we know the hues already in use.
            let settled = ctx.connected_at.is_some_and(|t| t.elapsed() >= Duration::from_secs(2));
            if settled && ctx.core.my_profile.hue == polyph_core::room::NO_HUE {
                let fx = ctx.core.set_hue_if_unset();
                apply(&rt, &mut ctx, &shared, fx).await;
            }
        }
        expire_parked(&rt, &mut ctx, &shared, now).await;
        save_codes(&mut ctx, &shared, now);
        if ctx.connected && (ctx.dirty && now - ctx.last_snapshot >= SNAPSHOT_EVERY) {
            upload_snapshot(&mut ctx, now).await;
        }
        if ctx.dirty {
            ctx.dirty = false;
            // Encrypting and writing a large room is slow; do it after
            // letting go of the room so agents and peers are not held up.
            let snapshot = shared.room_snapshot(&mut ctx.core);
            drop(ctx);
            if let Err(e) = snapshot.and_then(|s| shared.write_room(&s)) {
                warn!(error = %e, "saving room failed");
            }
        }
    }
}

pub async fn upload_snapshot(ctx: &mut RoomCtx, now: i64) {
    // The relay refuses snapshots from Viewers, and other clients ignore them.
    if !ctx.core.role().can_write() {
        return;
    }
    let room = ctx.core.id;
    match ctx.core.relay_snapshot() {
        Ok((epoch, blob)) => {
            if let Some(s) = &ctx.sender {
                let _ = s.send(ClientFrame::PutSnapshot { room, epoch, blob }).await;
                ctx.last_snapshot = now;
            }
        }
        Err(e) => warn!(error = %e, "snapshot failed"),
    }
}

async fn handle_frame(rt: &RoomRuntime, shared: &Shared, f: RelayFrame) {
    let now = shared.clock.now();
    let mut ctx = rt.ctx.lock().await;
    match f {
        RelayFrame::Subscribed { writer, .. } => {
            if !writer {
                shared.emit(ApiEvent::room_kind(
                    &rt.id,
                    "error",
                    serde_json::json!({ "message": "the relay did not accept this device as a member" }),
                ));
            }
        }
        RelayFrame::Deliver { room, cursor, envelope } if room == rt.id => {
            ctx.core.cursor = Some(cursor);
            let effects = ctx.core.receive(&envelope, now);
            apply(rt, &mut ctx, shared, effects).await;
        }
        RelayFrame::CaughtUp { .. } => {
            if ctx.needs_snapshot {
                ctx.needs_snapshot = false;
                if let Some(s) = &ctx.sender {
                    let _ = s.send(ClientFrame::GetSnapshots { room: rt.id }).await;
                }
            }
        }
        RelayFrame::Gap { .. } => {
            ctx.core.cursor = None;
            if let Some(s) = &ctx.sender {
                let _ = s.send(ClientFrame::GetSnapshots { room: rt.id }).await;
            }
            let effects = ctx.core.sync_requests();
            apply(rt, &mut ctx, shared, effects).await;
        }
        RelayFrame::Snapshots { items, .. } => {
            let me = shared.identity.device_id();
            let mut any = false;
            for item in items.iter().filter(|i| i.from != me) {
                any |= ctx.core.apply_relay_snapshot(&item.blob, now);
            }
            if any {
                for d in [polyph_proto::messages::DocName::Board, polyph_proto::messages::DocName::Threads] {
                    shared.emit(ApiEvent::room(&rt.id, &RoomEvent::DocChanged { doc: d }));
                }
                ctx.dirty = true;
            }
        }
        RelayFrame::RoomEnded { .. } => {
            ctx.offline = true;
            shared.emit(ApiEvent::room(&rt.id, &RoomEvent::RoomEnded));
        }
        RelayFrame::CodeHosted { rendezvous } => {
            if let Some(pos) = ctx.code_waiters.iter().position(|(r, _)| *r == rendezvous)
                && let Some((_, tx)) = ctx.code_waiters.remove(pos)
            {
                let _ = tx.send(true);
            }
        }
        RelayFrame::Error { code: polyph_proto::relay_wire::ErrorCode::CodeInUse, .. } => {
            // The oldest outstanding invite collided with another room's code.
            if let Some((r, tx)) = ctx.code_waiters.pop_front() {
                ctx.codes.remove(&r);
                let _ = tx.send(false);
            }
        }
        RelayFrame::CodeClosed { rendezvous, reason } => {
            if matches!(reason, CodeClosed::HostLeft) {
                return;
            }
            // A code held for a reconnecting joiner was put back after the
            // relay counted the use that retired it; this host decides when
            // it goes (it retires used codes itself, see `code_verified`).
            if matches!(reason, CodeClosed::Used)
                && ctx.codes.get(&rendezvous).is_some_and(|c| c.reserved_for.is_some())
            {
                return;
            }
            if let Some(c) = ctx.codes.remove(&rendezvous) {
                shared.emit(ApiEvent::room_kind(
                    &rt.id,
                    "code_closed",
                    serde_json::json!({ "code": c.code.to_string(), "reason": format!("{reason:?}").to_lowercase() }),
                ));
            }
        }
        RelayFrame::CodeLocked { rendezvous, until } => {
            if let Some(c) = ctx.codes.get(&rendezvous) {
                shared.emit(ApiEvent::room_kind(
                    &rt.id,
                    "code_locked",
                    serde_json::json!({ "code": c.code.to_string(), "until": until }),
                ));
            }
        }
        RelayFrame::JoinerArrived { rendezvous, session } => host_join_start(&mut ctx, rendezvous, session).await,
        RelayFrame::Pake { session, data } => host_join_step(rt, &mut ctx, shared, session, data, now).await,
        RelayFrame::PakeEnded { session, .. } => {
            let Some(j) = ctx.joins.remove(&session) else { return };
            end_join(&mut ctx, j, now, true).await;
        }
        RelayFrame::Error { code, detail } => {
            debug!(?code, "relay error");
            shared.emit(ApiEvent::room_kind(
                &rt.id,
                "relay_error",
                serde_json::json!({ "code": format!("{code:?}"), "message": detail }),
            ));
        }
        _ => {}
    }
}

async fn host_join_start(ctx: &mut RoomCtx, rendezvous: RendezvousId, session: u64) {
    let Some(code) = ctx.codes.get(&rendezvous) else { return };
    if code.failures >= HOST_MAX_FAILURES {
        if let Some(s) = &ctx.sender {
            let _ = s.send(ClientFrame::PakeResult { session, ok: false }).await;
        }
        return;
    }
    let own_device = code.own_device;
    let p = PakeStart::new(&code.code, PakeRole::Host, &ctx.binding);
    let msg = p.message().to_vec();
    let reserved_for = code.reserved_for;
    ctx.joins.insert(
        session,
        HostJoin { rendezvous, stage: Some(HostStage::AwaitPake(p)), own_device, reserved_for, retired: None },
    );
    if let Some(s) = &ctx.sender {
        let _ = s.send(ClientFrame::Pake { session, data: msg }).await;
    }
}

async fn host_join_step(rt: &RoomRuntime, ctx: &mut RoomCtx, shared: &Shared, session: u64, data: Vec<u8>, now: i64) {
    let Some(mut j) = ctx.joins.remove(&session) else { return };
    let Some(sender) = ctx.sender.clone() else { return };
    let stage = j.stage.take();
    let fail = |ctx: &mut RoomCtx, r: RendezvousId| {
        if let Some(c) = ctx.codes.get_mut(&r) {
            c.failures += 1;
        }
    };
    match stage {
        // The host's confirmation tag goes out only after the joiner's has
        // verified, so a guesser learns nothing it could check offline without
        // spending a counted attempt. Joiners send their tag without waiting
        // for the host's, so the order they receive is unchanged.
        Some(HostStage::AwaitPake(p)) => match p.finish(&data) {
            Ok(f) => {
                j.stage = Some(HostStage::AwaitConfirm(f));
                ctx.joins.insert(session, j);
            }
            Err(_) => {
                fail(ctx, j.rendezvous);
                let _ = sender.send(ClientFrame::PakeResult { session, ok: false }).await;
            }
        },
        Some(HostStage::AwaitConfirm(f)) => match (f.confirmation(), f.verify_peer(&data)) {
            (tag, Ok(ch)) => {
                let _ = sender.send(ClientFrame::Pake { session, data: tag.to_vec() }).await;
                let _ = sender.send(ClientFrame::PakeResult { session, ok: true }).await;
                if let Some(c) = code_verified(ctx, j.rendezvous) {
                    if c.reserved_for.is_none() {
                        shared.emit(ApiEvent::room_kind(
                            &rt.id,
                            "code_closed",
                            serde_json::json!({ "code": c.code.to_string(), "reason": "used" }),
                        ));
                    }
                    j.retired = Some(c);
                    // Off the disk now, not at the next tick: a used code
                    // must not come back after a crash.
                    save_codes(ctx, shared, now);
                }
                j.stage = Some(HostStage::AwaitRequest(ch));
                ctx.joins.insert(session, j);
            }
            (_, Err(_)) => {
                fail(ctx, j.rendezvous);
                let _ = sender.send(ClientFrame::PakeResult { session, ok: false }).await;
            }
        },
        Some(HostStage::AwaitRequest(mut ch)) => {
            let Ok(pt) = ch.open(&data) else { return };
            let Ok(req) = from_cbor::<JoinRequest>(&pt) else { return };
            let dev = req.device.sign;
            // The same joiner back after its connection dropped takes the
            // waiting knock back. Device ids are public, so only exactly the
            // same request does (code, member id, both device keys, name
            // salt); anything else leaves the waiting knock alone. The answer
            // is sealed to the knock's `kx`, of use only to its device.
            let exact = ctx.parked.get(&dev).is_some_and(|p| {
                p.rendezvous == j.rendezvous
                    && p.member == req.member
                    && p.kx.as_slice() == req.device.kx.as_slice()
                    && p.name_salt == req.name_salt
            });
            // Back before the relay noticed the old connection die: the old
            // session still holds the knock. Move the knock to this one
            // rather than knocking twice.
            let live = ctx
                .joins
                .iter()
                .find(|(_, o)| {
                    o.rendezvous == j.rendezvous
                        && matches!(&o.stage, Some(HostStage::Knocked { knock, .. })
                        if ctx.core.knocks.get(knock).is_some_and(|k| {
                            k.device == dev
                                && k.member == req.member
                                && k.kx.as_slice() == req.device.kx.as_slice()
                                && k.name_salt == req.name_salt
                        }))
                })
                .map(|(s, _)| *s);
            if let Some(old) = live
                && let Some(HostJoin { stage: Some(HostStage::Knocked { knock, .. }), .. }) = ctx.joins.remove(&old)
            {
                send_reply(&mut ch, &sender, session, &JoinReply::Pending).await;
                j.stage = Some(HostStage::Knocked { channel: ch, knock });
                ctx.joins.insert(session, j);
                return;
            }
            if exact && let Some(p) = ctx.parked.remove(&dev) {
                if let Some(reply) = &p.reply {
                    send_reply(&mut ch, &sender, session, reply).await;
                    return;
                }
                if ctx.core.knocks.contains_key(&p.knock) {
                    send_reply(&mut ch, &sender, session, &JoinReply::Pending).await;
                    j.stage = Some(HostStage::Knocked { channel: ch, knock: p.knock });
                    ctx.joins.insert(session, j);
                    return;
                }
            }
            // A used code hosted again for one reconnecting joiner never
            // starts a new knock: only that exact re-attach above uses it.
            if j.reserved_for.is_some() {
                let denied = JoinReply::Denied { reason: "this code was already used".into() };
                send_reply(&mut ch, &sender, session, &denied).await;
                if let Some(c) = j.retired.take() {
                    restore_code(ctx, j.rendezvous, c).await;
                }
                return;
            }
            let knock = Knock {
                id: ObjectId::random().unwrap_or(ObjectId([0; 16])),
                display_name: req.display_name.chars().take(64).collect(),
                member: req.member,
                device: req.device.sign,
                requested_role: Role::Member,
                via: shared.identity.device_id(),
                at: now,
                kx: req.device.kx.to_vec(),
                name_salt: req.name_salt.clone(),
                surface: Some(req.surface.clone()),
                capabilities: req.capabilities.clone(),
            };
            let kid = knock.id;
            // Only a code `phone.pair` or `host.invite` made on this device, for
            // this member, may add another device of this member; plain
            // `room.invite` codes never do. A knock that names an existing
            // member is never approved automatically: a person decides.
            let own = j.own_device == Some(req.member) && req.member == ctx.core.my_member;
            if own {
                ctx.core.allow_own_device(kid);
            }
            let auto = !ctx.core.knock_claims_member(&knock)
                && !ctx.core.require_approval()
                && ctx.core.role().can_approve_joins();
            j.stage = Some(HostStage::Knocked { channel: ch, knock: kid });
            ctx.joins.insert(session, j);
            let effects = ctx.core.post_knock(knock);
            apply(rt, ctx, shared, effects).await;
            if auto {
                match ctx.core.approve_knock(kid, Role::Member, now) {
                    Ok(fx) => apply(rt, ctx, shared, fx).await,
                    Err(e) => warn!(error = %e, "auto-approve failed"),
                }
            } else if let Some(HostJoin { stage: Some(HostStage::Knocked { channel, .. }), .. }) =
                ctx.joins.get_mut(&session)
            {
                send_reply(channel, &sender, session, &JoinReply::Pending).await;
            }
        }
        Some(HostStage::Knocked { channel, knock }) => {
            // The joiner has nothing more to say while waiting.
            j.stage = Some(HostStage::Knocked { channel, knock });
            ctx.joins.insert(session, j);
        }
        None => {}
    }
}

/// Seals a join reply to the joiner on this session.
async fn send_reply(channel: &mut SecureChannel, sender: &RelaySender, session: u64, reply: &JoinReply) {
    if let Ok(body) = to_cbor(reply)
        && let Ok(ct) = channel.seal(&body)
    {
        let _ = sender.send(ClientFrame::Pake { session, data: ct }).await;
    }
}

/// Ends a join whose relay session is gone (`PakeEnded`, or this host's
/// connection dropped). Before the joiner proved the code it is a failed
/// attempt when `counted`. After, a code it used up goes back up, and a knock
/// waits for the joiner to reconnect (see [`Parked`]).
async fn end_join(ctx: &mut RoomCtx, j: HostJoin, now: i64, counted: bool) {
    match j.stage {
        Some(HostStage::AwaitPake(_) | HostStage::AwaitConfirm(_)) => {
            if counted && let Some(c) = ctx.codes.get_mut(&j.rendezvous) {
                c.failures += 1;
            }
        }
        // The joiner proved the code but never asked to join: still unused.
        Some(HostStage::AwaitRequest(_)) => {
            if let Some(c) = j.retired {
                restore_code(ctx, j.rendezvous, c).await;
            }
        }
        Some(HostStage::Knocked { knock, .. }) => {
            // Already answered (and removed) means there is nothing to wait for.
            let Some(k) = ctx.core.knocks.get(&knock).cloned() else { return };
            ctx.parked.insert(
                k.device,
                Parked {
                    knock,
                    member: k.member,
                    kx: k.kx,
                    name_salt: k.name_salt,
                    rendezvous: j.rendezvous,
                    since: now,
                    reply: None,
                },
            );
            if let Some(mut c) = j.retired {
                c.reserved_for = Some(k.device);
                restore_code(ctx, j.rendezvous, c).await;
            }
        }
        None => {}
    }
}

/// Holds a code again and hosts it now, or on the next connect. A code held
/// for a joiner whose knock no longer waits stays retired.
async fn restore_code(ctx: &mut RoomCtx, r: RendezvousId, c: HostedCode) {
    if c.reserved_for.is_some_and(|d| !ctx.parked.contains_key(&d)) {
        return;
    }
    // The relay refuses an expired code.
    if c.expires_at <= ctx.core.last_now {
        return;
    }
    let (kind, expires_at) = (c.kind.clone(), c.expires_at);
    ctx.codes.insert(r, c);
    if let (Some(s), true) = (&ctx.sender, ctx.connected) {
        let _ = s.send(ClientFrame::HostCode { rendezvous: r, kind, expires_at }).await;
    }
}

/// Forgets knocks whose joiner did not come back within [`PARK_SECS`], with
/// any code held for that joiner.
async fn expire_parked(rt: &RoomRuntime, ctx: &mut RoomCtx, shared: &Shared, now: i64) {
    let gone: Vec<DeviceId> = ctx.parked.iter().filter(|(_, p)| now - p.since >= PARK_SECS).map(|(d, _)| *d).collect();
    for d in gone {
        let Some(p) = ctx.parked.remove(&d) else { continue };
        if p.reply.is_none() && ctx.core.knocks.contains_key(&p.knock) {
            ctx.core.drop_knock(&p.knock);
            // Local only: tells this device's apps the knock is gone.
            shared.emit(ApiEvent::room(&rt.id, &RoomEvent::KnockDecided { id: p.knock, approved: false }));
        }
        let held: Vec<RendezvousId> =
            ctx.codes.iter().filter(|(_, c)| c.reserved_for == Some(d)).map(|(r, _)| *r).collect();
        for r in held {
            ctx.codes.remove(&r);
            if let (Some(s), true) = (&ctx.sender, ctx.connected) {
                let _ = s.send(ClientFrame::RevokeCode { rendezvous: r }).await;
            }
        }
    }
}

/// A join code as saved on disk, sealed with the storage key, so codes such
/// as the one `polyph start` prints keep working after a restart. Codes for
/// another device of this member (SEC-3) and codes held for one reconnecting
/// joiner stay in memory only.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub(crate) struct SavedCode {
    code: String,
    kind: CodeKind,
    expires_at: i64,
    failures: u32,
    uses: u32,
}

fn codes_to_save(ctx: &RoomCtx, now: i64) -> Vec<SavedCode> {
    let mut v: Vec<SavedCode> = ctx
        .codes
        .values()
        .filter(|c| c.own_device.is_none() && c.reserved_for.is_none() && c.expires_at > now)
        .map(|c| SavedCode {
            code: c.code.to_string(),
            kind: c.kind.clone(),
            expires_at: c.expires_at,
            failures: c.failures,
            uses: c.uses,
        })
        .collect();
    v.sort_by(|a, b| a.code.cmp(&b.code));
    v
}

fn codes_aad(hex: &str) -> Vec<u8> {
    format!("{hex}.codes").into_bytes()
}

/// Writes the room's join codes when they changed since the last write
/// (removing the file once none is left).
pub fn save_codes(ctx: &mut RoomCtx, shared: &Shared, now: i64) {
    let list = codes_to_save(ctx, now);
    if ctx.saved_codes.as_ref() == Some(&list) {
        return;
    }
    let hex = ctx.core.id.to_hex();
    let path = shared.paths.room_codes_file(&hex);
    let written = if list.is_empty() {
        match std::fs::remove_file(&path) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e.into()),
            _ => Ok(()),
        }
    } else {
        to_cbor(&list)
            .map(zeroize::Zeroizing::new)
            .map_err(anyhow::Error::from)
            .and_then(|b| shared.key.seal(&codes_aad(&hex), &b))
            .and_then(|sealed| Ok(crate::paths::write_private(&path, &sealed)?))
    };
    match written {
        Ok(()) => ctx.saved_codes = Some(list),
        Err(e) => warn!(error = %e, "saving join codes failed"),
    }
}

/// Loads the room's saved join codes that have not expired; the connection
/// hosts them on the relay as it does every live code.
pub fn load_codes(ctx: &mut RoomCtx, shared: &Shared, now: i64) {
    let hex = ctx.core.id.to_hex();
    let Ok(data) = std::fs::read(shared.paths.room_codes_file(&hex)) else { return };
    let list: Vec<SavedCode> = match shared.key.open(&codes_aad(&hex), &data).and_then(|pt| Ok(from_cbor(&pt)?)) {
        Ok(l) => l,
        Err(e) => {
            warn!(room = %ctx.core.id.short(), error = %e, "could not read saved join codes");
            return;
        }
    };
    for s in list.iter().filter(|s| s.expires_at > now) {
        let Ok(code) = JoinCode::parse(&s.code) else { continue };
        ctx.codes.insert(
            code.rendezvous(),
            HostedCode {
                code,
                kind: s.kind.clone(),
                expires_at: s.expires_at,
                failures: s.failures,
                uses: s.uses,
                own_device: None,
                reserved_for: None,
            },
        );
    }
    ctx.saved_codes = Some(list);
}

/// Codes to host again after (re)connecting: the live ones this host still holds.
fn codes_to_rehost(ctx: &RoomCtx, now: i64) -> Vec<(RendezvousId, HostedCode)> {
    ctx.codes.iter().filter(|(_, c)| c.expires_at > now).map(|(r, c)| (*r, c.clone())).collect()
}

/// A joiner proved it knows the code. Resets the failure count and retires
/// the code here once it is used up (a one-time code at once), without
/// waiting for the relay's `CodeClosed`, which a reconnect can lose: the code
/// would otherwise be hosted again. Returns the retired code.
fn code_verified(ctx: &mut RoomCtx, r: RendezvousId) -> Option<HostedCode> {
    let c = ctx.codes.get_mut(&r)?;
    c.failures = 0;
    c.uses += 1;
    let done = match c.kind {
        CodeKind::OneTime => true,
        CodeKind::Reusable { max_uses } => max_uses.is_some_and(|m| c.uses >= m),
    };
    if done { ctx.codes.remove(&r) } else { None }
}

/// Creates a join code and asks the relay to host it. Returns the code and a
/// receiver that resolves once the relay confirms (`true`) or refuses it
/// because the rendezvous is taken (`false`). When disconnected the code is
/// hosted on reconnect and no receiver is returned. `own_device` marks a code
/// made for another device of this member (see `HostedCode::own_device`).
pub async fn host_code(
    ctx: &mut RoomCtx,
    kind: CodeKind,
    expires_at: i64,
    own_device: bool,
) -> anyhow::Result<(JoinCode, Option<oneshot::Receiver<bool>>)> {
    let code = JoinCode::generate()?;
    let r = code.rendezvous();
    let own_device = own_device.then_some(ctx.core.my_member);
    ctx.codes.insert(
        r,
        HostedCode {
            code: code.clone(),
            kind: kind.clone(),
            expires_at,
            failures: 0,
            uses: 0,
            own_device,
            reserved_for: None,
        },
    );
    let mut waiter = None;
    if let (Some(s), true) = (&ctx.sender, ctx.connected) {
        let (tx, rx) = oneshot::channel();
        ctx.code_waiters.push_back((r, tx));
        s.send(ClientFrame::HostCode { rendezvous: r, kind, expires_at }).await?;
        waiter = Some(rx);
    }
    Ok((code, waiter))
}

/// Member id used for a member's actions in this room.
pub fn my_member(ctx: &RoomCtx) -> MemberId {
    ctx.core.my_member
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;
    use polyph_proto::identity::DeviceIdentity;

    fn ctx() -> Arc<RoomRuntime> {
        let id = Arc::new(DeviceIdentity::generate().unwrap());
        let (core, _) = RoomCore::create(id, MemberId::random().unwrap(), "payments", "Ann", false, 1_000).unwrap();
        RoomRuntime::new(core, RelayAddr::parse("ws://127.0.0.1:9/ws"))
    }

    #[tokio::test]
    async fn a_used_one_time_code_is_not_hosted_again_after_a_reconnect() {
        let rt = ctx();
        let mut c = rt.ctx.lock().await;
        let (pair, _) = host_code(&mut c, CodeKind::OneTime, 5_000, true).await.unwrap();
        let (reusable, _) = host_code(&mut c, CodeKind::Reusable { max_uses: Some(2) }, 5_000, false).await.unwrap();
        assert_eq!(codes_to_rehost(&c, 1_000).len(), 2);
        // The pairing code verifies once; the relay's `CodeClosed` never arrives.
        assert!(code_verified(&mut c, pair.rendezvous()).is_some());
        let live: Vec<RendezvousId> = codes_to_rehost(&c, 1_000).into_iter().map(|(r, _)| r).collect();
        assert_eq!(live, vec![reusable.rendezvous()], "the used pairing code is gone");
        // A reusable code lasts until its use limit.
        assert!(code_verified(&mut c, reusable.rendezvous()).is_none());
        assert!(code_verified(&mut c, reusable.rendezvous()).is_some());
        assert!(codes_to_rehost(&c, 1_000).is_empty());
    }
}
