//! A room member that lives in a browser with no daemon: the phone PWA.
//!
//! [`Phone`] is a state machine with no I/O, like `polyph-core`. The page
//! opens a WebSocket to the relay, passes every binary frame to
//! [`Phone::frame`], sends the frames each call returns, and calls
//! [`Phone::tick`] every few seconds. The same machine runs natively in tests
//! (`polyph-testkit/tests/m8.rs`) and in the browser through `wasm.rs`.
//!
//! It covers relay authentication, the joiner side of the join handshake
//! (docs/protocol.md 7.3) and one room afterwards: chat, threads, the board,
//! agents' transcripts as they arrive, approvals and knocks. It never runs
//! agents, terminals or merges; those stay on daemons.

pub mod api;
#[cfg(feature = "wasm")]
pub mod wasm;

use std::collections::{BTreeMap, VecDeque};
use std::sync::Arc;

use polyph_core::{Effect, RoomCore, RoomEvent, SavedRoom};
use polyph_proto::codec::{from_cbor, to_cbor};
use polyph_proto::identity::DeviceIdentity;
use polyph_proto::identity::ctx;
use polyph_proto::ids::{MemberId, ObjectId};
use polyph_proto::join::{JoinReply, JoinRequest};
use polyph_proto::joincode::JoinCode;
use polyph_proto::messages::{AgentEvent, DocName, RoomMessage, Surface};
use polyph_proto::pake::{PakeFinished, PakeRole, PakeStart, SecureChannel};
use polyph_proto::relay_wire::{ClientFrame, ErrorCode, RelayFrame, auth_message};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

/// Messages kept while the relay is unreachable; the oldest go first.
const OUTBOX_MAX: usize = 500;
/// Transcript events kept per agent (the phone shows recent work only).
pub const TRANSCRIPT_KEEP: usize = 1000;
/// Seconds between engine ticks (heartbeats, election, escalation).
pub const TICK_SECS: i64 = 5;
/// Seconds after connecting before choosing a voice hue, so the hues already
/// in use have arrived.
const HUE_SETTLE: i64 = 2;

#[derive(Debug, thiserror::Error)]
pub enum PhoneError {
    #[error("{0}")]
    Bad(String),
    #[error("{0}")]
    NotFound(String),
    #[error("{0}")]
    Forbidden(String),
    #[error("{0}")]
    Unsupported(String),
}

impl PhoneError {
    /// JSON-RPC error code, matching the daemon's API.
    pub fn code(&self) -> i64 {
        match self {
            Self::Bad(_) => -32602,
            Self::NotFound(_) => 404,
            Self::Forbidden(_) => 403,
            Self::Unsupported(_) => -32601,
        }
    }
}

impl From<polyph_core::RoomError> for PhoneError {
    fn from(e: polyph_core::RoomError) -> Self {
        match e {
            polyph_core::RoomError::Forbidden => Self::Forbidden(e.to_string()),
            polyph_core::RoomError::NotFound(_) => Self::NotFound(e.to_string()),
            _ => Self::Bad(e.to_string()),
        }
    }
}

impl From<polyph_proto::ProtoError> for PhoneError {
    fn from(e: polyph_proto::ProtoError) -> Self {
        Self::Bad(e.to_string())
    }
}

pub type Result<T> = std::result::Result<T, PhoneError>;

/// What one call produced: frames for the relay and events for the page.
#[derive(Debug, Default)]
pub struct Output {
    /// Encoded `ClientFrame`s to send on the WebSocket, in order.
    pub frames: Vec<Vec<u8>>,
    /// Events shaped like the daemon's: `{ room, event: { type, ... } }`.
    pub events: Vec<Value>,
    /// The saved state changed; the page should call [`Phone::save`].
    pub dirty: bool,
}

impl Output {
    fn frame(&mut self, f: &ClientFrame) {
        if let Ok(b) = f.encode() {
            self.frames.push(b);
        }
    }

    fn event(&mut self, room: Option<String>, event: Value) {
        self.events.push(json!({ "room": room, "event": event }));
    }

