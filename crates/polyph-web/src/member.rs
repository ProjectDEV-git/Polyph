//! A room member with no daemon: the phone PWA's engine. Sans-I/O like
//! `RoomCore`: the host page owns the relay WebSocket, storage and clock; it
//! feeds frames and calls in, and sends what comes out.
//!
//! - `connected()` after the socket opens, `on_frame()` for every binary
//!   frame, `tick()` every few seconds, `call()` for the app's API methods.
//! - `take()` returns the frames to send and the events for the app (the same
//!   `{room, event}` shape the daemon streams), so `packages/ui` runs unchanged.
//! - `save()` returns the room state for storage; `load()` restores it.

use std::collections::BTreeMap;
use std::sync::Arc;

use polyph_core::room::{Effect, RoomCore, RoomEvent, SavedRoom};
use polyph_proto::codec::{from_cbor, to_cbor};
use polyph_proto::crdt::{Board, NewCard};
use polyph_proto::identity::{DeviceIdentity, ctx};
use polyph_proto::ids::{MemberId, ObjectId};
use polyph_proto::join::{JoinReply, JoinRequest};
use polyph_proto::joincode::JoinCode;
use polyph_proto::messages::{AgentEvent, AgentEventKind, AgentState, ApprovalScope, Decision, DocName, Surface};
use polyph_proto::pake::{PakeFinished, PakeRole, PakeStart, SecureChannel};
use polyph_proto::relay_wire::{ClientFrame, ErrorCode, RelayFrame, auth_message};
use polyph_proto::version::PROTOCOL_VERSION;
use serde::Deserialize;
use serde_json::{Value, json};

/// Messages kept for a disconnected room, oldest dropped first.
const OUTBOX_MAX: usize = 500;
/// Transcript events kept per agent.
const TRANSCRIPT_KEEP: usize = 500;

#[derive(Debug, PartialEq, Eq)]
enum Auth {
    Closed,
    Challenge,
    Welcome,
    Ready,
}

enum Stage {
    Started,
    Host { session: u64, pake: PakeStart },
    Tag { session: u64, fin: PakeFinished },
    Reply { session: u64, channel: SecureChannel },
}

struct Join {
    code: JoinCode,
    link: Option<MemberId>,
    salt: [u8; 16],
    stage: Option<Stage>,
}

pub struct Member {
    identity: Arc<DeviceIdentity>,
    name: String,
    relay: String,
    auth: Auth,
    join: Option<Join>,
    pub room: Option<RoomCore>,
    subscribed: bool,
    needs_snapshot: bool,
    connected_at: Option<i64>,
    transcripts: BTreeMap<ObjectId, Vec<AgentEvent>>,
    outbox: Vec<Vec<u8>>,
    frames: Vec<Vec<u8>>,
    events: Vec<Value>,
    dirty: bool,
}

impl std::fmt::Debug for Member {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Member")
            .field("auth", &self.auth)
            .field("in_room", &self.room.is_some())
            .finish_non_exhaustive()
    }
}

/// What to send to the relay and what to tell the app.
#[derive(Debug, Default)]
pub struct Output {
    pub frames: Vec<Vec<u8>>,
    pub events: Vec<Value>,
    /// The room changed and should be saved.
    pub dirty: bool,
}

fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

fn param<T: for<'de> Deserialize<'de>>(v: &Value) -> Result<T, String> {
    serde_json::from_value(v.clone()).map_err(|e| format!("invalid parameters: {e}"))
}

impl Member {
    pub fn new(identity: DeviceIdentity, name: &str, relay: &str) -> Self {
        Self {
            identity: Arc::new(identity),
            name: name.to_string(),
            relay: relay.to_string(),
            auth: Auth::Closed,
            join: None,
            room: None,
            subscribed: false,
            needs_snapshot: true,
            connected_at: None,
            transcripts: BTreeMap::new(),
            outbox: vec![],
            frames: vec![],
            events: vec![],
            dirty: false,
        }
    }

    /// Restores a room saved with `save()`.
    pub fn load(identity: DeviceIdentity, name: &str, saved: &[u8], now: i64) -> Result<Self, String> {
        let s: SavedRoom = from_cbor(saved).map_err(err)?;
        let relay = s.relay.clone().unwrap_or_default();
        let mut m = Self::new(identity, name, &relay);
        let mut core = RoomCore::load(m.identity.clone(), s, now).map_err(err)?;
        core.surface = Surface::Phone;
        m.room = Some(core);
        Ok(m)
    }

