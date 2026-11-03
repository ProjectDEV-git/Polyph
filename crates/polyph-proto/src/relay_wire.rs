//! Frames between a client (daemon, browser) and a relay. The relay parses
//! these; nothing in them is content. Content travels only inside
//! [`crate::envelope::Envelope`] ciphertext or the PAKE channel.

use serde::{Deserialize, Serialize};

use crate::cert::{CertChain, RoomGenesis};
use crate::codec::{from_cbor_limited, to_cbor};
use crate::envelope::MAX_ENVELOPE;
use crate::error::Result;
use crate::ids::{DeviceId, RendezvousId, RoomId, Sig};
use crate::signed::Signed;

/// Largest frame either side accepts. Snapshots are the biggest frames.
pub const MAX_FRAME: usize = 8 * 1024 * 1024;
pub const MAX_SNAPSHOT: usize = MAX_FRAME - 4096;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CodeKind {
    /// Dies after one successful join or five failures.
    OneTime,
    /// Survives successful joins; locked for ten minutes after five failures.
    Reusable { max_uses: Option<u32> },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "t", content = "c", rename_all = "snake_case")]
pub enum ClientFrame {
    /// First frame. The relay answers with a challenge.
    Hello {
        protocol: u16,
        device: DeviceId,
        client: String,
    },
    /// Signature over the challenge nonce with context `relay-auth`.
    Auth {
        sig: Sig,
    },
    /// Registers a room by its signed genesis so the relay knows the owner
    /// device (needed to authorize ending a quick session).
    RegisterRoom {
        genesis: Signed<RoomGenesis>,
    },
    /// Subscribe to a room. A certificate chain makes this device a writer;
    /// without one it is read-only (spectators). `after` resumes from a cursor.
    Subscribe {
        room: RoomId,
        chain: Option<Box<CertChain>>,
        after: Option<u64>,
    },
    Unsubscribe {
        room: RoomId,
    },
    Publish {
        #[serde(with = "crate::bytes::vec")]
        envelope: Vec<u8>,
    },
    PutSnapshot {
        room: RoomId,
        epoch: u64,
        #[serde(with = "crate::bytes::vec")]
        blob: Vec<u8>,
    },
    GetSnapshots {
        room: RoomId,
    },
    /// Ends a room and deletes its relay state. Signed by the owner device
    /// over `room || timestamp` with context `room-end`.
    EndRoom {
        room: RoomId,
        at: i64,
        sig: Sig,
    },
    HostCode {
        rendezvous: RendezvousId,
        kind: CodeKind,
        expires_at: i64,
    },
    RevokeCode {
        rendezvous: RendezvousId,
    },
    JoinCode {
        rendezvous: RendezvousId,
    },
    /// PAKE and join traffic between a joiner session and the host.
    Pake {
        session: u64,
        #[serde(with = "crate::bytes::vec")]
        data: Vec<u8>,
    },
    /// Host reports the outcome of a join attempt.
    PakeResult {
        session: u64,
        ok: bool,
    },
    Ping,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SnapshotItem {
    pub from: DeviceId,
    pub epoch: u64,
    pub stored_at: i64,
    #[serde(with = "crate::bytes::vec")]
    pub blob: Vec<u8>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    BadFrame,
    Unauthenticated,
    UnsupportedVersion,
    RateLimited,
    TooLarge,
    NotMember,
    RoomFull,
    PlanLimit,
    CodeInUse,
    CodeUnknown,
    CodeLocked,
    Forbidden,
    Internal,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CodeClosed {
    Used,
    Failed,
    Expired,
    Revoked,
    HostLeft,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "t", content = "c", rename_all = "snake_case")]
pub enum RelayFrame {
    Challenge {
        #[serde(with = "crate::bytes::arr32")]
        nonce: [u8; 32],
        protocol: u16,
    },
    Welcome {
        device: DeviceId,
    },
    Subscribed {
        room: RoomId,
        writer: bool,
        cursor: u64,
    },
    Deliver {
        room: RoomId,
        cursor: u64,
        #[serde(with = "crate::bytes::vec")]
        envelope: Vec<u8>,
    },
    /// Buffered history replay finished.
    CaughtUp {
        room: RoomId,
        cursor: u64,
    },
    /// The requested cursor is older than the buffer; fetch a snapshot.
    Gap {
        room: RoomId,
    },
    Snapshots {
        room: RoomId,
        items: Vec<SnapshotItem>,
    },
    RoomEnded {
        room: RoomId,
    },
    CodeHosted {
        rendezvous: RendezvousId,
    },
    /// To the host: a joiner opened a session on one of its codes.
    JoinerArrived {
        rendezvous: RendezvousId,
        session: u64,
    },
    /// To the joiner: connected to a host.
    JoinStarted {
        session: u64,
    },
    Pake {
        session: u64,
        #[serde(with = "crate::bytes::vec")]
        data: Vec<u8>,
    },
    PakeEnded {
        session: u64,
        ok: bool,
    },
    CodeClosed {
        rendezvous: RendezvousId,
        reason: CodeClosed,
    },
    /// Too many failed attempts; tell the room owner.
    CodeLocked {
        rendezvous: RendezvousId,
        until: i64,
    },
    Error {
        code: ErrorCode,
        detail: String,
    },
    Pong,
}

impl ClientFrame {
    pub fn encode(&self) -> Result<Vec<u8>> {
        to_cbor(self)
    }
    pub fn decode(b: &[u8]) -> Result<Self> {
        from_cbor_limited(b, MAX_FRAME)
    }
}

impl RelayFrame {
    pub fn encode(&self) -> Result<Vec<u8>> {
        to_cbor(self)
    }
    pub fn decode(b: &[u8]) -> Result<Self> {
        from_cbor_limited(b, MAX_FRAME)
    }
}

/// Bytes a client signs to authenticate to a relay.
pub fn auth_message(nonce: &[u8; 32], device: &DeviceId) -> Vec<u8> {
    let mut m = nonce.to_vec();
    m.extend_from_slice(&device.0);
    m
}

/// Bytes the owner signs to end a room.
pub fn end_room_message(room: &RoomId, at: i64) -> Vec<u8> {
    let mut m = room.0.to_vec();
    m.extend_from_slice(&at.to_be_bytes());
    m
}

pub const _ASSERT_ENVELOPE_FITS: () = assert!(MAX_ENVELOPE < MAX_FRAME);

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    #[test]
    fn frames_round_trip() {
        let f = ClientFrame::Publish { envelope: vec![1, 2, 3] };
        assert_eq!(ClientFrame::decode(&f.encode().unwrap()).unwrap(), f);
        let r = RelayFrame::Deliver { room: RoomId([1; 16]), cursor: 7, envelope: vec![9] };
        assert_eq!(RelayFrame::decode(&r.encode().unwrap()).unwrap(), r);
    }

    proptest! {
        #[test]
        fn decode_never_panics(data in proptest::collection::vec(any::<u8>(), 0..1024)) {
            let _ = ClientFrame::decode(&data);
            let _ = RelayFrame::decode(&data);
        }
    }
}
