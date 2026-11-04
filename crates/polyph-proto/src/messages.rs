//! Messages that travel encrypted inside envelopes. The relay never sees
//! these. Unknown message kinds decode as [`RoomMessage::Unknown`] so older
//! clients ignore newer messages instead of failing.

// These are wire types; boxing large variants only to satisfy the lint would
// change nothing on the wire and add noise at every construction site.
#![allow(clippy::large_enum_variant)]

use serde::{Deserialize, Serialize};

use crate::cert::{CertChain, Role};
use crate::codec::{from_cbor_limited, to_cbor};
use crate::delivery::EpochKeyDelivery;
use crate::error::Result;
use crate::group::MAX_PLAINTEXT;
use crate::ids::{DeviceId, MemberId, ObjectId};
use crate::signed::Signed;

/// Where a member is connected from.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Surface {
    Cli,
    Tui,
    Desktop,
    Browser,
    Phone,
    Vscode,
    AgentHost,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Profile {
    pub member: MemberId,
    pub display_name: String,
    #[serde(with = "crate::bytes::vec")]
    pub name_salt: Vec<u8>,
    /// OKLCH hue in degrees, chosen when the member first joined.
    pub hue: u16,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Presence {
    pub surface: Surface,
    pub online: bool,
    /// What the member is looking at, e.g. "board", "follow:<agent>".
    pub viewing: Option<String>,
    pub at: i64,
}

/// A reference to an agent: who owns it and which device runs it.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct AgentRef {
    pub id: ObjectId,
    pub member: MemberId,
    pub device: DeviceId,
    /// Agent kind, e.g. "claude", "codex", "local".
    pub kind: String,
    /// Display label, e.g. "Mai/claude".
    pub label: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChatMessage {
    pub id: ObjectId,
    pub thread: Option<ObjectId>,
    pub author: MemberId,
    /// Set when an agent wrote the message.
    pub agent: Option<AgentRef>,
    pub text: String,
    pub at: i64,
}

/// Which shared CRDT document an update belongs to.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DocName {
    Board,
    Threads,
    Activity,
    Memory,
    Settings,
    Whiteboard(String),
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct ClaimTarget {
    /// Repository-relative path with `/` separators. A trailing `/` claims a directory.
    pub path: String,
    /// Optional symbol inside the file, the part after `#` in `path#symbol`.
    pub symbol: Option<String>,
}

impl ClaimTarget {
    pub fn parse(s: &str) -> Self {
        match s.split_once('#') {
            Some((p, sym)) if !sym.is_empty() => Self { path: p.to_string(), symbol: Some(sym.to_string()) },
            _ => Self { path: s.trim_end_matches('#').to_string(), symbol: None },
        }
    }
}

impl std::fmt::Display for ClaimTarget {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.symbol {
            Some(s) => write!(f, "{}#{}", self.path, s),
            None => f.write_str(&self.path),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Claim {
    pub id: ObjectId,
    pub target: ClaimTarget,
    pub owner: AgentRef,
    pub reason: String,
    pub task: Option<ObjectId>,
    /// When the arbiter granted it (used to reconcile claims after a split).
    pub granted_at: i64,
    pub expires_at: i64,
    /// Arbiter term that granted the claim.
    pub term: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "t", content = "c", rename_all = "snake_case")]
pub enum ClaimMsg {
    Request {
        request: ObjectId,
        agent: AgentRef,
        targets: Vec<ClaimTarget>,
        reason: String,
        task: Option<ObjectId>,
        lease_secs: u32,
        at: i64,
    },
    Decision {
        request: ObjectId,
        granted: Vec<Claim>,
        denied: Vec<Claim>,
    },
    Renew {
        claims: Vec<ObjectId>,
        lease_secs: u32,
    },
    Release {
        claims: Vec<ObjectId>,
    },
    /// Full table from the arbiter, sent on election and periodically.
    Table {
        term: u64,
        claims: Vec<Claim>,
    },
    /// After a split heals: this claim lost to an earlier one.
    Revoked {
        claim: Claim,
        winner: Claim,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "t", content = "c", rename_all = "snake_case")]
pub enum ApprovalKind {
    Command { command: String, cwd: String },
    Edit { files: Vec<String>, outside_claims: bool },
    Network { host: String, port: u16 },
    OutsideWorktree { path: String, write: bool },
    Push { remote: String, branch: String },
    Merge { branch: String, into: String },
    TerminalInput { terminal: ObjectId },
    HostCommand { command: String },
    Steer { text: String },
    Other { title: String, detail: String },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ApprovalScope {
    Once,
    Task,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "t", content = "c", rename_all = "snake_case")]
pub enum Decision {
    Approve { scope: ApprovalScope },
    Deny { note: Option<String> },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "t", content = "c", rename_all = "snake_case")]
pub enum ApprovalMsg {
    Request {
        id: ObjectId,
        agent: AgentRef,
        kind: ApprovalKind,
        task: Option<ObjectId>,
        created_at: i64,
        /// After this time anyone whose role allows it may answer.
        escalate_at: i64,
    },
    Respond {
        id: ObjectId,
        decision: Decision,
        by: MemberId,
        at: i64,
    },
    Resolved {
        id: ObjectId,
        decision: Decision,
        by: MemberId,
        at: i64,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentState {
    Starting,
    Idle,
    Running,
    WaitingApproval,
    Blocked,
    Done,
    Error,
    Stopped,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolKind {
    Read,
    Edit,
    Delete,
    Move,
    Search,
    Execute,
    Think,
    Fetch,
    Mcp,
    Other,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolStatus {
    Pending,
    Running,
    Completed,
    Failed,
}

/// Structured tool call data. Summary lines are generated from this, not by a model.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    pub kind: ToolKind,
    pub title: String,
    pub status: ToolStatus,
    #[serde(default)]
    pub paths: Vec<String>,
    pub command: Option<String>,
    pub query: Option<String>,
    pub lines_added: Option<u32>,
    pub lines_removed: Option<u32>,
    pub matches: Option<u32>,
    pub tests_passed: Option<u32>,
    pub tests_failed: Option<u32>,
    pub exit_code: Option<i32>,
    /// Unified diff for edits (already redacted).
    pub diff: Option<String>,
    /// Truncated, redacted output.
    pub output: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Usage {
    pub input_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
    pub cache_read_tokens: Option<u64>,
    pub cache_write_tokens: Option<u64>,
    pub model: Option<String>,
    /// True when the agent uses a subscription rather than per-token billing.
    pub subscription: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "t", content = "c", rename_all = "snake_case")]
pub enum AgentEventKind {
    Started { task: Option<ObjectId>, model: Option<String>, cloud: bool },
    State { state: AgentState, detail: Option<String> },
    Prompt { text: String, from: MemberId },
    Message { text: String },
    Thought { text: String },
    Tool(Box<ToolCall>),
    Usage(Usage),
    Cursor { path: String, line: u32 },
    Diff { path: String, unified: String },
    Stopped { reason: String },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentEvent {
    pub agent: AgentRef,
    pub seq: u64,
    pub at: i64,
    pub kind: AgentEventKind,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "t", content = "c", rename_all = "snake_case")]
pub enum TerminalMsg {
    Opened {
        id: ObjectId,
        owner: MemberId,
        title: String,
        agent: Option<AgentRef>,
        cols: u16,
        rows: u16,
    },
    Output {
        id: ObjectId,
        seq: u64,
        #[serde(with = "crate::bytes::vec")]
        data: Vec<u8>,
    },
    /// Keystrokes from a member granted input. Shown with the typist's color.
    Input {
        id: ObjectId,
        typist: MemberId,
        #[serde(with = "crate::bytes::vec")]
        data: Vec<u8>,
    },
    Resize {
        id: ObjectId,
        cols: u16,
        rows: u16,
    },
    Control {
        id: ObjectId,
        writer: Option<MemberId>,
    },
    Closed {
        id: ObjectId,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Knock {
    pub id: ObjectId,
    pub display_name: String,
    pub member: MemberId,
    pub device: DeviceId,
    pub requested_role: Role,
    /// The device running the join handshake, which forwards the result.
    pub via: DeviceId,
    pub at: i64,
    /// Joiner's X25519 key, so any Owner or Maintainer can seal the room key.
    #[serde(default, with = "crate::bytes::vec")]
    pub kx: Vec<u8>,
    /// Salt for the joiner's display-name commitment.
    #[serde(default, with = "crate::bytes::vec")]
    pub name_salt: Vec<u8>,
    #[serde(default)]
    pub surface: Option<String>,
}

/// One file of a hosted working copy. `hash` is BLAKE3 of the content.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MirrorEntry {
    pub path: String,
    #[serde(with = "crate::bytes::arr32")]
    pub hash: [u8; 32],
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MirrorFile {
    pub path: String,
    #[serde(with = "crate::bytes::vec")]
    pub data: Vec<u8>,
}

/// Hosted mode: the host's working copy, mirrored to members. Only the
/// hosting device sends `Index`, `Files`, `Changed` and `Rejected`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "t", content = "c", rename_all = "snake_case")]
pub enum MirrorMsg {
    /// Member to host: send the index.
    IndexRequest,
    /// Host: every mirrored file and its hash.
    Index { files: Vec<MirrorEntry>, too_large: Vec<String> },
    /// Member to host: send these files.
    Fetch { paths: Vec<String> },
    /// Host: file contents (members keep the ones they asked for).
    Files { files: Vec<MirrorFile> },
    /// Member to host: a change made in a mirror. `base` is the hash the
    /// change was made on (None for a new file); `deleted` removes the file.
    Propose {
        id: ObjectId,
        agent: Option<AgentRef>,
        path: String,
        #[serde(default, with = "crate::bytes::vec")]
        base: Vec<u8>,
        #[serde(with = "crate::bytes::vec")]
        data: Vec<u8>,
        deleted: bool,
    },
    /// Host: the working copy changed (by the host, or an accepted proposal).
    Changed {
        path: String,
        #[serde(with = "crate::bytes::vec")]
        data: Vec<u8>,
        deleted: bool,
        by: Option<AgentRef>,
        proposal: Option<ObjectId>,
    },
    /// Host to proposer: the change was not applied.
    Rejected { id: ObjectId, path: String, reason: String },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "t", content = "c", rename_all = "snake_case")]
pub enum RoomMessage {
    /// Sent on connect: proves membership and introduces the device.
    Hello {
        chain: Box<CertChain>,
        profile: Profile,
        presence: Presence,
    },
    Profile(Profile),
    Presence(Presence),
    Chat(ChatMessage),
    Crdt {
        doc: DocName,
        #[serde(with = "crate::bytes::vec")]
        update: Vec<u8>,
    },
    CrdtSyncRequest {
        doc: DocName,
        #[serde(with = "crate::bytes::vec")]
        state_vector: Vec<u8>,
    },
    MemberAdded {
        chain: Box<CertChain>,
    },
    MemberRemoved {
        member: MemberId,
        devices: Vec<DeviceId>,
        by: MemberId,
    },
    RoleChanged {
        member: MemberId,
        chain: Box<CertChain>,
    },
    /// A new epoch: keys sealed to each remaining device.
    EpochRotated {
        epoch: u64,
        deliveries: Vec<Signed<EpochKeyDelivery>>,
    },
    /// The acting host announces itself; `term` increases on each election.
    ActingHost {
        term: u64,
        device: DeviceId,
        member: MemberId,
        role: Role,
        joined_at: i64,
    },
    Knock(Knock),
    KnockDecided {
        id: ObjectId,
        approved: bool,
        by: MemberId,
    },
    Claim(ClaimMsg),
    Approval(ApprovalMsg),
    Agent(Box<AgentEvent>),
    /// Directed message to an agent's inbox (`@member/agent`, ask_agent, reply).
    Inbox {
        id: ObjectId,
        to: AgentRef,
        from_member: MemberId,
        from_agent: Option<AgentRef>,
        thread: Option<ObjectId>,
        reply_to: Option<ObjectId>,
        text: String,
        at: i64,
        /// Agent-to-agent hops since a human last spoke in this exchange (0 for
        /// a human's message). Receivers stop waking agents past a limit.
        #[serde(default)]
        hops: u8,
    },
    Terminal(TerminalMsg),
    /// A spectator view key was created or revoked by the owner.
    SpectatorLink {
        id: ObjectId,
        revoked: bool,
    },
    /// Hosted mode file mirroring.
    Mirror(MirrorMsg),
    Goodbye,
    /// A message kind this version does not know. Never sent.
    Unknown,
}

/// Every tag this version understands. Anything else decodes as `Unknown`.
const KNOWN_KINDS: &[&str] = &[
    "hello",
    "profile",
    "presence",
    "chat",
    "crdt",
    "crdt_sync_request",
    "member_added",
    "member_removed",
    "role_changed",
    "epoch_rotated",
    "acting_host",
    "knock",
    "knock_decided",
    "claim",
    "approval",
    "agent",
    "inbox",
    "terminal",
    "spectator_link",
    "mirror",
    "goodbye",
    "unknown",
];

#[derive(Deserialize)]
struct Tag {
    t: String,
}

impl RoomMessage {
    pub fn encode(&self) -> Result<Vec<u8>> {
        to_cbor(self)
    }

    pub fn decode(b: &[u8]) -> Result<Self> {
        match from_cbor_limited::<RoomMessage>(b, MAX_PLAINTEXT) {
            Ok(m) => Ok(m),
            Err(e) => {
                let tag: Tag = from_cbor_limited(b, MAX_PLAINTEXT).map_err(|_| e.clone())?;
                if KNOWN_KINDS.contains(&tag.t.as_str()) { Err(e) } else { Ok(RoomMessage::Unknown) }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_kind_is_tolerated() {
        #[derive(Serialize)]
        #[serde(tag = "t", content = "c", rename_all = "snake_case")]
        enum Future {
            HoloDeck { x: u32 },
        }
        let b = to_cbor(&Future::HoloDeck { x: 1 }).unwrap();
        assert_eq!(RoomMessage::decode(&b).unwrap(), RoomMessage::Unknown);
    }

    #[test]
    fn claim_target_parse() {
        let t = ClaimTarget::parse("src/auth.rs#login");
        assert_eq!(t.path, "src/auth.rs");
        assert_eq!(t.symbol.as_deref(), Some("login"));
        assert_eq!(t.to_string(), "src/auth.rs#login");
        assert_eq!(ClaimTarget::parse("src/").symbol, None);
    }
}