    fn global(&mut self, kind: &str, mut data: Value) {
        if let Value::Object(o) = &mut data {
            o.insert("type".into(), json!(kind));
        }
        self.event(None, data);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Conn {
    Closed,
    AwaitChallenge,
    AwaitWelcome,
    Open,
}

/// What the joiner waits for next.
enum JoinStage {
    Started,
    Host(PakeStart),
    Tag(PakeFinished),
    Reply(SecureChannel),
}

struct Join {
    code: JoinCode,
    salt: [u8; 16],
    session: u64,
    stage: Option<JoinStage>,
}

/// The joined room and what the phone keeps beside the engine.
pub struct Room {
    pub core: RoomCore,
    pub transcripts: BTreeMap<ObjectId, VecDeque<AgentEvent>>,
    outbox: VecDeque<Vec<u8>>,
    subscribed: bool,
    needs_snapshot: bool,
    connected_at: Option<i64>,
    last_tick: i64,
}

impl std::fmt::Debug for Room {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Room({})", self.core.id.short())
    }
}

impl Room {
    fn new(core: RoomCore) -> Self {
        Self {
            core,
            transcripts: BTreeMap::new(),
            outbox: VecDeque::new(),
            subscribed: false,
            needs_snapshot: true,
            connected_at: None,
            last_tick: 0,
        }
    }

    pub fn id_hex(&self) -> String {
        self.core.id.to_hex()
    }
}

/// What the page stores between visits (sealed by the page before storage).
#[derive(Serialize, Deserialize)]
struct Saved {
    member: MemberId,
    name: String,
    relay: String,
    room: Option<SavedRoom>,
}

/// One browser-only member: one relay connection and at most one room.
pub struct Phone {
    identity: Arc<DeviceIdentity>,
    pub member: MemberId,
    pub name: String,
    /// The relay this phone talks to (`wss://…/ws`).
    pub relay: String,
    conn: Conn,
    join: Option<Join>,
    pub room: Option<Room>,
}

impl std::fmt::Debug for Phone {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Phone({}, {:?}, {:?})", self.identity.device_id().short(), self.conn, self.room)
    }
}

impl Phone {
    /// A phone that has not joined a room yet. `member` is the person's member
    /// id when this phone pairs with their existing devices (so it can answer
    /// their agents' approvals), or a new one.
    pub fn new(identity: Arc<DeviceIdentity>, member: Option<MemberId>, name: &str, relay: &str) -> Result<Self> {
        let member = match member {
            Some(m) => m,
            None => MemberId::random()?,
        };
        Ok(Self {
            identity,
            member,
            name: name.to_string(),
            relay: relay.to_string(),
            conn: Conn::Closed,
            join: None,
            room: None,
        })
    }

    /// Restores a phone from [`Phone::save`].
    pub fn restore(identity: Arc<DeviceIdentity>, bytes: &[u8], now: i64) -> Result<Self> {
        let s: Saved = from_cbor(bytes)?;
        let room = match s.room {
            Some(r) => {
                let mut core = RoomCore::load(identity.clone(), r, now)?;
                core.surface = Surface::Phone;
                Some(Room::new(core))
            }
            None => None,
        };
        Ok(Self { identity, member: s.member, name: s.name, relay: s.relay, conn: Conn::Closed, join: None, room })
    }

    pub fn save(&mut self) -> Result<Vec<u8>> {
        let s = Saved {
            member: self.member,
            name: self.name.clone(),
            relay: self.relay.clone(),
            room: self.room.as_mut().map(|r| r.core.save()),
        };
        Ok(to_cbor(&s)?)
    }

    pub fn device_hex(&self) -> String {
        self.identity.device_id().to_hex()
    }

    pub fn is_open(&self) -> bool {
        self.conn == Conn::Open
    }

    pub fn joining(&self) -> bool {
        self.join.is_some()
    }

    /// The WebSocket opened: start relay authentication.
    pub fn connected(&mut self) -> Output {
        let mut out = Output::default();
        self.conn = Conn::AwaitChallenge;
        out.frame(&ClientFrame::Hello {
            protocol: polyph_proto::PROTOCOL_VERSION,
            device: self.identity.device_id(),
            client: format!("polyph-phone/{}", env!("CARGO_PKG_VERSION")),
        });
        out
    }

    /// The WebSocket closed. A join in progress fails; the room keeps
    /// queueing messages until the next connection.
    pub fn disconnected(&mut self) -> Output {
        let mut out = Output::default();
        let was_open = self.conn == Conn::Open;
        self.conn = Conn::Closed;
        if self.join.take().is_some() {
            out.global("join_failed", json!({ "message": "the connection to the relay closed during the join" }));
        }
        if let Some(r) = &mut self.room {
            r.subscribed = false;
            r.connected_at = None;
            if was_open {
                out.event(Some(r.id_hex()), json!({ "type": "connection", "connected": false }));
            }
        }
        out
    }

