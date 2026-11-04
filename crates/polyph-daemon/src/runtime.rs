//! One open room: its engine, its relay connection and the host side of
//! joins that arrive on that connection.

use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use std::time::Duration;

use polyph_core::{Effect, RoomCore, RoomEvent};
use polyph_proto::cert::Role;
use polyph_proto::codec::{from_cbor, to_cbor};
use polyph_proto::ids::{MemberId, ObjectId, RendezvousId, RoomId};
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
}

/// Resolves with (granted, denied) when the acting host answers a claim request.
pub type ClaimWaiter = oneshot::Sender<(Vec<Claim>, Vec<Claim>)>;

pub struct RoomCtx {
    pub core: RoomCore,
    pub sender: Option<RelaySender>,
    pub connected: bool,
    pub offline: bool,
    pub dirty: bool,
    pub relay: RelayAddr,
    /// Fingerprint bound into join PAKEs on this connection (LAN relays).
    pub binding: Vec<u8>,
    pub codes: HashMap<RendezvousId, HostedCode>,
    joins: HashMap<u64, HostJoin>,
    outbox: VecDeque<Vec<u8>>,
    pub claim_waiters: HashMap<ObjectId, ClaimWaiter>,
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
                offline: false,
                dirty: true,
                relay,
                binding: vec![],
                codes: HashMap::new(),
                joins: HashMap::new(),
                outbox: VecDeque::new(),
                claim_waiters: HashMap::new(),
                coord: Default::default(),
                mirror: None,
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
                    RoomEvent::JoinDenied { knock } => {
                        let reply = JoinReply::Denied { reason: "denied by the room".into() };
                        finish_knocked_join(ctx, *knock, &reply).await;
                    }
                    RoomEvent::ClaimDecision { request, granted, denied } => {
                        if let Some(tx) = ctx.claim_waiters.remove(request) {
                            let _ = tx.send((granted.clone(), denied.clone()));
                        }
                    }
                    RoomEvent::ApprovalResolved { id, decision, .. } => {
                        if let Some(tx) = ctx.approval_waiters.remove(id) {
                            let _ = tx.send(decision.clone());
                        }
                    }
                    RoomEvent::Inbox { message, for_me: true } => {
                        ctx.coord.deliver(message.clone());
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
                shared.emit(ApiEvent::room(&rt.id, &ev));
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
    let Some(session) = session else { return };
    let Some(mut j) = ctx.joins.remove(&session) else { return };
    if let Some(HostStage::Knocked { mut channel, .. }) = j.stage.take() {
        let Ok(body) = to_cbor(reply) else { return };
        if let (Ok(ct), Some(s)) = (channel.seal(&body), &ctx.sender) {
            let _ = s.send(ClientFrame::Pake { session, data: ct }).await;
        }
    }
}

async fn connection_loop(rt: Arc<RoomRuntime>, shared: Arc<Shared>) {
    let mut backoff = Duration::from_millis(500);
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
                debug!(error = %e, "relay connect failed");
                tokio::select! {
                    _ = tokio::time::sleep(backoff) => {}
                    _ = rt.kick.notified() => {}
                }
                backoff = (backoff * 2).min(Duration::from_secs(30));
                continue;
            }
        };
        backoff = Duration::from_millis(500);
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
    let codes: Vec<(RendezvousId, HostedCode)> = ctx.codes.iter().map(|(r, c)| (*r, c.clone())).collect();
    for (r, c) in codes {
        if c.expires_at > now {
            sender
                .send(ClientFrame::HostCode { rendezvous: r, kind: c.kind.clone(), expires_at: c.expires_at })
                .await?;
        }
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
        if ctx.connected && (ctx.dirty && now - ctx.last_snapshot >= SNAPSHOT_EVERY) {
            upload_snapshot(&mut ctx, now).await;
        }
        if ctx.dirty {
            ctx.dirty = false;
            if let Err(e) = shared.save_room(&mut ctx.core) {
                warn!(error = %e, "saving room failed");
            }
        }
    }
}

pub async fn upload_snapshot(ctx: &mut RoomCtx, now: i64) {
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
            ctx.joins.remove(&session);
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
    let p = PakeStart::new(&code.code, PakeRole::Host, &ctx.binding);
    let msg = p.message().to_vec();
    ctx.joins.insert(session, HostJoin { rendezvous, stage: Some(HostStage::AwaitPake(p)) });
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
        Some(HostStage::AwaitPake(p)) => match p.finish(&data) {
            Ok(f) => {
                let _ = sender.send(ClientFrame::Pake { session, data: f.confirmation().to_vec() }).await;
                j.stage = Some(HostStage::AwaitConfirm(f));
                ctx.joins.insert(session, j);
            }
            Err(_) => {
                fail(ctx, j.rendezvous);
                let _ = sender.send(ClientFrame::PakeResult { session, ok: false }).await;
            }
        },
        Some(HostStage::AwaitConfirm(f)) => match f.verify_peer(&data) {
            Ok(ch) => {
                let _ = sender.send(ClientFrame::PakeResult { session, ok: true }).await;
                if let Some(c) = ctx.codes.get_mut(&j.rendezvous) {
                    c.failures = 0;
                }
                j.stage = Some(HostStage::AwaitRequest(ch));
                ctx.joins.insert(session, j);
            }
            Err(_) => {
                fail(ctx, j.rendezvous);
                let _ = sender.send(ClientFrame::PakeResult { session, ok: false }).await;
            }
        },
        Some(HostStage::AwaitRequest(mut ch)) => {
            let Ok(pt) = ch.open(&data) else { return };
            let Ok(req) = from_cbor::<JoinRequest>(&pt) else { return };
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
            };
            let kid = knock.id;
            let auto = !ctx.core.require_approval() && ctx.core.role().can_approve_joins();
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
                && let (Ok(body), Some(s)) = (to_cbor(&JoinReply::Pending), &ctx.sender)
                && let Ok(ct) = channel.seal(&body)
            {
                let _ = s.send(ClientFrame::Pake { session, data: ct }).await;
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

/// Creates a join code and asks the relay to host it. Returns the code and a
/// receiver that resolves once the relay confirms (`true`) or refuses it
/// because the rendezvous is taken (`false`). When disconnected the code is
/// hosted on reconnect and no receiver is returned.
pub async fn host_code(
    ctx: &mut RoomCtx,
    kind: CodeKind,
    expires_at: i64,
) -> anyhow::Result<(JoinCode, Option<oneshot::Receiver<bool>>)> {
    let code = JoinCode::generate()?;
    let r = code.rendezvous();
    ctx.codes.insert(r, HostedCode { code: code.clone(), kind: kind.clone(), expires_at, failures: 0 });
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