    pub fn save(&mut self) -> Option<Vec<u8>> {
        let core = self.room.as_mut()?;
        to_cbor(&core.save()).ok()
    }

    pub fn device_id(&self) -> String {
        self.identity.device_id().to_hex()
    }

    pub fn relay(&self) -> &str {
        &self.relay
    }

    pub fn take_frames(&mut self) -> Vec<Vec<u8>> {
        std::mem::take(&mut self.frames)
    }

    pub fn take_events(&mut self) -> Vec<Value> {
        std::mem::take(&mut self.events)
    }

    pub fn take_dirty(&mut self) -> bool {
        std::mem::take(&mut self.dirty)
    }

    pub fn take(&mut self) -> Output {
        Output {
            frames: std::mem::take(&mut self.frames),
            events: std::mem::take(&mut self.events),
            dirty: std::mem::take(&mut self.dirty),
        }
    }

    fn send(&mut self, f: ClientFrame) {
        if let Ok(b) = f.encode() {
            self.frames.push(b);
        }
    }

    fn emit_global(&mut self, kind: &str, data: Value) {
        let mut ev = json!({ "type": kind });
        if let (Some(o), Value::Object(d)) = (ev.as_object_mut(), data) {
            o.extend(d);
        }
        self.events.push(json!({ "room": null, "event": ev }));
    }

    fn emit_room(&mut self, ev: &RoomEvent) {
        let Some(core) = &self.room else { return };
        let v = serde_json::to_value(ev).unwrap_or(Value::Null);
        self.events.push(json!({ "room": core.id.to_hex(), "event": v }));
    }

    /// Starts joining with a code, or linking this device to `member` with a
    /// link code. Runs once the relay connection is ready.
    pub fn start_join(&mut self, code: &str, link: Option<&str>) -> Result<(), String> {
        if self.room.is_some() {
            return Err("this device is already in a room".into());
        }
        let code = JoinCode::parse(code).map_err(|e| format!("{e}; codes look like A1B2-CDEF"))?;
        let link = match link {
            Some(h) => Some(MemberId::from_hex(h).map_err(|_| "member must be a member id".to_string())?),
            None => None,
        };
        let salt = ObjectId::random().map_err(err)?.0;
        self.join = Some(Join { code, link, salt, stage: None });
        if self.auth == Auth::Ready {
            self.send_join_code();
        }
        Ok(())
    }

    fn send_join_code(&mut self) {
        let Some(j) = self.join.as_mut() else { return };
        j.stage = Some(Stage::Started);
        let rendezvous = j.code.rendezvous();
        self.send(ClientFrame::JoinCode { rendezvous });
    }

    /// The WebSocket opened: introduce this device.
    pub fn connected(&mut self) {
        self.auth = Auth::Challenge;
        self.subscribed = false;
        let device = self.identity.device_id();
        self.send(ClientFrame::Hello { protocol: PROTOCOL_VERSION, device, client: "polyph-web/0.1".into() });
    }

    /// The WebSocket closed.
    pub fn disconnected(&mut self) {
        self.auth = Auth::Closed;
        self.subscribed = false;
        self.connected_at = None;
        if let Some(j) = self.join.as_mut() {
            j.stage = None;
        }
        self.emit_room_kind("connection", json!({ "connected": false }));
    }

    fn emit_room_kind(&mut self, kind: &str, data: Value) {
        let Some(core) = &self.room else { return };
        let room = core.id.to_hex();
        let mut ev = json!({ "type": kind });
        if let (Some(o), Value::Object(d)) = (ev.as_object_mut(), data) {
            o.extend(d);
        }
        self.events.push(json!({ "room": room, "event": ev }));
    }

    fn on_ready(&mut self, now: i64) {
        self.auth = Auth::Ready;
        if self.join.is_some() {
            self.send_join_code();
        }
        self.subscribe(now);
    }

    fn subscribe(&mut self, now: i64) {
        let Some(core) = self.room.as_mut() else { return };
        let f =
            ClientFrame::Subscribe { room: core.id, chain: Some(Box::new(core.my_chain.clone())), after: core.cursor };
        self.send(f);
        self.subscribed = true;
        self.connected_at = Some(now);
        let fx = match self.room.as_mut() {
            Some(c) => c.on_connected(now),
            None => vec![],
        };
        self.apply(fx);
        for env in std::mem::take(&mut self.outbox) {
            self.send(ClientFrame::Publish { envelope: env });
        }
        self.emit_room_kind("connection", json!({ "connected": true }));
    }