    /// Starts joining a room with a code. The result arrives as a `joined`,
    /// `join_pending` or `join_failed` event.
    pub fn start_join(&mut self, code: &str, name: Option<&str>) -> Result<Output> {
        if self.room.is_some() {
            return Err(PhoneError::Bad("this phone is already in a room; leave it before joining another".into()));
        }
        if self.conn != Conn::Open {
            return Err(PhoneError::Bad("not connected to the relay yet".into()));
        }
        let code = JoinCode::parse(code).map_err(|e| PhoneError::Bad(format!("{e}; codes look like A1B2-CDEF")))?;
        if let Some(n) = name.map(str::trim).filter(|n| !n.is_empty()) {
            self.name = n.chars().take(64).collect();
        }
        let salt = ObjectId::random()?.0;
        let mut out = Output::default();
        out.frame(&ClientFrame::JoinCode { rendezvous: code.rendezvous() });
        self.join = Some(Join { code, salt, session: 0, stage: Some(JoinStage::Started) });
        Ok(out)
    }

    /// Leaves the room on this phone (the room and its other members are
    /// unaffected; an Owner can remove the device's membership).
    pub fn forget_room(&mut self) -> Output {
        let mut out = Output::default();
        if let Some(r) = self.room.take() {
            out.frame(&ClientFrame::Unsubscribe { room: r.core.id });
            out.dirty = true;
        }
        out
    }

    /// One binary frame from the relay.
    pub fn frame(&mut self, bytes: &[u8], now: i64) -> Output {
        let mut out = Output::default();
        let Ok(f) = RelayFrame::decode(bytes) else { return out };
        match (self.conn, f) {
            (Conn::AwaitChallenge, RelayFrame::Challenge { nonce, .. }) => {
                let device = self.identity.device_id();
                let sig = self.identity.sign(ctx::RELAY_AUTH, &auth_message(&nonce, &device));
                out.frame(&ClientFrame::Auth { sig });
                self.conn = Conn::AwaitWelcome;
            }
            (Conn::AwaitWelcome, RelayFrame::Welcome { .. }) => {
                self.conn = Conn::Open;
                out.global("relay", json!({ "connected": true }));
                self.subscribe(&mut out, now);
            }
            (Conn::AwaitChallenge | Conn::AwaitWelcome, RelayFrame::Error { detail, .. }) => {
                out.global("relay_error", json!({ "message": detail }));
            }
            (Conn::Open, f) => self.open_frame(f, now, &mut out),
            _ => {}
        }
        out
    }

    fn subscribe(&mut self, out: &mut Output, now: i64) {
        let Some(r) = &mut self.room else { return };
        out.frame(&ClientFrame::Subscribe {
            room: r.core.id,
            chain: Some(Box::new(r.core.my_chain.clone())),
            after: r.core.cursor,
        });
        r.subscribed = true;
        r.connected_at = Some(now);
        let fx = r.core.on_connected(now);
        apply(r, fx, out);
        let queued: Vec<Vec<u8>> = r.outbox.drain(..).collect();
        for envelope in queued {
            out.frame(&ClientFrame::Publish { envelope });
        }
        out.event(Some(r.id_hex()), json!({ "type": "connection", "connected": true }));
    }

    fn open_frame(&mut self, f: RelayFrame, now: i64, out: &mut Output) {
        match f {
            RelayFrame::JoinStarted { .. }
            | RelayFrame::Pake { .. }
            | RelayFrame::PakeEnded { .. }
            | RelayFrame::Error { .. }
                if self.join.is_some() =>
            {
                self.join_frame(f, now, out)
            }
            f => {
                let Some(r) = &mut self.room else { return };
                match f {
                    RelayFrame::Subscribed { writer, .. } => {
                        if !writer {
                            out.event(
                                Some(r.id_hex()),
                                json!({ "type": "error", "message": "the relay did not accept this device as a member" }),
                            );
                        }
                    }
                    RelayFrame::Deliver { room, cursor, envelope } if room == r.core.id => {
                        r.core.cursor = Some(cursor);
                        let fx = r.core.receive(&envelope, now);
                        apply(r, fx, out);
                    }
                    RelayFrame::CaughtUp { .. } => {
                        if r.needs_snapshot {
                            r.needs_snapshot = false;
                            out.frame(&ClientFrame::GetSnapshots { room: r.core.id });
                        }
                    }
                    RelayFrame::Gap { .. } => {
                        r.core.cursor = None;
                        out.frame(&ClientFrame::GetSnapshots { room: r.core.id });
                        let fx = r.core.sync_requests();
                        apply(r, fx, out);
                    }
                    RelayFrame::Snapshots { items, .. } => {
                        let me = self.identity.device_id();
                        let mut any = false;
                        for item in items.iter().filter(|i| i.from != me) {
                            any |= r.core.apply_relay_snapshot(&item.blob, now);
                        }
                        if any {
                            for d in [DocName::Board, DocName::Threads, DocName::Activity] {
                                event(r, &RoomEvent::DocChanged { doc: d }, out);
                            }
                            out.dirty = true;
                        }
                    }
                    RelayFrame::RoomEnded { .. } => event(r, &RoomEvent::RoomEnded, out),
                    RelayFrame::Error { code, detail } => {
                        out.event(
                            Some(r.id_hex()),
                            json!({ "type": "relay_error", "code": format!("{code:?}"), "message": detail }),
                        );
                    }
                    _ => {}
                }
            }
        }
    }

