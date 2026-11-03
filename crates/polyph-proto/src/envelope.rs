//! The envelope: the only structure the relay can parse.
//!
//! Layout (big endian):
//! ```text
//! magic "PY" | version u16 | room 16 | sender 32 | epoch u64 | seq u64 |
//! nonce 24 | ct_len u32 | ciphertext | signature 64
//! ```
//! The header is everything before `ct_len`. It is the AEAD associated data,
//! and the signature covers header || ciphertext.

use crate::error::{ProtoError, Result};
use crate::ids::{DeviceId, RoomId, Sig};
use crate::version::{PROTOCOL_VERSION, is_compatible};

pub const MAGIC: [u8; 2] = *b"PY";
pub const HEADER_LEN: usize = 2 + 2 + 16 + 32 + 8 + 8 + 24;
pub const OVERHEAD: usize = HEADER_LEN + 4 + 64;
/// Largest envelope accepted anywhere (relay, daemon, browser).
pub const MAX_ENVELOPE: usize = 1024 * 1024;

#[derive(Clone, PartialEq, Eq)]
pub struct Envelope {
    pub version: u16,
    pub room: RoomId,
    pub sender: DeviceId,
    pub epoch: u64,
    pub seq: u64,
    pub nonce: [u8; 24],
    pub ciphertext: Vec<u8>,
    pub signature: Sig,
}

impl std::fmt::Debug for Envelope {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Envelope")
            .field("room", &self.room)
            .field("sender", &self.sender)
            .field("epoch", &self.epoch)
            .field("seq", &self.seq)
            .field("ct_len", &self.ciphertext.len())
            .finish()
    }
}

impl Envelope {
    pub fn header_bytes(&self) -> [u8; HEADER_LEN] {
        let mut h = [0u8; HEADER_LEN];
        let mut o = 0;
        let mut put = |b: &[u8]| {
            h[o..o + b.len()].copy_from_slice(b);
            o += b.len();
        };
        put(&MAGIC);
        put(&self.version.to_be_bytes());
        put(&self.room.0);
        put(&self.sender.0);
        put(&self.epoch.to_be_bytes());
        put(&self.seq.to_be_bytes());
        put(&self.nonce);
        h
    }

    /// Bytes covered by the signature.
    pub fn signed_bytes(&self) -> Vec<u8> {
        let mut v = Vec::with_capacity(HEADER_LEN + self.ciphertext.len());
        v.extend_from_slice(&self.header_bytes());
        v.extend_from_slice(&self.ciphertext);
        v
    }

    pub fn encode(&self) -> Vec<u8> {
        let mut v = Vec::with_capacity(OVERHEAD + self.ciphertext.len());
        v.extend_from_slice(&self.header_bytes());
        v.extend_from_slice(&(self.ciphertext.len() as u32).to_be_bytes());
        v.extend_from_slice(&self.ciphertext);
        v.extend_from_slice(&self.signature.0);
        v
    }

    /// Parses an envelope, enforcing size limits before reading anything.
    pub fn decode(bytes: &[u8]) -> Result<Self> {
        if bytes.len() > MAX_ENVELOPE {
            return Err(ProtoError::TooLarge { len: bytes.len(), max: MAX_ENVELOPE });
        }
        if bytes.len() < OVERHEAD {
            return Err(ProtoError::Malformed("envelope shorter than its fixed overhead"));
        }
        if bytes[..2] != MAGIC {
            return Err(ProtoError::Malformed("bad envelope magic"));
        }
        let mut r = Reader { b: bytes, o: 2 };
        let version = u16::from_be_bytes(r.take::<2>()?);
        if !is_compatible(version) {
            return Err(ProtoError::UnsupportedVersion(version));
        }
        let room = RoomId(r.take::<16>()?);
        let sender = DeviceId(r.take::<32>()?);
        let epoch = u64::from_be_bytes(r.take::<8>()?);
        let seq = u64::from_be_bytes(r.take::<8>()?);
        let nonce = r.take::<24>()?;
        let ct_len = u32::from_be_bytes(r.take::<4>()?) as usize;
        if ct_len != bytes.len() - OVERHEAD {
            return Err(ProtoError::Malformed("ciphertext length does not match envelope size"));
        }
        let ciphertext = bytes[r.o..r.o + ct_len].to_vec();
        r.o += ct_len;
        let signature = Sig(r.take::<64>()?);
        Ok(Self { version, room, sender, epoch, seq, nonce, ciphertext, signature })
    }

    pub fn current_version() -> u16 {
        PROTOCOL_VERSION
    }
}

struct Reader<'a> {
    b: &'a [u8],
    o: usize,
}

impl Reader<'_> {
    fn take<const N: usize>(&mut self) -> Result<[u8; N]> {
        let end = self.o.checked_add(N).ok_or(ProtoError::Malformed("overflow"))?;
        let s = self.b.get(self.o..end).ok_or(ProtoError::Malformed("truncated envelope"))?;
        self.o = end;
        let mut a = [0u8; N];
        a.copy_from_slice(s);
        Ok(a)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    fn sample(ct: Vec<u8>) -> Envelope {
        Envelope {
            version: PROTOCOL_VERSION,
            room: RoomId([1; 16]),
            sender: DeviceId([2; 32]),
            epoch: 3,
            seq: 4,
            nonce: [5; 24],
            ciphertext: ct,
            signature: Sig([6; 64]),
        }
    }

    #[test]
    fn round_trip() {
        let e = sample(vec![9; 100]);
        assert_eq!(Envelope::decode(&e.encode()).unwrap(), e);
    }

    #[test]
    fn rejects_oversize_and_truncated() {
        assert!(matches!(Envelope::decode(&vec![0; MAX_ENVELOPE + 1]), Err(ProtoError::TooLarge { .. })));
        let enc = sample(vec![1; 10]).encode();
        assert!(Envelope::decode(&enc[..enc.len() - 1]).is_err());
    }

    proptest! {
        #[test]
        fn decode_never_panics(data in proptest::collection::vec(any::<u8>(), 0..512)) {
            let _ = Envelope::decode(&data);
        }

        #[test]
        fn decode_never_panics_with_valid_prefix(tail in proptest::collection::vec(any::<u8>(), 0..512)) {
            let mut data = MAGIC.to_vec();
            data.extend_from_slice(&PROTOCOL_VERSION.to_be_bytes());
            data.extend_from_slice(&tail);
            let _ = Envelope::decode(&data);
        }

        #[test]
        fn round_trip_any(ct in proptest::collection::vec(any::<u8>(), 0..2048), epoch: u64, seq: u64) {
            let mut e = sample(ct);
            e.epoch = epoch;
            e.seq = seq;
            prop_assert_eq!(Envelope::decode(&e.encode()).unwrap(), e);
        }
    }
}