    pub fn on_frame(&mut self, bytes: &[u8], now: i64) {
        let Ok(f) = RelayFrame::decode(bytes) else { return };
        match f {
            RelayFrame::Challenge { nonce, .. } if self.auth == Auth::Challenge => {
                let sig = self.identity.sign(ctx::RELAY_AUTH, &auth_message(&nonce, &self.identity.device_id()));
                self.auth = Auth::Welcome;
                self.send(ClientFrame::Auth { sig });
            }
            RelayFrame::Welcome { device } if self.auth == Auth::Welcome && device == self.identity.device_id() => {
                self.on_ready(now)
            }
            RelayFrame::JoinStarted { session } => self.join_started(session),
            RelayFrame::Pake { session, data } => self.join_step(session, &data, now),
            RelayFrame::PakeEnded { .. } if self.room.is_none() => {
                if self.join.as_ref().is_some_and(|j| matches!(j.stage, Some(Stage::Host { .. } | Stage::Tag { .. }))) {
                    self.fail_join("the code did not match; check it and try again");
                }
            }
            RelayFrame::Deliver { room, cursor, envelope } => {
                let Some(core) = self.room.as_mut() else { return };
                if core.id != room {
                    return;
                }
                core.cursor = Some(cursor);
                let fx = core.receive(&envelope, now);
                self.dirty = true;
                self.apply(fx);
            }
            RelayFrame::CaughtUp { .. } => {
                if self.needs_snapshot
                    && let Some(core) = &self.room
                {
                    self.needs_snapshot = false;
                    let room = core.id;
                    self.send(ClientFrame::GetSnapshots { room });
                }
            }
            RelayFrame::Gap { .. } => {
                let Some(core) = self.room.as_mut() else { return };
                core.cursor = None;
                let room = core.id;
                let fx = core.sync_requests();
                self.send(ClientFrame::GetSnapshots { room });
                self.apply(fx);
            }
            RelayFrame::Snapshots { items, .. } => {
                let me = self.identity.device_id();
                let Some(core) = self.room.as_mut() else { return };
                let mut any = false;
                for item in items.iter().filter(|i| i.from != me) {
                    any |= core.apply_relay_snapshot(&item.blob, now);
                }
                if any {
                    self.dirty = true;
                    for d in [DocName::Board, DocName::Threads] {
                        self.emit_room(&RoomEvent::DocChanged { doc: d });
                    }
                }
            }
            RelayFrame::RoomEnded { .. } => self.emit_room(&RoomEvent::RoomEnded),
            RelayFrame::Error { code, detail } => {
                if self.room.is_none() && self.join.is_some() {
                    let msg = match code {
                        ErrorCode::CodeUnknown => "no room is waiting for this code; it may have expired or been used",
                        ErrorCode::CodeLocked => "this code is locked for 10 minutes after failed attempts",
                        ErrorCode::RateLimited => "too many join attempts from this network; wait a few minutes",
                        _ => detail.as_str(),
                    }
                    .to_string();
                    self.fail_join(&msg);
                } else {
                    self.emit_global("error", json!({ "message": detail }));
                }
            }
            _ => {}
        }
    }

    fn fail_join(&mut self, message: &str) {
        self.join = None;
        self.emit_global("join_failed", json!({ "message": message }));
    }

    fn join_started(&mut self, session: u64) {
        let Some(j) = self.join.as_mut() else { return };
        if !matches!(j.stage, Some(Stage::Started)) {
            return;
        }
        // Browsers cannot pin a LAN relay's certificate: joins here use public relays only.
        let pake = PakeStart::new(&j.code, PakeRole::Joiner, &[]);
        let data = pake.message().to_vec();
        j.stage = Some(Stage::Host { session, pake });
        self.send(ClientFrame::Pake { session, data });
    }

