//! The room engine: membership, keys, shared documents, election, claims,
//! knocks and approvals for one room on one device. It performs no I/O.
//! The daemon feeds it envelopes and ticks and carries out its effects.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::sync::Arc;

use polyph_proto::cert::{CertChain, MembershipCert, Role, RoomGenesis, VerifiedMember, name_commitment};
use polyph_proto::codec::{from_cbor, to_cbor};
use polyph_proto::crdt::{apply_update, diff_since, encode_full, state_vector};
use polyph_proto::delivery::{EpochKeyDelivery, open_epoch_key, seal_epoch_key};
use polyph_proto::envelope::Envelope;
use polyph_proto::group::{EpochKey, Keyring};
use polyph_proto::identity::{DeviceIdentity, DevicePublic};
use polyph_proto::ids::{DeviceId, MemberId, ObjectId, RoomId};
use polyph_proto::join::JoinApproved;
use polyph_proto::messages::{
    AgentRef, ApprovalKind, ApprovalMsg, Claim, ClaimMsg, ClaimTarget, Decision, DocName, Knock, Presence, Profile,
    RoomMessage, Surface,
};
use polyph_proto::signed::Signed;
use serde::{Deserialize, Serialize};
use tracing::debug;
use yrs::Doc;

use crate::claims::ClaimTable;
use crate::docs::{self, ChatEntry};
use crate::election::{Candidate, HEARTBEAT, ONLINE_WINDOW, top};
use crate::voice::assign_hue;

/// Default claim lease in seconds.
pub const DEFAULT_LEASE: u32 = 600;
/// Default time before an approval escalates beyond the agent's owner.
pub const APPROVAL_ESCALATION_SECS: i64 = 120;
/// How often the acting host broadcasts the full claim table.
const TABLE_INTERVAL: i64 = 30;
/// Hue not chosen yet.
pub const NO_HUE: u16 = u16::MAX;
/// Longest inbox message accepted, in bytes.
pub const MAX_INBOX_TEXT: usize = 16 * 1024;

/// A directed message to an agent (from a person or another agent).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct InboxMessage {
    pub id: ObjectId,
    pub to: AgentRef,
    pub from_member: MemberId,
    pub from_agent: Option<AgentRef>,
    pub thread: Option<ObjectId>,
    pub reply_to: Option<ObjectId>,
    pub text: String,
    pub at: i64,
    pub hops: u8,
}