    fn join_frame(&mut self, f: RelayFrame, now: i64, out: &mut Output) {
        let Some(mut j) = self.join.take() else { return };
        let fail = |out: &mut Output, msg: &str| out.global("join_failed", json!({ "message": msg }));
        let wrong = "the code did not match; check it and try again";
        match (j.stage.take(), f) {
            (_, RelayFrame::Error { code, detail }) => {
                let msg = match code {
                    ErrorCode::CodeUnknown => {
                        "no room is waiting for this code; it may have expired or been used".to_string()
                    }
                    ErrorCode::CodeLocked => "this code is locked for 10 minutes after failed attempts".to_string(),
                    ErrorCode::RateLimited => {
                        "too many join attempts from this network; wait a few minutes".to_string()
                    }
                    _ => detail,
                };
                fail(out, &msg);
            }
            (_, RelayFrame::PakeEnded { session, .. }) if session == j.session => fail(out, wrong),
            (Some(JoinStage::Started), RelayFrame::JoinStarted { session }) => {
                let pake = PakeStart::new(&j.code, PakeRole::Joiner, &[]);
                out.frame(&ClientFrame::Pake { session, data: pake.message().to_vec() });
                j.session = session;
                j.stage = Some(JoinStage::Host(pake));
                self.join = Some(j);
            }
            (Some(JoinStage::Host(pake)), RelayFrame::Pake { session, data }) if session == j.session => {
                match pake.finish(&data) {
                    Ok(fin) => {
                        out.frame(&ClientFrame::Pake { session, data: fin.confirmation().to_vec() });
                        j.stage = Some(JoinStage::Tag(fin));
                        self.join = Some(j);
                    }
                    Err(_) => fail(out, wrong),
                }
            }
            (Some(JoinStage::Tag(fin)), RelayFrame::Pake { session, data }) if session == j.session => {
                let Ok(mut channel) = fin.verify_peer(&data) else { return fail(out, wrong) };
                let req = JoinRequest {
                    device: self.identity.public(),
                    member: self.member,
                    display_name: self.name.clone(),
                    name_salt: j.salt.to_vec(),
                    surface: "phone".into(),
                    link_device: false,
                };
                match to_cbor(&req).map_err(PhoneError::from).and_then(|b| Ok(channel.seal(&b)?)) {
                    Ok(body) => {
                        out.frame(&ClientFrame::Pake { session, data: body });
                        j.stage = Some(JoinStage::Reply(channel));
                        self.join = Some(j);
                    }
                    Err(e) => fail(out, &e.to_string()),
                }
            }
            (Some(JoinStage::Reply(mut channel)), RelayFrame::Pake { session, data }) if session == j.session => {
                let reply =
                    channel.open(&data).map_err(PhoneError::from).and_then(|pt| Ok(from_cbor::<JoinReply>(&pt)?));
                match reply {
                    Ok(JoinReply::Pending) => {
                        out.global("join_pending", json!({ "code": j.code.first_half() }));
                        j.stage = Some(JoinStage::Reply(channel));
                        self.join = Some(j);
                    }
                    Ok(JoinReply::Denied { reason }) => {
                        fail(out, &format!("the room declined the request: {reason}"));
                    }
                    Ok(JoinReply::Approved(a)) => {
                        match RoomCore::from_join(self.identity.clone(), &a, &self.name, j.salt, now) {
                            Ok(mut core) => {
                                core.surface = Surface::Phone;
                                if let Some(r) = a.relay.as_deref() {
                                    // A pinned LAN relay cannot be reached from a browser;
                                    // keep the one this phone already uses.
                                    if !r.contains("#sha256=") {
                                        self.relay = r.to_string();
                                    }
                                }
                                core.relay = Some(self.relay.clone());
                                let room = Room::new(core);
                                let id = room.id_hex();
                                let name = room.core.name.clone();
                                self.room = Some(room);
                                out.dirty = true;
                                out.global("joined", json!({ "room": id, "name": name, "relay": self.relay }));
                                // The relay ends the join session itself; subscribe now.
                                self.subscribe(out, now);
                            }
                            Err(e) => fail(out, &e.to_string()),
                        }
                    }
                    Err(e) => fail(out, &e.to_string()),
                }
            }
            (stage, _) => {
                // Frames for other sessions: keep waiting.
                j.stage = stage;
                self.join = Some(j);
            }
        }
    }