    fn join_step(&mut self, session: u64, data: &[u8], now: i64) {
        let Some(mut j) = self.join.take() else { return };
        let stage = j.stage.take();
        match stage {
            Some(Stage::Host { session: s, pake }) if s == session => match pake.finish(data) {
                Ok(fin) => {
                    let c = fin.confirmation().to_vec();
                    j.stage = Some(Stage::Tag { session, fin });
                    self.join = Some(j);
                    self.send(ClientFrame::Pake { session, data: c });
                }
                Err(_) => self.fail_join("the code did not match; check it and try again"),
            },
            Some(Stage::Tag { session: s, fin }) if s == session => match fin.verify_peer(data) {
                Ok(mut channel) => {
                    let member = match j.link {
                        Some(m) => m,
                        None => match MemberId::random() {
                            Ok(m) => m,
                            Err(e) => return self.fail_join(&e.to_string()),
                        },
                    };
                    let req = JoinRequest {
                        device: self.identity.public(),
                        member,
                        display_name: self.name.clone(),
                        name_salt: j.salt.to_vec(),
                        surface: "phone".into(),
                        link_device: j.link.is_some(),
                    };
                    let body = to_cbor(&req).map_err(err).and_then(|b| channel.seal(&b).map_err(err));
                    match body {
                        Ok(b) => {
                            j.stage = Some(Stage::Reply { session, channel });
                            self.join = Some(j);
                            self.send(ClientFrame::Pake { session, data: b });
                        }
                        Err(e) => self.fail_join(&e),
                    }
                }
                Err(_) => self.fail_join("the code did not match; check it and try again"),
            },
            Some(Stage::Reply { session: s, mut channel }) if s == session => {
                let reply = channel.open(data).map_err(err).and_then(|pt| from_cbor::<JoinReply>(&pt).map_err(err));
                match reply {
                    Ok(JoinReply::Pending) => {
                        j.stage = Some(Stage::Reply { session, channel });
                        self.join = Some(j);
                        self.emit_global("join_pending", json!({ "code": j_first_half(&self.join) }));
                    }
                    Ok(JoinReply::Denied { reason }) => {
                        self.fail_join(&format!("the room declined the request: {reason}"))
                    }
                    Ok(JoinReply::Approved(a)) => {
                        match RoomCore::from_join(self.identity.clone(), &a, &self.name, j.salt, now) {
                            Ok(mut core) => {
                                core.surface = Surface::Phone;
                                core.relay = Some(a.relay.clone().unwrap_or_else(|| self.relay.clone()));
                                let room = core.id.to_hex();
                                self.room = Some(core);
                                self.dirty = true;
                                self.emit_global("joined", json!({ "room": room }));
                                if self.auth == Auth::Ready {
                                    self.subscribe(now);
                                }
                            }
                            Err(e) => self.fail_join(&e.to_string()),
                        }
                    }
                    Err(e) => self.fail_join(&e),
                }
            }
            other => {
                j.stage = other;
                self.join = Some(j);
            }
        }
    }

    /// Periodic work: heartbeats, elections, picking a voice hue.
    pub fn tick(&mut self, now: i64) {
        let Some(core) = self.room.as_mut() else { return };
        let mut fx = core.tick(now);
        let settled = self.connected_at.is_some_and(|t| now - t >= 2);
        if settled && core.my_profile.hue == polyph_core::room::NO_HUE {
            fx.extend(core.set_hue_if_unset());
        }
        self.apply(fx);
    }

    fn apply(&mut self, effects: Vec<Effect>) {
        for e in effects {
            match e {
                Effect::Send(m) => {
                    let Some(core) = self.room.as_mut() else { continue };
                    let Ok(env) = core.seal(&m) else { continue };
                    let env = env.encode();
                    if self.subscribed {
                        self.send(ClientFrame::Publish { envelope: env });
                    } else {
                        if self.outbox.len() >= OUTBOX_MAX {
                            self.outbox.remove(0);
                        }
                        self.outbox.push(env);
                    }
                    self.dirty = true;
                }
                // Terminal bytes and mirrored files are for daemons.
                Effect::Event(RoomEvent::Terminal { .. } | RoomEvent::Mirror { .. }) => {}
                Effect::Event(ev) => {
                    if let RoomEvent::Agent { event } = &ev {
                        let log = self.transcripts.entry(event.agent.id).or_default();
                        log.push((**event).clone());
                        if log.len() > TRANSCRIPT_KEEP {
                            log.remove(0);
                        }
                    }
                    self.emit_room(&ev);
                }
            }
        }
    }

    fn room_info(&self, now: i64) -> Option<Value> {
        let core = self.room.as_ref()?;
        Some(json!({
            "id": core.id.to_hex(),
            "name": core.name,
            "quick": core.quick,
            "role": core.role(),
            "relay": self.relay,
            "connected": self.subscribed,
            "open": true,
            "members_online": polyph_core::views::members_online(core, now),
            "members": polyph_core::views::member_count(core),
            "epoch": core.epoch(),
            "lan": null,
        }))
    }