impl InboxMessage {
    fn into_wire(self) -> RoomMessage {
        RoomMessage::Inbox {
            id: self.id,
            to: self.to,
            from_member: self.from_member,
            from_agent: self.from_agent,
            thread: self.thread,
            reply_to: self.reply_to,
            text: self.text,
            at: self.at,
            hops: self.hops,
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum RoomError {
    #[error("{0}")]
    Proto(#[from] polyph_proto::ProtoError),
    #[error("your role does not allow this")]
    Forbidden,
    #[error("no such {0}")]
    NotFound(&'static str),
    #[error("{0}")]
    Invalid(&'static str),
}

pub type Result<T> = std::result::Result<T, RoomError>;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Peer {
    pub chain: CertChain,
    #[serde(skip)]
    pub verified: Option<VerifiedMember>,
    pub profile: Option<Profile>,
    pub presence: Option<Presence>,
    pub last_seen: i64,
}

impl Peer {
    pub fn role(&self) -> Role {
        self.verified.as_ref().map(|v| v.role).unwrap_or(Role::Viewer)
    }
    pub fn member(&self) -> Option<MemberId> {
        self.verified.as_ref().map(|v| v.member)
    }
    pub fn joined_at(&self) -> i64 {
        self.chain.certs.last().and_then(|c| c.open().ok()).map(|c| c.issued_at).unwrap_or(i64::MAX)
    }
    pub fn is_daemon(&self) -> bool {
        !matches!(self.presence.as_ref().map(|p| p.surface), Some(Surface::Browser | Surface::Phone))
    }
    pub fn online(&self, now: i64) -> bool {
        self.presence.as_ref().is_none_or(|p| p.online) && now - self.last_seen <= ONLINE_WINDOW
    }
    pub fn name(&self) -> String {
        self.profile.as_ref().map(|p| p.display_name.clone()).unwrap_or_default()
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct HostState {
    pub term: u64,
    pub device: Option<DeviceId>,
    pub last_seen: i64,
    pub last_table: i64,
    pub last_announce: i64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PendingApproval {
    pub id: ObjectId,
    pub agent: AgentRef,
    pub kind: ApprovalKind,
    pub task: Option<ObjectId>,
    pub created_at: i64,
    pub escalate_at: i64,
}

/// Events for local clients (CLI, TUI, app). Serialized as JSON on the local API.
#[derive(Clone, Debug, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
#[allow(clippy::large_enum_variant)]
pub enum RoomEvent {
    MemberOnline {
        member: MemberId,
        device: DeviceId,
        name: String,
        role: Role,
        hue: u16,
    },
    MemberOffline {
        member: MemberId,
        device: DeviceId,
    },
    MemberJoined {
        member: MemberId,
        name: String,
        role: Role,
    },
    MemberRemoved {
        member: MemberId,
    },
    YouWereRemoved,
    Chat(ChatEntry),
    Knock {
        knock: Knock,
    },
    KnockDecided {
        id: ObjectId,
        approved: bool,
    },
    /// The device that ran the join handshake should now send this to the joiner.
    JoinReady {
        knock: ObjectId,
        approved: Box<JoinApproved>,
    },
    JoinDenied {
        knock: ObjectId,
    },
    KeysRotated {
        epoch: u64,
    },
    ActingHost {
        device: DeviceId,
        term: u64,
        me: bool,
    },
    ClaimDecision {
        request: ObjectId,
        granted: Vec<Claim>,
        denied: Vec<Claim>,
    },
    ClaimsChanged {
        count: usize,
    },
    ClaimRevoked {
        claim: Claim,
        winner: Claim,
    },
    DocChanged {
        doc: DocName,
    },
    ApprovalPending {
        approval: PendingApproval,
    },
    ApprovalResolved {
        id: ObjectId,
        decision: Decision,
        by: MemberId,
        for_me: bool,
    },
    /// Shared terminals, for the daemon's terminal module.
    Terminal {
        from: DeviceId,
        member: MemberId,
        msg: Box<polyph_proto::messages::TerminalMsg>,
    },
    /// Hosted-mode mirroring, for the daemon's mirror module.
    Mirror {
        from: DeviceId,
        msg: Box<polyph_proto::messages::MirrorMsg>,
    },
    /// A message to an agent. `for_me` when the agent runs on this device.
    Inbox {
        message: InboxMessage,
        for_me: bool,
    },
    /// A transcript event from an agent (ours or a teammate's).
    Agent {
        event: Box<polyph_proto::messages::AgentEvent>,
    },
    RoomEnded,
}

#[derive(Clone, Debug)]
pub enum Effect {
    /// Seal and publish to the room.
    Send(RoomMessage),
    Event(RoomEvent),
}

/// Everything a device persists about a room (encrypted at rest by the daemon).
#[derive(Serialize, Deserialize)]
pub struct SavedRoom {
    pub id: RoomId,
    pub name: String,
    pub quick: bool,
    pub my_member: MemberId,
    pub my_chain: CertChain,
    pub my_profile: Profile,
    pub keys: Vec<(u64, [u8; 32])>,
    pub seq: u64,
    pub peers: Vec<Peer>,
    pub revoked: Vec<DeviceId>,
    pub host: HostState,
    pub claims: Vec<Claim>,
    pub docs: Vec<(DocName, Vec<u8>)>,
    pub cursor: Option<u64>,
    pub relay: Option<String>,
    pub created_at: i64,
}

impl std::fmt::Debug for SavedRoom {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "SavedRoom({}, {} keys, {} peers)", self.id.short(), self.keys.len(), self.peers.len())
    }
}

/// What a member uploads to the relay as an encrypted snapshot.
#[derive(Serialize, Deserialize)]
struct RelaySnapshot {
    name: String,
    members: Vec<(CertChain, Option<Profile>)>,
    revoked: Vec<DeviceId>,
    docs: Vec<(DocName, Vec<u8>)>,
    claims: Vec<Claim>,
    term: u64,
}

pub struct RoomCore {
    pub id: RoomId,
    pub name: String,
    pub quick: bool,
    identity: Arc<DeviceIdentity>,
    pub me: DeviceId,
    pub my_member: MemberId,
    pub my_chain: CertChain,
    pub my_verified: VerifiedMember,
    pub my_profile: Profile,
    pub surface: Surface,
    keyring: Keyring,
    keys: BTreeMap<u64, [u8; 32]>,
    seq: u64,
    pub peers: BTreeMap<DeviceId, Peer>,
    pub revoked: BTreeSet<DeviceId>,
    pub host: HostState,
    pub claims: ClaimTable,
    docs: HashMap<DocName, Doc>,
    pub knocks: BTreeMap<ObjectId, Knock>,
    pending_deliveries: HashMap<DeviceId, Signed<EpochKeyDelivery>>,
    pub approvals: BTreeMap<ObjectId, PendingApproval>,
    pub cursor: Option<u64>,
    pub relay: Option<String>,
    pub created_at: i64,
    last_sync_reply: i64,
    online_seen: BTreeSet<DeviceId>,
    /// Next epoch, installed by `commit_rotation` once the announcement is sealed.
    pending_rotation: Option<EpochKey>,
    /// Latest time seen (ticks, messages), for activity entries.
    pub last_now: i64,
    /// The embedder's clock, when it has one (the daemon's).
    pub clock: Option<Arc<dyn Fn() -> i64 + Send + Sync>>,
}

impl std::fmt::Debug for RoomCore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "RoomCore({}, {} peers)", self.id.short(), self.peers.len())
    }
}

fn verify_profile(profile: &Profile, v: &VerifiedMember) -> bool {
    let Ok(salt) = <[u8; 16]>::try_from(profile.name_salt.as_slice()) else { return false };
    profile.member == v.member && name_commitment(&profile.display_name, &salt) == v.name_commit
}

fn new_salt() -> Result<[u8; 16]> {
    Ok(ObjectId::random()?.0)
}

impl RoomCore {
    /// Creates a room owned by this device.
    pub fn create(
        identity: Arc<DeviceIdentity>,
        member: MemberId,
        name: &str,
        display_name: &str,
        quick: bool,
        now: i64,
    ) -> Result<(Self, Signed<RoomGenesis>)> {
        let (genesis, room) = RoomGenesis::create(&identity, member, now, quick)?;
        let salt = new_salt()?;
        let chain =
            CertChain::for_owner(&identity, genesis.clone(), member, name_commitment(display_name, &salt), now)?;
        let profile =
            Profile { member, display_name: display_name.to_string(), name_salt: salt.to_vec(), hue: assign_hue(&[]) };
        let key = EpochKey::generate(room, 1)?;
        let mut core = Self::assemble(identity, chain, profile, name.to_string(), quick, now)?;
        core.add_key(key);
        debug_assert_eq!(core.id, room);
        docs::set_setting(core.doc(&DocName::Settings), "require_approval", &serde_json::json!(true));
        docs::set_setting(core.doc(&DocName::Settings), "name", &serde_json::json!(name));
        polyph_proto::crdt::Board::new(core.doc(&DocName::Board))
            .init_columns(&polyph_proto::crdt::board::DEFAULT_COLUMNS);
        Ok((core, genesis))
    }

    /// Builds the room on the joining device from the approval it received.
    pub fn from_join(
        identity: Arc<DeviceIdentity>,
        approved: &JoinApproved,
        display_name: &str,
        salt: [u8; 16],
        now: i64,
    ) -> Result<Self> {
        let v = approved.chain.verify(now, &|_| false)?;
        if v.device.sign != identity.device_id() {
            return Err(RoomError::Invalid("certificate is for another device"));
        }
        let key = open_epoch_key(&identity, &approved.key)?;
        let profile =
            Profile { member: v.member, display_name: display_name.to_string(), name_salt: salt.to_vec(), hue: NO_HUE };
        let mut core = Self::assemble(
            identity,
            approved.chain.clone(),
            profile,
            approved.room_name.clone().unwrap_or_default(),
            false,
            now,
        )?;
        core.add_key(key);
        core.relay = approved.relay.clone();
        Ok(core)
    }

    fn assemble(
        identity: Arc<DeviceIdentity>,
        chain: CertChain,
        profile: Profile,
        name: String,
        quick: bool,
        now: i64,
    ) -> Result<Self> {
        let v = chain.verify(now, &|_| false)?;
        let me = identity.device_id();
        let mut docs = HashMap::new();
        for d in [DocName::Board, DocName::Threads, DocName::Activity, DocName::Memory, DocName::Settings] {
            docs.insert(d, Doc::new());
        }
        Ok(Self {
            id: v.room,
            name,
            quick,
            identity,
            me,
            my_member: v.member,
            my_chain: chain,
            my_verified: v,
            my_profile: profile,
            surface: Surface::Cli,
            keyring: Keyring::default(),
            keys: BTreeMap::new(),
            seq: 0,
            peers: BTreeMap::new(),
            revoked: BTreeSet::new(),
            host: HostState::default(),
            claims: ClaimTable::default(),
            docs,
            knocks: BTreeMap::new(),
            pending_deliveries: HashMap::new(),
            approvals: BTreeMap::new(),
            cursor: None,
            relay: None,
            created_at: now,
            last_sync_reply: 0,
            online_seen: BTreeSet::new(),
            pending_rotation: None,
            last_now: now,
            clock: None,
        })
    }

    fn add_key(&mut self, k: EpochKey) {
        self.keys.insert(k.epoch, *k.secret_bytes());
        self.keyring.insert(k);
    }

    pub fn role(&self) -> Role {
        self.my_verified.role
    }

    pub fn epoch(&self) -> u64 {
        self.keyring.current.unwrap_or(0)
    }

    pub fn current_key(&self) -> Option<&EpochKey> {
        self.keyring.current_key()
    }

    pub fn identity(&self) -> &DeviceIdentity {
        &self.identity
    }

    pub fn doc(&mut self, name: &DocName) -> &Doc {
        self.docs.entry(name.clone()).or_default()
    }

    pub fn is_acting_host(&self) -> bool {
        self.host.device == Some(self.me)
    }

    // ---------- envelopes ----------

    pub fn seal(&mut self, msg: &RoomMessage) -> Result<Envelope> {
        let key = self.keyring.current_key().ok_or(RoomError::Invalid("no room key"))?;
        self.seq += 1;
        Ok(key.seal(&self.identity, self.seq, &msg.encode()?)?)
    }

    pub fn hello(&self, now: i64) -> RoomMessage {
        RoomMessage::Hello {
            chain: Box::new(self.my_chain.clone()),
            profile: self.my_profile.clone(),
            presence: Presence { surface: self.surface, online: true, viewing: None, at: now },
        }
    }

    /// Messages to send right after (re)connecting to the relay.
    pub fn on_connected(&mut self, now: i64) -> Vec<Effect> {
        // Whatever we believed while disconnected may be stale; re-run the
        // election from fresh heartbeats.
        let was_host = self.is_acting_host();
        if was_host {
            self.host.device = None;
        }
        let mut out = vec![Effect::Send(self.hello(now))];
        if was_host {
            // Claims granted while we were cut off must reach whoever is host
            // now so the split can be reconciled.
            out.push(Effect::Send(RoomMessage::Claim(ClaimMsg::Table {
                term: self.host.term,
                claims: self.claims.all(),
            })));
        }
        out.extend(self.sync_requests());
        // Re-run the election now rather than waiting a heartbeat; a better
        // candidate that is online will reassert and win.
        out.extend(self.elect(now));
        out
    }

    pub fn sync_requests(&mut self) -> Vec<Effect> {
        let names: Vec<DocName> = self.docs.keys().cloned().collect();
        names
            .into_iter()
            .map(|d| {
                let sv = state_vector(self.doc(&d));
                Effect::Send(RoomMessage::CrdtSyncRequest { doc: d, state_vector: sv })
            })
            .collect()
    }

    /// Opens and processes one envelope from the relay.
    pub fn receive(&mut self, bytes: &[u8], now: i64) -> Vec<Effect> {
        self.last_now = self.last_now.max(now);
        let env = match Envelope::decode(bytes) {
            Ok(e) => e,
            Err(_) => return vec![],
        };
        if env.room != self.id || env.sender == self.me || self.revoked.contains(&env.sender) {
            return vec![];
        }
        let pt = match self.keyring.open(&env) {
            Ok(pt) => pt,
            Err(e) => {
                debug!(error = %e, "dropped envelope");
                return vec![];
            }
        };
        let msg = match RoomMessage::decode(&pt) {
            Ok(m) => m,
            Err(_) => return vec![],
        };
        self.handle(env.sender, msg, now)
    }

    fn sender_role(&self, d: &DeviceId) -> Option<Role> {
        self.peers.get(d).and_then(|p| p.verified.as_ref()).map(|v| v.role)
    }

    fn sender_member(&self, d: &DeviceId) -> Option<MemberId> {
        self.peers.get(d).and_then(|p| p.member())
    }

    fn handle(&mut self, from: DeviceId, msg: RoomMessage, now: i64) -> Vec<Effect> {
        if let RoomMessage::Hello { chain, profile, presence } = msg {
            return self.on_hello(from, *chain, profile, presence, now);
        }
        // Everything else needs a known, verified sender.
        let Some(role) = self.sender_role(&from) else { return vec![] };
        if let Some(p) = self.peers.get_mut(&from) {
            p.last_seen = now;
        }
        let mut out = vec![];
        if self.online_seen.insert(from) {
            out.extend(self.online_event(&from));
        }
        let writer = role.can_write();
        match msg {
            RoomMessage::Profile(p) => {
                if let Some(peer) = self.peers.get_mut(&from)
                    && peer.verified.as_ref().is_some_and(|v| verify_profile(&p, v))
                {
                    peer.profile = Some(p);
                }
            }
            RoomMessage::Presence(p) => {
                let went_offline = !p.online;
                if let Some(peer) = self.peers.get_mut(&from) {
                    peer.presence = Some(p);
                }
                if went_offline {
                    self.online_seen.remove(&from);
                    if let Some(m) = self.sender_member(&from) {
                        out.push(Effect::Event(RoomEvent::MemberOffline { member: m, device: from }));
                    }
                }
            }
            RoomMessage::Goodbye => {
                if let Some(peer) = self.peers.get_mut(&from) {
                    peer.last_seen = 0;
                }
                self.online_seen.remove(&from);
                if let Some(m) = self.sender_member(&from) {
                    out.push(Effect::Event(RoomEvent::MemberOffline { member: m, device: from }));
                }
            }
            RoomMessage::Crdt { doc, update } if writer => {
                // New chat messages from others become Chat events, like our own.
                let chat_before: Option<BTreeSet<String>> =
                    (doc == DocName::Threads).then(|| docs::chat(self.doc(&doc)).into_iter().map(|c| c.id).collect());
                if apply_update(self.doc(&doc), &update).is_ok() {
                    let settings = doc == DocName::Settings;
                    if let Some(seen) = chat_before {
                        for c in docs::chat(self.doc(&DocName::Threads)) {
                            if !seen.contains(&c.id) {
                                out.push(Effect::Event(RoomEvent::Chat(c)));
                            }
                        }
                    }
                    out.push(Effect::Event(RoomEvent::DocChanged { doc }));
                    if settings {
                        out.extend(self.fetch_new_whiteboards());
                    }
                }
            }
            RoomMessage::CrdtSyncRequest { doc, state_vector } => {
                let answer = self.is_acting_host() || self.host.device.is_none_or(|h| !self.device_online(&h, now));
                if answer {
                    if let Ok(update) = diff_since(self.doc(&doc), &state_vector) {
                        out.push(Effect::Send(RoomMessage::Crdt { doc: doc.clone(), update }));
                    }
                    // Ask back once in a while so the requester's changes reach us too.
                    if now - self.last_sync_reply >= 5 {
                        self.last_sync_reply = now;
                        out.extend(self.sync_requests());
                    }
                }
            }
            RoomMessage::MemberAdded { chain } => out.extend(self.on_member_added(*chain, now)),
            RoomMessage::RoleChanged { member: _, chain } => {
                if let Ok(v) = chain.verify(now, &|d| self.revoked.contains(d))
                    && v.room == self.id
                {
                    if v.device.sign == self.me {
                        self.my_verified = v;
                        self.my_chain = *chain;
                    } else if let Some(p) = self.peers.get_mut(&v.device.sign) {
                        p.chain = *chain;
                        p.verified = Some(v);
                    }
                }
            }
            RoomMessage::MemberRemoved { member, devices, by: _ } if role.can_rotate_keys() => {
                for d in &devices {
                    self.revoked.insert(*d);
                    self.peers.remove(d);
                }
                if devices.contains(&self.me) || member == self.my_member {
                    out.push(Effect::Event(RoomEvent::YouWereRemoved));
                } else {
                    out.push(Effect::Event(RoomEvent::MemberRemoved { member }));
                }
            }
            RoomMessage::EpochRotated { epoch, deliveries } if role.can_rotate_keys() => {
                for d in deliveries {
                    if d.signer != from {
                        continue;
                    }
                    let Ok(body) = d.open() else { continue };
                    if body.recipient == self.me {
                        if let Ok(k) = open_epoch_key(&self.identity, &d) {
                            let fresh = !self.keys.contains_key(&k.epoch);
                            self.add_key(k);
                            if fresh {
                                out.push(Effect::Event(RoomEvent::KeysRotated { epoch }));
                            }
                        }
                    } else if self.knocks.values().any(|k| k.device == body.recipient && k.via == self.me) {
                        self.pending_deliveries.insert(body.recipient, d);
                    }
                }
            }
            RoomMessage::ActingHost { term, device, member: _, role: _, joined_at: _ } => {
                out.extend(self.on_acting_host(from, term, device, now));
            }
            RoomMessage::Knock(k) if writer => {
                self.knocks.insert(k.id, k.clone());
                out.push(Effect::Event(RoomEvent::Knock { knock: k }));
            }
            RoomMessage::KnockDecided { id, approved, by: _ } if role.can_approve_joins() => {
                out.push(Effect::Event(RoomEvent::KnockDecided { id, approved }));
                if !approved
                    && let Some(k) = self.knocks.remove(&id)
                    && k.via == self.me
                {
                    out.push(Effect::Event(RoomEvent::JoinDenied { knock: id }));
                }
            }
            RoomMessage::Claim(c) if writer => out.extend(self.on_claim(from, c, now)),
            // Only the device running an agent may speak for it.
            RoomMessage::Agent(ev) if writer && ev.agent.device == from => {
                out.push(Effect::Event(RoomEvent::Agent { event: ev }));
            }
            RoomMessage::Approval(a) => out.extend(self.on_approval(from, role, a, now)),
            RoomMessage::Terminal(t) if writer => {
                use polyph_proto::messages::TerminalMsg;
                let Some(member) = self.sender_member(&from) else { return out };
                // Nobody types or asks on someone else's behalf.
                let honest = match &t {
                    TerminalMsg::Input { typist, .. } => *typist == member,
                    TerminalMsg::RequestInput { member: m, .. } => *m == member,
                    TerminalMsg::Opened { owner, .. } => *owner == member,
                    _ => true,
                };
                if honest {
                    out.push(Effect::Event(RoomEvent::Terminal { from, member, msg: Box::new(t) }));
                }
            }
            RoomMessage::Mirror(m) if writer => {
                // A device proposes changes only for its own agents.
                let honest = match &m {
                    polyph_proto::messages::MirrorMsg::Propose { agent: Some(a), .. } => a.device == from,
                    _ => true,
                };
                if honest {
                    out.push(Effect::Event(RoomEvent::Mirror { from, msg: Box::new(m) }));
                }
            }
            RoomMessage::Inbox { id, to, from_member, from_agent, thread, reply_to, text, at, hops } if writer => {
                // The sender speaks only for itself and its own agents.
                let honest = self.sender_member(&from) == Some(from_member)
                    && from_agent.as_ref().is_none_or(|a| a.device == from && a.member == from_member);
                if honest && text.len() <= MAX_INBOX_TEXT {
                    let for_me = to.device == self.me;
                    let message = InboxMessage { id, to, from_member, from_agent, thread, reply_to, text, at, hops };
                    out.push(Effect::Event(RoomEvent::Inbox { message, for_me }));
                }
            }
            _ => {}
        }
        out
    }

    fn online_event(&self, d: &DeviceId) -> Vec<Effect> {
        match self.peers.get(d) {
            Some(p) => match p.member() {
                Some(m) => vec![Effect::Event(RoomEvent::MemberOnline {
                    member: m,
                    device: *d,
                    name: p.name(),
                    role: p.role(),
                    hue: p.profile.as_ref().map(|p| p.hue).unwrap_or(NO_HUE),
                })],
                None => vec![],
            },
            None => vec![],
        }
    }

    fn on_hello(
        &mut self,
        from: DeviceId,
        chain: CertChain,
        profile: Profile,
        presence: Presence,
        now: i64,
    ) -> Vec<Effect> {
        let revoked = self.revoked.clone();
        let Ok(v) = chain.verify(now, &|d| revoked.contains(d)) else { return vec![] };
        if v.room != self.id || v.device.sign != from || !verify_profile(&profile, &v) {
            return vec![];
        }
        let known_member = self.peers.values().any(|p| p.member() == Some(v.member)) || v.member == self.my_member;
        let new_device = !self.peers.contains_key(&from);
        let (member, role) = (v.member, v.role);
        let name = profile.display_name.clone();
        self.peers.insert(
            from,
            Peer { chain, verified: Some(v), profile: Some(profile), presence: Some(presence), last_seen: now },
        );
        let mut out = vec![];
        if !known_member {
            out.push(Effect::Event(RoomEvent::MemberJoined { member, name, role }));
        }
        if self.online_seen.insert(from) || new_device {
            out.extend(self.online_event(&from));
            // Introduce ourselves to a device we had not seen online.
            out.push(Effect::Send(self.hello(now)));
            if self.is_acting_host() {
                out.push(Effect::Send(self.announce(now)));
                out.push(Effect::Send(RoomMessage::Claim(ClaimMsg::Table {
                    term: self.host.term,
                    claims: self.claims.all(),
                })));
            }
        }
        out
    }

    fn on_member_added(&mut self, chain: CertChain, now: i64) -> Vec<Effect> {
        let revoked = self.revoked.clone();
        let Ok(v) = chain.verify(now, &|d| revoked.contains(d)) else { return vec![] };
        if v.room != self.id {
            return vec![];
        }
        let dev = v.device.sign;
        let mut out = vec![];
        let knock = self.knocks.iter().find(|(_, k)| k.device == dev).map(|(id, k)| (*id, k.clone()));
        if let Some((kid, k)) = knock {
            out.push(Effect::Event(RoomEvent::KnockDecided { id: kid, approved: true }));
            if k.via == self.me {
                if let Some(delivery) = self.pending_deliveries.remove(&dev) {
                    self.knocks.remove(&kid);
                    out.push(Effect::Event(RoomEvent::JoinReady {
                        knock: kid,
                        approved: Box::new(JoinApproved {
                            room: self.id,
                            room_name: Some(self.name.clone()),
                            role: v.role,
                            chain: chain.clone(),
                            key: delivery,
                            relay: self.relay.clone(),
                        }),
                    }));
                }
            } else {
                self.knocks.remove(&kid);
            }
        }
        self.peers.entry(dev).or_insert(Peer { chain, verified: Some(v), profile: None, presence: None, last_seen: 0 });
        out
    }

    // ---------- members ----------

    /// Approves a knock: issues the certificate and seals the room key for the
    /// new device. Requires Owner or Maintainer.
    pub fn approve_knock(&mut self, knock: ObjectId, role: Role, now: i64) -> Result<Vec<Effect>> {
        if !self.role().can_approve_joins() {
            return Err(RoomError::Forbidden);
        }
        if role > self.role() {
            return Err(RoomError::Forbidden);
        }
        let k = self.knocks.get(&knock).cloned().ok_or(RoomError::NotFound("knock"))?;
        let kx: [u8; 32] = k.kx.as_slice().try_into().map_err(|_| RoomError::Invalid("knock has no key"))?;
        let salt: [u8; 16] =
            k.name_salt.as_slice().try_into().map_err(|_| RoomError::Invalid("knock has no name salt"))?;
        let device = DevicePublic { sign: k.device, kx };
        let cert = MembershipCert {
            room: self.id,
            member: k.member,
            device,
            name_commit: name_commitment(&k.display_name, &salt),
            role,
            issued_at: now,
            expires_at: None,
            capabilities: vec![],
        };
        let chain = self.my_chain.issue(&self.identity, &cert)?;
        let key = self.keyring.current_key().ok_or(RoomError::Invalid("no room key"))?;
        let delivery = seal_epoch_key(&self.identity, &device, key)?;
        let mut out = vec![
            Effect::Send(RoomMessage::KnockDecided { id: knock, approved: true, by: self.my_member }),
            Effect::Send(RoomMessage::EpochRotated { epoch: key.epoch, deliveries: vec![delivery.clone()] }),
            Effect::Send(RoomMessage::MemberAdded { chain: Box::new(chain.clone()) }),
        ];
        if k.via == self.me {
            self.pending_deliveries.insert(k.device, delivery);
        }
        out.extend(self.on_member_added(chain, now));
        Ok(out)
    }

    pub fn deny_knock(&mut self, knock: ObjectId) -> Result<Vec<Effect>> {
        if !self.role().can_approve_joins() {
            return Err(RoomError::Forbidden);
        }
        let k = self.knocks.remove(&knock).ok_or(RoomError::NotFound("knock"))?;
        let mut out = vec![Effect::Send(RoomMessage::KnockDecided { id: knock, approved: false, by: self.my_member })];
        if k.via == self.me {
            out.push(Effect::Event(RoomEvent::JoinDenied { knock }));
        }
        Ok(out)
    }

    /// Records a knock from a join this device is handling and tells the room.
    pub fn post_knock(&mut self, k: Knock) -> Vec<Effect> {
        self.knocks.insert(k.id, k.clone());
        vec![Effect::Send(RoomMessage::Knock(k.clone())), Effect::Event(RoomEvent::Knock { knock: k })]
    }

    pub fn require_approval(&mut self) -> bool {
        docs::setting(self.doc(&DocName::Settings), "require_approval").and_then(|v| v.as_bool()).unwrap_or(true)
    }

    /// Removes a member and rotates the room key so they cannot read new content.
    pub fn remove_member(&mut self, member: MemberId, now: i64) -> Result<Vec<Effect>> {
        if !self.role().can_rotate_keys() {
            return Err(RoomError::Forbidden);
        }
        let target_role = self
            .peers
            .values()
            .filter(|p| p.member() == Some(member))
            .map(|p| p.role())
            .max()
            .ok_or(RoomError::NotFound("member"))?;
        if target_role == Role::Owner && self.role() != Role::Owner {
            return Err(RoomError::Forbidden);
        }
        let devices: Vec<DeviceId> =
            self.peers.iter().filter(|(_, p)| p.member() == Some(member)).map(|(d, _)| *d).collect();
        let mut out =
            vec![Effect::Send(RoomMessage::MemberRemoved { member, devices: devices.clone(), by: self.my_member })];
        for d in &devices {
            self.revoked.insert(*d);
            self.peers.remove(d);
            self.online_seen.remove(d);
        }
        out.extend(self.rotate(now)?);
        out.push(Effect::Event(RoomEvent::MemberRemoved { member }));
        Ok(out)
    }

    /// Starts a new epoch and seals it to every remaining device. The rotation
    /// message goes out under the old epoch; later messages use the new one.
    pub fn rotate(&mut self, _now: i64) -> Result<Vec<Effect>> {
        if !self.role().can_rotate_keys() {
            return Err(RoomError::Forbidden);
        }
        let next = EpochKey::generate(self.id, self.epoch() + 1)?;
        let mut deliveries = vec![];
        for (d, p) in &self.peers {
            if self.revoked.contains(d) {
                continue;
            }
            if let Some(v) = &p.verified {
                deliveries.push(seal_epoch_key(&self.identity, &v.device, &next)?);
            }
        }
        let msg = RoomMessage::EpochRotated { epoch: next.epoch, deliveries };
        let epoch = next.epoch;
        // Seal the announcement now, under the current epoch, before switching.
        let out = vec![Effect::Send(msg), Effect::Event(RoomEvent::KeysRotated { epoch })];
        self.pending_rotation = Some(next);
        Ok(out)
    }

    /// Called by the daemon after it has published every message produced by
    /// `rotate`, so the announcement went out under the old key.
    pub fn commit_rotation(&mut self) {
        if let Some(k) = self.pending_rotation.take() {
            self.add_key(k);
        }
    }

    pub fn change_role(&mut self, member: MemberId, role: Role, now: i64) -> Result<Vec<Effect>> {
        if !self.role().can_issue_certs() || role > self.role() {
            return Err(RoomError::Forbidden);
        }
        let (dev, peer) = self
            .peers
            .iter()
            .find(|(_, p)| p.member() == Some(member))
            .map(|(d, p)| (*d, p.clone()))
            .ok_or(RoomError::NotFound("member"))?;
        let v = peer.verified.ok_or(RoomError::NotFound("member"))?;
        let cert = MembershipCert {
            room: self.id,
            member,
            device: v.device,
            name_commit: v.name_commit,
            role,
            issued_at: now,
            expires_at: None,
            capabilities: v.capabilities,
        };
        let chain = self.my_chain.issue(&self.identity, &cert)?;
        let nv = chain.verify(now, &|_| false)?;
        if let Some(p) = self.peers.get_mut(&dev) {
            p.chain = chain.clone();
            p.verified = Some(nv);
        }
        Ok(vec![Effect::Send(RoomMessage::RoleChanged { member, chain: Box::new(chain) })])
    }

    pub fn set_hue_if_unset(&mut self) -> Vec<Effect> {
        if self.my_profile.hue != NO_HUE {
            return vec![];
        }
        let used: Vec<u16> =
            self.peers.values().filter_map(|p| p.profile.as_ref().map(|p| p.hue)).filter(|h| *h != NO_HUE).collect();
        self.my_profile.hue = assign_hue(&used);
        vec![Effect::Send(RoomMessage::Profile(self.my_profile.clone()))]
    }

    pub fn set_display_name(&mut self, _name: &str) -> Result<()> {
        // Names are committed in certificates; renaming needs a re-issued
        // certificate from an Owner or Maintainer.
        Err(RoomError::Invalid("renaming needs a new certificate"))
    }

    // ---------- chat and documents ----------

    /// Applies a local change to a document and returns the update to broadcast.
    pub fn change_doc(&mut self, name: DocName, f: impl FnOnce(&Doc)) -> Result<Vec<Effect>> {
        self.change_doc_as(name, None, f)
    }

    /// Like [`RoomCore::change_doc`], crediting board changes in the activity
    /// feed to `agent` (a label) as well as this member.
    pub fn change_doc_as(&mut self, name: DocName, agent: Option<&str>, f: impl FnOnce(&Doc)) -> Result<Vec<Effect>> {
        if !self.role().can_write() {
            return Err(RoomError::Forbidden);
        }
        let doc = self.doc(&name);
        let board_before = (name == DocName::Board).then(|| crate::activity::cards(doc));
        let before = state_vector(doc);
        f(doc);
        let update = diff_since(doc, &before)?;
        let mut out = vec![
            Effect::Send(RoomMessage::Crdt { doc: name.clone(), update }),
            Effect::Event(RoomEvent::DocChanged { doc: name }),
        ];
        // Every local board change is logged by its author.
        if let Some(before) = board_before {
            let after = crate::activity::cards(self.doc(&DocName::Board));
            let changes = crate::activity::diff(&before, &after);
            if !changes.is_empty() {
                let actor = self.my_member.to_hex();
                let now = self.clock.as_ref().map(|c| c()).unwrap_or(self.last_now).max(self.last_now);
                let act = self.doc(&DocName::Activity);
                let sv = state_vector(act);
                let mut ids = || ObjectId::random().map(|o| o.to_hex()).unwrap_or_default();
                crate::activity::record_board_change(act, &after, changes, &actor, agent, now, &mut ids);
                let update = diff_since(act, &sv)?;
                out.push(Effect::Send(RoomMessage::Crdt { doc: DocName::Activity, update }));
                out.push(Effect::Event(RoomEvent::DocChanged { doc: DocName::Activity }));
            }
        }
        Ok(out)
    }

    /// Whiteboards listed in the Settings index.
    pub fn whiteboards(&mut self) -> Vec<crate::whiteboard::WhiteboardInfo> {
        docs::settings_with_prefix(self.doc(&DocName::Settings), crate::whiteboard::INDEX_PREFIX)
            .into_iter()
            .filter_map(|(_, v)| serde_json::from_value(v).ok())
            .collect()
    }

    /// Asks peers for whiteboards that are listed but not here yet.
    fn fetch_new_whiteboards(&mut self) -> Vec<Effect> {
        let missing: Vec<DocName> = self
            .whiteboards()
            .into_iter()
            .map(|w| DocName::Whiteboard(w.id))
            .filter(|d| !self.docs.contains_key(d))
            .collect();
        missing
            .into_iter()
            .map(|d| {
                let sv = state_vector(self.doc(&d));
                Effect::Send(RoomMessage::CrdtSyncRequest { doc: d, state_vector: sv })
            })
            .collect()
    }

    pub fn send_chat(
        &mut self,
        text: &str,
        thread: Option<String>,
        agent: Option<String>,
        now: i64,
    ) -> Result<Vec<Effect>> {
        let e = ChatEntry {
            id: ObjectId::random()?.to_hex(),
            author: self.my_member.to_hex(),
            agent,
            thread,
            text: crate::redact::redact(text).into_owned(),
            at: now,
        };
        let mut out = self.change_doc(DocName::Threads, |d| docs::append_chat(d, &e))?;
        out.push(Effect::Event(RoomEvent::Chat(e)));
        Ok(out)
    }

    pub fn chat(&mut self) -> Vec<ChatEntry> {
        docs::chat(self.doc(&DocName::Threads))
    }

    // ---------- election ----------

    fn candidates(&self, now: i64) -> Vec<Candidate> {
        let mut c: Vec<Candidate> = self
            .peers
            .iter()
            .filter(|(d, p)| !self.revoked.contains(d) && p.is_daemon() && p.online(now) && p.verified.is_some())
            .map(|(d, p)| Candidate { device: *d, role: p.role(), joined_at: p.joined_at() })
            .collect();
        if !matches!(self.surface, Surface::Browser | Surface::Phone) {
            c.push(self.my_candidate());
        }
        c
    }

    fn my_candidate(&self) -> Candidate {
        let joined = self.my_chain.certs.last().and_then(|c| c.open().ok()).map(|c| c.issued_at).unwrap_or(i64::MAX);
        Candidate { device: self.me, role: self.role(), joined_at: joined }
    }

    fn candidate_for(&self, d: &DeviceId) -> Option<Candidate> {
        if *d == self.me {
            return Some(self.my_candidate());
        }
        self.peers.get(d).filter(|p| p.verified.is_some()).map(|p| Candidate {
            device: *d,
            role: p.role(),
            joined_at: p.joined_at(),
        })
    }

    fn device_online(&self, d: &DeviceId, now: i64) -> bool {
        *d == self.me || self.peers.get(d).is_some_and(|p| p.online(now))
    }

    fn announce(&mut self, now: i64) -> RoomMessage {
        self.host.last_announce = now;
        let c = self.my_candidate();
        RoomMessage::ActingHost {
            term: self.host.term,
            device: self.me,
            member: self.my_member,
            role: c.role,
            joined_at: c.joined_at,
        }
    }

    fn on_acting_host(&mut self, from: DeviceId, term: u64, device: DeviceId, now: i64) -> Vec<Effect> {
        if device != from {
            return vec![];
        }
        let Some(theirs) = self.candidate_for(&device) else { return vec![] };
        if !crate::election::eligible(theirs.role) {
            return vec![];
        }
        let current = self.host.device.and_then(|d| self.candidate_for(&d));
        let current_alive = self.host.device.is_some_and(|d| self.device_online(&d, now));
        let better = match current {
            None => true,
            Some(_) if !current_alive => true,
            Some(cur) if cur.device == device => true,
            Some(cur) => {
                let ord = crate::election::rank_cmp(&theirs, &cur);
                term > self.host.term && ord != std::cmp::Ordering::Less || ord == std::cmp::Ordering::Greater
            }
        };
        let mut out = vec![];
        if better {
            let was_me = self.is_acting_host();
            let changed = self.host.device != Some(device);
            self.host.device = Some(device);
            self.host.term = self.host.term.max(term);
            self.host.last_seen = now;
            if was_me && changed {
                // Step down and hand over our claim table.
                out.push(Effect::Send(RoomMessage::Claim(ClaimMsg::Table {
                    term: self.host.term,
                    claims: self.claims.all(),
                })));
            }
            if changed {
                out.push(Effect::Event(RoomEvent::ActingHost { device, term: self.host.term, me: false }));
            }
        } else if self.is_acting_host() {
            // A lower-ranked device thinks it is host (for example after a
            // split). Reassert with a newer term so it steps down and sends
            // its table.
            self.host.term = self.host.term.max(term) + 1;
            out.push(Effect::Send(self.announce(now)));
            return out;
        }
        self.host.term = self.host.term.max(term);
        out
    }

    /// Periodic work: heartbeat, election, claim table, lease expiry.
    pub fn tick(&mut self, now: i64) -> Vec<Effect> {
        self.last_now = self.last_now.max(now);
        let mut out = vec![Effect::Send(RoomMessage::Presence(Presence {
            surface: self.surface,
            online: true,
            viewing: None,
            at: now,
        }))];
        // Peers that went quiet.
        let gone: Vec<DeviceId> =
            self.online_seen.iter().filter(|d| !self.peers.get(d).is_some_and(|p| p.online(now))).copied().collect();
        for d in gone {
            self.online_seen.remove(&d);
            if let Some(m) = self.sender_member(&d) {
                out.push(Effect::Event(RoomEvent::MemberOffline { member: m, device: d }));
            }
        }
        out.extend(self.elect(now));
        if self.is_acting_host() {
            if now - self.host.last_announce >= HEARTBEAT {
                out.push(Effect::Send(self.announce(now)));
            }
            if now - self.host.last_table >= TABLE_INTERVAL {
                self.host.last_table = now;
                out.push(Effect::Send(RoomMessage::Claim(ClaimMsg::Table {
                    term: self.host.term,
                    claims: self.claims.all(),
                })));
            }
        }
        let before = self.claims.len();
        self.claims.expire(now);
        if self.claims.len() != before {
            out.push(Effect::Event(RoomEvent::ClaimsChanged { count: self.claims.len() }));
        }
        out
    }

    /// Runs the election from this device's view (docs/protocol.md 10.2).
    pub fn elect(&mut self, now: i64) -> Vec<Effect> {
        let cands = self.candidates(now);
        let Some(best) = top(&cands) else { return vec![] };
        if best.device != self.me || self.is_acting_host() {
            return vec![];
        }
        let current_alive = self.host.device.is_some_and(|d| self.device_online(&d, now));
        let current_rank = self.host.device.and_then(|d| self.candidate_for(&d));
        let take = !current_alive
            || current_rank.is_none_or(|c| crate::election::rank_cmp(&best, &c) == std::cmp::Ordering::Greater);
        if !take {
            return vec![];
        }
        self.host.term += 1;
        self.host.device = Some(self.me);
        self.host.last_seen = now;
        self.host.last_table = now;
        vec![
            Effect::Send(self.announce(now)),
            Effect::Send(RoomMessage::Claim(ClaimMsg::Table { term: self.host.term, claims: self.claims.all() })),
            Effect::Event(RoomEvent::ActingHost { device: self.me, term: self.host.term, me: true }),
        ]
    }

    // ---------- claims ----------

    fn on_claim(&mut self, from: DeviceId, c: ClaimMsg, now: i64) -> Vec<Effect> {
        let mut out = vec![];
        match c {
            ClaimMsg::Request { request, agent, targets, reason, task, lease_secs, at: _ } => {
                if self.host.device.is_none() {
                    out.extend(self.elect(now));
                }
                if self.is_acting_host() && agent.device == from {
                    out.extend(self.arbitrate(request, &agent, &targets, &reason, task, lease_secs, now));
                }
            }
            ClaimMsg::Decision { request, granted, denied } => {
                if self.host.device == Some(from) {
                    for g in &granted {
                        self.claims.merge(vec![g.clone()], now);
                    }
                    out.push(Effect::Event(RoomEvent::ClaimDecision { request, granted, denied }));
                    out.push(Effect::Event(RoomEvent::ClaimsChanged { count: self.claims.len() }));
                }
            }
            ClaimMsg::Renew { claims, lease_secs } => {
                let mine: Vec<ObjectId> = claims
                    .into_iter()
                    .filter(|id| self.claims.get(id).is_some_and(|c| c.owner.device == from))
                    .collect();
                self.claims.renew(&mine, lease_secs, now);
            }
            ClaimMsg::Release { claims } => {
                let mine: Vec<ObjectId> = claims
                    .into_iter()
                    .filter(|id| self.claims.get(id).is_some_and(|c| c.owner.device == from))
                    .collect();
                self.claims.release(&mine);
                out.push(Effect::Event(RoomEvent::ClaimsChanged { count: self.claims.len() }));
            }
            ClaimMsg::Table { term: _, claims } => {
                if self.is_acting_host() {
                    // A stepping-down host or the other side of a split.
                    let lost = self.claims.merge(claims, now);
                    for (loser, winner) in lost {
                        out.push(Effect::Send(RoomMessage::Claim(ClaimMsg::Revoked {
                            claim: loser.clone(),
                            winner: winner.clone(),
                        })));
                        out.push(Effect::Event(RoomEvent::ClaimRevoked { claim: loser, winner }));
                    }
                    self.host.last_table = now;
                    out.push(Effect::Send(RoomMessage::Claim(ClaimMsg::Table {
                        term: self.host.term,
                        claims: self.claims.all(),
                    })));
                } else if self.host.device == Some(from) {
                    self.claims.replace(claims);
                }
                out.push(Effect::Event(RoomEvent::ClaimsChanged { count: self.claims.len() }));
            }
            ClaimMsg::Revoked { claim, winner } => {
                if self.host.device == Some(from) {
                    self.claims.release(&[claim.id]);
                    out.push(Effect::Event(RoomEvent::ClaimRevoked { claim, winner }));
                }
            }
        }
        out
    }

    #[allow(clippy::too_many_arguments)]
    fn arbitrate(
        &mut self,
        request: ObjectId,
        agent: &AgentRef,
        targets: &[ClaimTarget],
        reason: &str,
        task: Option<ObjectId>,
        lease: u32,
        now: i64,
    ) -> Vec<Effect> {
        let mut next = || ObjectId::random().unwrap_or(ObjectId([0; 16]));
        let r = self.claims.grant(agent, targets, reason, task, lease, now, self.host.term, &mut next);
        vec![
            Effect::Send(RoomMessage::Claim(ClaimMsg::Decision {
                request,
                granted: r.granted.clone(),
                denied: r.denied.clone(),
            })),
            Effect::Event(RoomEvent::ClaimDecision { request, granted: r.granted, denied: r.denied }),
            Effect::Event(RoomEvent::ClaimsChanged { count: self.claims.len() }),
        ]
    }

    /// Asks for claims for one of this device's agents. The answer arrives as
    /// a `ClaimDecision` event with the returned request id.
    pub fn request_claims(
        &mut self,
        agent: &AgentRef,
        targets: &[ClaimTarget],
        reason: &str,
        task: Option<ObjectId>,
        lease: u32,
        now: i64,
    ) -> Result<(ObjectId, Vec<Effect>)> {
        if !self.role().can_write() {
            return Err(RoomError::Forbidden);
        }
        let request = ObjectId::random()?;
        let mut out = self.elect(now);
        if self.is_acting_host() {
            out.extend(self.arbitrate(request, agent, targets, reason, task, lease, now));
        } else {
            out.push(Effect::Send(RoomMessage::Claim(ClaimMsg::Request {
                request,
                agent: agent.clone(),
                targets: targets.to_vec(),
                reason: reason.to_string(),
                task,
                lease_secs: lease,
                at: now,
            })));
        }
        Ok((request, out))
    }

    pub fn release_claims(&mut self, ids: &[ObjectId]) -> Vec<Effect> {
        let mine: Vec<ObjectId> =
            ids.iter().copied().filter(|id| self.claims.get(id).is_some_and(|c| c.owner.device == self.me)).collect();
        self.claims.release(&mine);
        vec![
            Effect::Send(RoomMessage::Claim(ClaimMsg::Release { claims: mine })),
            Effect::Event(RoomEvent::ClaimsChanged { count: self.claims.len() }),
        ]
    }

    // ---------- inboxes ----------

    /// Sends a message to an agent's inbox. `from_agent` must run on this device.
    #[allow(clippy::too_many_arguments)]
    pub fn send_inbox(
        &mut self,
        to: AgentRef,
        from_agent: Option<AgentRef>,
        thread: Option<ObjectId>,
        reply_to: Option<ObjectId>,
        text: &str,
        hops: u8,
        now: i64,
    ) -> Result<(InboxMessage, Vec<Effect>)> {
        if !self.role().can_write() {
            return Err(RoomError::Forbidden);
        }
        if from_agent.as_ref().is_some_and(|a| a.device != self.me) {
            return Err(RoomError::Invalid("that agent does not run on this device"));
        }
        if text.trim().is_empty() || text.len() > MAX_INBOX_TEXT {
            return Err(RoomError::Invalid("message is empty or too long"));
        }
        let message = InboxMessage {
            id: ObjectId::random()?,
            to,
            from_member: self.my_member,
            from_agent,
            thread,
            reply_to,
            text: crate::redact::redact(text).into_owned(),
            at: now,
            hops,
        };
        let for_me = message.to.device == self.me;
        let fx = vec![
            Effect::Send(message.clone().into_wire()),
            Effect::Event(RoomEvent::Inbox { message: message.clone(), for_me }),
        ];
        Ok((message, fx))
    }

    // ---------- approvals ----------

    /// Registers an approval request for one of this device's agents.
    pub fn request_approval(
        &mut self,
        agent: AgentRef,
        kind: ApprovalKind,
        task: Option<ObjectId>,
        now: i64,
    ) -> Result<(ObjectId, Vec<Effect>)> {
        let id = ObjectId::random()?;
        let p = PendingApproval {
            id,
            agent: agent.clone(),
            kind: kind.clone(),
            task,
            created_at: now,
            escalate_at: now + APPROVAL_ESCALATION_SECS,
        };
        self.approvals.insert(id, p.clone());
        Ok((
            id,
            vec![
                Effect::Send(RoomMessage::Approval(ApprovalMsg::Request {
                    id,
                    agent,
                    kind,
                    task,
                    created_at: now,
                    escalate_at: p.escalate_at,
                })),
                Effect::Event(RoomEvent::ApprovalPending { approval: p }),
            ],
        ))
    }

    /// Whether a member with `role` may answer this approval now:
    /// the agent's owner first, anyone with Member or higher after escalation.
    pub fn may_answer(&self, p: &PendingApproval, member: MemberId, role: Role, now: i64) -> bool {
        if !role.can_approve_agents() {
            return false;
        }
        // Typing in someone's terminal runs as them: only they may allow it,
        // so these never escalate (and the asker can never answer).
        if matches!(p.kind, ApprovalKind::TerminalInput { .. }) {
            return member == p.agent.member;
        }
        member == p.agent.member || now >= p.escalate_at
    }

    /// Answers an approval from this device.
    pub fn answer_approval(&mut self, id: ObjectId, decision: Decision, now: i64) -> Result<Vec<Effect>> {
        let p = self.approvals.get(&id).cloned().ok_or(RoomError::NotFound("approval"))?;
        if !self.may_answer(&p, self.my_member, self.role(), now) {
            return Err(RoomError::Forbidden);
        }
        let mut out = vec![Effect::Send(RoomMessage::Approval(ApprovalMsg::Respond {
            id,
            decision: decision.clone(),
            by: self.my_member,
            at: now,
        }))];
        out.extend(self.resolve(id, decision, self.my_member));
        Ok(out)
    }

    fn resolve(&mut self, id: ObjectId, decision: Decision, by: MemberId) -> Vec<Effect> {
        match self.approvals.remove(&id) {
            Some(p) => {
                vec![Effect::Event(RoomEvent::ApprovalResolved { id, decision, by, for_me: p.agent.device == self.me })]
            }
            None => vec![],
        }
    }

    fn on_approval(&mut self, from: DeviceId, role: Role, a: ApprovalMsg, now: i64) -> Vec<Effect> {
        match a {
            ApprovalMsg::Request { id, agent, kind, task, created_at, escalate_at } => {
                if agent.device != from {
                    return vec![];
                }
                let p = PendingApproval { id, agent, kind, task, created_at, escalate_at };
                self.approvals.insert(id, p.clone());
                vec![Effect::Event(RoomEvent::ApprovalPending { approval: p })]
            }
            ApprovalMsg::Respond { id, decision, by, at: _ } => {
                let Some(p) = self.approvals.get(&id).cloned() else { return vec![] };
                // The responder must be who they claim and allowed to answer.
                if self.sender_member(&from) != Some(by) || !self.may_answer(&p, by, role, now) {
                    debug!("ignored approval response from a member who may not answer");
                    return vec![];
                }
                let mut out = self.resolve(id, decision.clone(), by);
                if p.agent.device == self.me {
                    out.push(Effect::Send(RoomMessage::Approval(ApprovalMsg::Resolved { id, decision, by, at: now })));
                }
                out
            }
            ApprovalMsg::Resolved { id, decision, by, at: _ } => {
                let Some(p) = self.approvals.get(&id) else { return vec![] };
                if p.agent.device != from {
                    return vec![];
                }
                self.resolve(id, decision, by)
            }
        }
    }

    // ---------- persistence ----------

    pub fn save(&mut self) -> SavedRoom {
        let docs: Vec<(DocName, Vec<u8>)> = self.docs.iter().map(|(n, d)| (n.clone(), encode_full(d))).collect();
        SavedRoom {
            id: self.id,
            name: self.name.clone(),
            quick: self.quick,
            my_member: self.my_member,
            my_chain: self.my_chain.clone(),
            my_profile: self.my_profile.clone(),
            keys: self.keys.iter().map(|(e, k)| (*e, *k)).collect(),
            seq: self.seq,
            peers: self.peers.values().cloned().collect(),
            revoked: self.revoked.iter().copied().collect(),
            host: self.host.clone(),
            claims: self.claims.all(),
            docs,
            cursor: self.cursor,
            relay: self.relay.clone(),
            created_at: self.created_at,
        }
    }

    pub fn load(identity: Arc<DeviceIdentity>, s: SavedRoom, now: i64) -> Result<Self> {
        let mut core = Self::assemble(identity, s.my_chain, s.my_profile, s.name, s.quick, now)?;
        for (e, k) in s.keys {
            core.add_key(EpochKey::from_bytes(s.id, e, k));
        }
        // Sequence numbers must never repeat within an epoch; skip ahead in
        // case the last save happened before some sends.
        core.seq = s.seq + 1000;
        core.revoked = s.revoked.into_iter().collect();
        for mut p in s.peers {
            let revoked = core.revoked.clone();
            p.verified = p.chain.verify(now, &|d| revoked.contains(d)).ok();
            if let Some(v) = &p.verified {
                p.last_seen = 0;
                core.peers.insert(v.device.sign, p);
            }
        }
        core.host = HostState { term: s.host.term, ..Default::default() };
        core.claims.replace(s.claims);
        for (n, d) in s.docs {
            let doc = Doc::new();
            let _ = apply_update(&doc, &d);
            core.docs.insert(n, doc);
        }
        core.cursor = s.cursor;
        core.relay = s.relay;
        core.created_at = s.created_at;
        Ok(core)
    }

    /// Encrypted snapshot for the relay, readable by any holder of the current epoch key.
    pub fn relay_snapshot(&mut self) -> Result<(u64, Vec<u8>)> {
        let mut members: Vec<(CertChain, Option<Profile>)> =
            self.peers.values().map(|p| (p.chain.clone(), p.profile.clone())).collect();
        members.push((self.my_chain.clone(), Some(self.my_profile.clone())));
        let snap = RelaySnapshot {
            name: self.name.clone(),
            members,
            revoked: self.revoked.iter().copied().collect(),
            docs: self.docs.iter().map(|(n, d)| (n.clone(), encode_full(d))).collect(),
            claims: self.claims.all(),
            term: self.host.term,
        };
        let key = self.keyring.current_key().ok_or(RoomError::Invalid("no room key"))?;
        let env = key.seal(&self.identity, 0, &to_cbor(&snap)?)?;
        Ok((key.epoch, env.encode()))
    }

    /// Merges a snapshot another member uploaded. Returns whether it could be read.
    pub fn apply_relay_snapshot(&mut self, blob: &[u8], now: i64) -> bool {
        let Ok(env) = Envelope::decode(blob) else { return false };
        let Some(key) = self.keyring.get(env.epoch) else { return false };
        let Ok(pt) = key.open(&env) else { return false };
        let Ok(snap) = from_cbor::<RelaySnapshot>(&pt) else { return false };
        if env.sender != self.me && !self.peers.contains_key(&env.sender) {
            // Accept only snapshots from devices whose chain is inside the snapshot itself or known.
            let ok = snap.members.iter().any(|(c, _)| {
                c.verify(now, &|_| false).is_ok_and(|v| v.device.sign == env.sender && v.room == self.id)
            });
            if !ok {
                return false;
            }
        }
        if self.name.is_empty() {
            self.name = snap.name;
        }
        for d in snap.revoked {
            self.revoked.insert(d);
        }
        for (chain, profile) in snap.members {
            let revoked = self.revoked.clone();
            if let Ok(v) = chain.verify(now, &|d| revoked.contains(d))
                && v.room == self.id
                && v.device.sign != self.me
            {
                let profile = profile.filter(|p| verify_profile(p, &v));
                self.peers.entry(v.device.sign).or_insert(Peer {
                    chain,
                    verified: Some(v),
                    profile,
                    presence: None,
                    last_seen: 0,
                });
            }
        }
        for (n, d) in snap.docs {
            let _ = apply_update(self.doc(&n), &d);
        }
        self.claims.merge(snap.claims, now);
        self.host.term = self.host.term.max(snap.term);
        true
    }
}