    /// Heartbeats, election, escalation and the voice hue.
    pub fn tick(&mut self, now: i64) -> Output {
        let mut out = Output::default();
        let open = self.conn == Conn::Open;
        let Some(r) = &mut self.room else { return out };
        let hue_pending = r.core.my_profile.hue == polyph_core::room::NO_HUE;
        if now - r.last_tick >= TICK_SECS || now < r.last_tick || hue_pending {
            r.last_tick = now;
            let fx = r.core.tick(now);
            apply(r, fx, &mut out);
            let settled = open && r.connected_at.is_some_and(|t| now - t >= HUE_SETTLE);
            if settled && hue_pending {
                let fx = r.core.set_hue_if_unset();
                apply(r, fx, &mut out);
            }
        }
        self.flush(&mut out);
        out
    }

    /// Runs one local API call. See [`api`].
    pub fn call(&mut self, method: &str, params: &Value, now: i64) -> Result<(Value, Output)> {
        let mut out = Output::default();
        let v = api::call(self, method, params, now, &mut out)?;
        self.flush(&mut out);
        Ok((v, out))
    }

    /// Moves sealed envelopes either onto the wire or into the outbox.
    fn flush(&mut self, out: &mut Output) {
        let open = self.conn == Conn::Open;
        let Some(r) = &mut self.room else { return };
        let mut keep = vec![];
        for f in out.frames.drain(..) {
            keep.push(f);
        }
        if open && r.subscribed {
            out.frames = keep;
            let queued: Vec<Vec<u8>> = r.outbox.drain(..).collect();
            for envelope in queued {
                out.frame(&ClientFrame::Publish { envelope });
            }
        } else {
            // Room messages wait for the connection; other frames are dropped.
            for f in keep {
                if let Ok(ClientFrame::Publish { envelope }) = ClientFrame::decode(&f) {
                    if r.outbox.len() >= OUTBOX_MAX {
                        r.outbox.pop_front();
                    }
                    r.outbox.push_back(envelope);
                }
            }
        }
    }
}

/// Carries out an engine's effects: seals messages and turns events into
/// page events (keeping agents' transcripts on the way).
pub(crate) fn apply(r: &mut Room, effects: Vec<Effect>, out: &mut Output) {
    for e in effects {
        match e {
            Effect::Send(m) => send(r, &m, out),
            Effect::Event(RoomEvent::Terminal { .. } | RoomEvent::Mirror { .. }) => {
                // Terminals and mirrored files are for daemons.
            }
            Effect::Event(ev) => {
                if let RoomEvent::Agent { event } = &ev {
                    let log = r.transcripts.entry(event.agent.id).or_default();
                    if !log.iter().any(|e| e.seq == event.seq) {
                        log.push_back((**event).clone());
                    }
                    while log.len() > TRANSCRIPT_KEEP {
                        log.pop_front();
                    }
                }
                event(r, &ev, out);
            }
        }
    }
    r.core.commit_rotation();
    out.dirty = true;
}

fn event(r: &Room, ev: &RoomEvent, out: &mut Output) {
    out.event(Some(r.id_hex()), serde_json::to_value(ev).unwrap_or(Value::Null));
}

fn send(r: &mut Room, m: &RoomMessage, out: &mut Output) {
    match r.core.seal(m) {
        Ok(env) => out.frame(&ClientFrame::Publish { envelope: env.encode() }),
        Err(e) => out.event(Some(r.id_hex()), json!({ "type": "error", "message": e.to_string() })),
    }
}