    fn agent_state(log: &[AgentEvent]) -> Option<AgentState> {
        log.iter().rev().find_map(|e| match &e.kind {
            AgentEventKind::State { state, .. } => Some(*state),
            AgentEventKind::Stopped { .. } => Some(AgentState::Stopped),
            _ => None,
        })
    }

    /// The app's API, answered locally: the subset that makes sense on a
    /// device that runs no agents and opens no terminals.
    pub fn call(&mut self, method: &str, params: &Value, now: i64) -> Result<Value, String> {
        match method {
            "ping" => return Ok(json!({ "protocol": PROTOCOL_VERSION })),
            "whoami" => {
                let member = self.room.as_ref().map(|c| c.my_member.to_hex());
                return Ok(
                    json!({ "name": self.name, "member": member, "device": self.device_id(), "relay": self.relay, "keystore": "browser" }),
                );
            }
            "room.list" => return Ok(Value::Array(self.room_info(now).into_iter().collect())),
            "status" => {
                let Some(core) = self.room.as_ref() else {
                    return Ok(
                        json!({ "room": null, "room_name": null, "connected": false, "members_online": 0, "agents": [], "approvals": [], "budgets": [] }),
                    );
                };
                let approvals: Vec<Value> = core
                    .approvals
                    .values()
                    .map(|a| json!({ "id": a.id.to_hex(), "agent": a.agent.label, "title": "", "created_at": a.created_at, "mine": a.agent.member == core.my_member }))
                    .collect();
                let agents: Vec<Value> = self
                    .transcripts
                    .iter()
                    .filter_map(|(id, log)| {
                        let first = log.first()?;
                        let state = Self::agent_state(log)?;
                        (state != AgentState::Stopped).then(|| json!({ "id": id.to_hex(), "label": first.agent.label, "owner": "", "color": "", "state": state, "task": null, "mine": false, "cloud": false }))
                    })
                    .collect();
                return Ok(
                    json!({ "room": core.id.to_hex(), "room_name": core.name, "connected": self.subscribed, "members_online": polyph_core::views::members_online(core, now), "agents": agents, "approvals": approvals, "budgets": [] }),
                );
            }
            _ => {}
        }
        if method == "join" {
            #[derive(Deserialize)]
            struct P {
                code: String,
                member: Option<String>,
            }
            let p: P = param(params)?;
            self.start_join(&p.code, p.member.as_deref())?;
            return Ok(json!({ "pending": true }));
        }
        let Some(core) = self.room.as_mut() else { return Err("this device is not in a room yet".into()) };
        if let Some(r) = params.get("room").and_then(Value::as_str)
            && r != core.id.to_hex()
        {
            return Err("no such room on this device".into());
        }
        let out = match method {
            "room.get" | "room.open" => return self.room_info(now).ok_or_else(|| "no room".into()),
            "room.members" => serde_json::to_value(polyph_core::views::members(core, now)).map_err(err)?,
            "chat.list" => {
                let limit = params.get("limit").and_then(Value::as_u64).map(|l| l as usize);
                Value::Array(polyph_core::views::chat(core, limit))
            }
            "thread.list" => Value::Array(polyph_core::views::threads(core)),
            "board.get" => polyph_core::views::board(core),
            "activity.list" => {
                let limit = params.get("limit").and_then(Value::as_u64).map(|l| l as usize);
                serde_json::to_value(polyph_core::views::activity(core, None, &[], limit)).map_err(err)?
            }
            "activity.replay" => {
                let at = params.get("at").and_then(Value::as_i64).ok_or("missing at")?;
                let entries = polyph_proto::crdt::Activity::new(core.doc(&DocName::Activity)).entries();
                serde_json::to_value(polyph_core::activity::replay(&entries, at)).map_err(err)?
            }
            "merge.queue" => {
                serde_json::to_value(polyph_core::docs::merge_entries(core.doc(&DocName::Board))).map_err(err)?
            }
            "approvals.list" => {
                serde_json::to_value(core.approvals.values().cloned().collect::<Vec<_>>()).map_err(err)?
            }
            "knock.list" => serde_json::to_value(core.knocks.values().cloned().collect::<Vec<_>>()).map_err(err)?,
            "whiteboard.list" => serde_json::to_value(core.whiteboards()).map_err(err)?,
            "term.list" => json!([]),
            "integrations.status" => json!({ "github": null, "linear": null }),
            "agent.list" => {
                let mut out = vec![];
                for (id, log) in &self.transcripts {
                    let Some(first) = log.first() else { continue };
                    out.push(json!({ "id": id.to_hex(), "label": first.agent.label, "kind": first.agent.kind, "member": first.agent.member.to_hex(), "mine": false, "state": Self::agent_state(log) }));
                }
                Value::Array(out)
            }
            "agent.transcript" => {
                let want = params.get("agent").and_then(Value::as_str).unwrap_or_default();
                let events = self
                    .transcripts
                    .iter()
                    .find(|(id, _)| id.to_hex() == want)
                    .map(|(_, l)| l.clone())
                    .unwrap_or_default();
                json!({ "events": events, "summary": [] })
            }
            _ => return self.act(method, params, now),
        };
        Ok(out)
    }

    fn act(&mut self, method: &str, params: &Value, now: i64) -> Result<Value, String> {
        let Some(core) = self.room.as_mut() else { return Err("this device is not in a room yet".into()) };
        let (fx, out) = match method {
            "chat.send" => {
                #[derive(Deserialize)]
                struct P {
                    text: String,
                    thread: Option<String>,
                }
                let p: P = param(params)?;
                if p.text.trim().is_empty() {
                    return Err("message is empty".into());
                }
                (core.send_chat(&p.text, p.thread, None, now).map_err(err)?, json!({}))
            }
            "board.add" => {
                #[derive(Deserialize)]
                struct P {
                    title: String,
                    column: Option<String>,
                }
                let p: P = param(params)?;
                let id = ObjectId::random().map_err(err)?.to_hex();
                let card = NewCard {
                    id: id.clone(),
                    title: polyph_core::redact::redact(&p.title).into_owned(),
                    column: p.column.unwrap_or_else(|| "backlog".into()),
                    created_by: core.my_member.to_hex(),
                    created_at: now,
                    ..Default::default()
                };
                let fx = core
                    .change_doc(DocName::Board, |d| {
                        let _ = Board::new(d).add_card(&card);
                    })
                    .map_err(err)?;
                (fx, json!({ "id": id }))
            }
            "board.move" => {
                #[derive(Deserialize)]
                struct P {
                    card: String,
                    column: String,
                    after: Option<String>,
                }
                let p: P = param(params)?;
                let mut res = Ok(());
                let fx = core
                    .change_doc(DocName::Board, |d| {
                        res = Board::new(d).move_card(&p.card, &p.column, p.after.as_deref(), now)
                    })
                    .map_err(err)?;
                res.map_err(err)?;
                (fx, json!({}))
            }
            "approve" | "deny" => {
                #[derive(Deserialize)]
                struct P {
                    id: String,
                    #[serde(default)]
                    task: bool,
                    note: Option<String>,
                }
                let p: P = param(params)?;
                let id = core
                    .approvals
                    .keys()
                    .find(|k| k.to_hex().starts_with(&p.id.to_lowercase()))
                    .copied()
                    .ok_or("no such approval")?;
                let decision = if method == "approve" {
                    Decision::Approve { scope: if p.task { ApprovalScope::Task } else { ApprovalScope::Once } }
                } else {
                    Decision::Deny { note: p.note }
                };
                (core.answer_approval(id, decision, now).map_err(err)?, json!({}))
            }
            "knock.decide" => {
                #[derive(Deserialize)]
                struct P {
                    knock: String,
                    approve: bool,
                }
                let p: P = param(params)?;
                let id =
                    core.knocks.keys().find(|k| k.to_hex().starts_with(&p.knock)).copied().ok_or("no such knock")?;
                let fx = if p.approve {
                    core.approve_knock(id, polyph_proto::cert::Role::Member, now).map_err(err)?
                } else {
                    core.deny_knock(id).map_err(err)?
                };
                (fx, json!({}))
            }
            "inbox.send" => {
                #[derive(Deserialize)]
                struct P {
                    agent: String,
                    text: String,
                }
                let p: P = param(params)?;
                let to = self
                    .transcripts
                    .values()
                    .filter_map(|l| l.first().map(|e| e.agent.clone()))
                    .find(|a| a.id.to_hex().starts_with(&p.agent))
                    .ok_or("no such agent")?;
                let core = self.room.as_mut().ok_or("no room")?;
                let (m, fx) = core.send_inbox(to, None, None, None, &p.text, 0, now).map_err(err)?;
                (fx, json!({ "id": m.id.to_hex() }))
            }
            _ => return Err(format!("{method} is not available on this device")),
        };
        self.dirty = true;
        self.apply(fx);
        Ok(out)
    }
}

fn j_first_half(j: &Option<Join>) -> String {
    j.as_ref().map(|j| j.code.first_half().to_string()).unwrap_or_default()
}
