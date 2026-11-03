//! Group encryption under a per-epoch room key.
//!
//! Each message is encrypted with XChaCha20-Poly1305 using a key derived from
//! the epoch key, with the envelope header as associated data, and then the
//! header and ciphertext are signed by the sender's device key.

use std::collections::HashMap;

use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{XChaCha20Poly1305, XNonce};
use hkdf::Hkdf;
use sha2::Sha256;
use zeroize::Zeroizing;

use crate::envelope::{Envelope, MAX_ENVELOPE, OVERHEAD};
use crate::error::{ProtoError, Result};
use crate::identity::{DeviceIdentity, ctx, verify};
use crate::ids::{DeviceId, RoomId, Sig};
use crate::version::PROTOCOL_VERSION;

/// Largest plaintext that fits in one envelope after AEAD and framing overhead.
pub const MAX_PLAINTEXT: usize = MAX_ENVELOPE - OVERHEAD - 16;

/// A room's symmetric key for one epoch.
#[derive(Clone)]
pub struct EpochKey {
    pub room: RoomId,
    pub epoch: u64,
    key: Zeroizing<[u8; 32]>,
}

impl std::fmt::Debug for EpochKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "EpochKey(room {}, epoch {})", self.room.short(), self.epoch)
    }
}

impl EpochKey {
    pub fn generate(room: RoomId, epoch: u64) -> Result<Self> {
        let mut key = Zeroizing::new([0u8; 32]);
        getrandom::getrandom(key.as_mut()).map_err(|_| ProtoError::Rng)?;
        Ok(Self { room, epoch, key })
    }

    pub fn from_bytes(room: RoomId, epoch: u64, bytes: [u8; 32]) -> Self {
        Self { room, epoch, key: Zeroizing::new(bytes) }
    }

    pub fn secret_bytes(&self) -> &[u8; 32] {
        &self.key
    }

    fn message_cipher(&self) -> Result<XChaCha20Poly1305> {
        let hk = Hkdf::<Sha256>::new(Some(b"polyph/v1/msg-key"), self.key.as_ref());
        let mut info = Vec::with_capacity(24);
        info.extend_from_slice(&self.room.0);
        info.extend_from_slice(&self.epoch.to_be_bytes());
        let mut k = Zeroizing::new([0u8; 32]);
        hk.expand(&info, k.as_mut()).map_err(|_| ProtoError::Malformed("hkdf length"))?;
        XChaCha20Poly1305::new_from_slice(k.as_ref()).map_err(|_| ProtoError::Malformed("key length"))
    }

    /// Encrypts and signs a plaintext into an envelope.
    pub fn seal(&self, sender: &DeviceIdentity, seq: u64, plaintext: &[u8]) -> Result<Envelope> {
        if plaintext.len() > MAX_PLAINTEXT {
            return Err(ProtoError::TooLarge { len: plaintext.len(), max: MAX_PLAINTEXT });
        }
        let mut nonce = [0u8; 24];
        getrandom::getrandom(&mut nonce).map_err(|_| ProtoError::Rng)?;
        let mut env = Envelope {
            version: PROTOCOL_VERSION,
            room: self.room,
            sender: sender.device_id(),
            epoch: self.epoch,
            seq,
            nonce,
            ciphertext: Vec::new(),
            signature: Sig([0; 64]),
        };
        let aad = env.header_bytes();
        env.ciphertext = self
            .message_cipher()?
            .encrypt(XNonce::from_slice(&nonce), Payload { msg: plaintext, aad: &aad })
            .map_err(|_| ProtoError::Encode("aead".into()))?;
        env.signature = sender.sign(ctx::ENVELOPE, &env.signed_bytes());
        Ok(env)
    }

    /// Verifies the sender's signature, then decrypts. The caller must have
    /// already established that `env.sender` holds a valid membership
    /// certificate, and should run the result through a [`ReplayGuard`].
    pub fn open(&self, env: &Envelope) -> Result<Vec<u8>> {
        verify_envelope_signature(env)?;
        if env.room != self.room {
            return Err(ProtoError::Malformed("envelope is for a different room"));
        }
        if env.epoch != self.epoch {
            return Err(ProtoError::UnknownEpoch(env.epoch));
        }
        let aad = env.header_bytes();
        self.message_cipher()?
            .decrypt(XNonce::from_slice(&env.nonce), Payload { msg: &env.ciphertext, aad: &aad })
            .map_err(|_| ProtoError::DecryptFailed)
    }
}

pub fn verify_envelope_signature(env: &Envelope) -> Result<()> {
    verify(&env.sender, ctx::ENVELOPE, &env.signed_bytes(), &env.signature)
}

/// Sliding-window replay protection per (sender, epoch).
#[derive(Debug, Default)]
pub struct ReplayGuard {
    windows: HashMap<(DeviceId, u64), Window>,
}

const WINDOW: u64 = 1024;

#[derive(Debug, Default)]
struct Window {
    highest: u64,
    seen_any: bool,
    bits: Vec<u64>, // WINDOW bits, bit i => seq (highest - i) seen
}

impl Window {
    fn check_and_mark(&mut self, seq: u64) -> bool {
        if self.bits.is_empty() {
            self.bits = vec![0; (WINDOW / 64) as usize];
        }
        if !self.seen_any {
            self.seen_any = true;
            self.highest = seq;
            self.set(0);
            return true;
        }
        if seq > self.highest {
            let shift = seq - self.highest;
            self.shift(shift);
            self.highest = seq;
            self.set(0);
            return true;
        }
        let back = self.highest - seq;
        if back >= WINDOW || self.get(back) {
            return false;
        }
        self.set(back);
        true
    }

    fn get(&self, i: u64) -> bool {
        self.bits[(i / 64) as usize] & (1 << (i % 64)) != 0
    }
    fn set(&mut self, i: u64) {
        self.bits[(i / 64) as usize] |= 1 << (i % 64);
    }
    fn shift(&mut self, n: u64) {
        if n >= WINDOW {
            self.bits.iter_mut().for_each(|w| *w = 0);
            return;
        }
        // Move every bit i to i + n.
        let old = self.bits.clone();
        self.bits.iter_mut().for_each(|w| *w = 0);
        for i in 0..WINDOW - n {
            if old[(i / 64) as usize] & (1 << (i % 64)) != 0 {
                self.set(i + n);
            }
        }
    }
}

impl ReplayGuard {
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns an error if this (sender, epoch, seq) was seen before or is
    /// older than the window.
    pub fn check(&mut self, env: &Envelope) -> Result<()> {
        let w = self.windows.entry((env.sender, env.epoch)).or_default();
        if w.check_and_mark(env.seq) { Ok(()) } else { Err(ProtoError::Replay) }
    }

    /// Drops state for epochs older than `keep_from`.
    pub fn forget_before(&mut self, keep_from: u64) {
        self.windows.retain(|(_, e), _| *e >= keep_from);
    }
}

/// Opens envelopes using a set of known epoch keys plus replay protection.
#[derive(Debug, Default)]
pub struct Keyring {
    keys: HashMap<u64, EpochKey>,
    pub current: Option<u64>,
    replay: ReplayGuard,
}

impl Keyring {
    pub fn insert(&mut self, key: EpochKey) {
        let e = key.epoch;
        self.keys.insert(e, key);
        if self.current.is_none_or(|c| e > c) {
            self.current = Some(e);
        }
    }

    pub fn current_key(&self) -> Option<&EpochKey> {
        self.current.and_then(|c| self.keys.get(&c))
    }

    pub fn get(&self, epoch: u64) -> Option<&EpochKey> {
        self.keys.get(&epoch)
    }

    pub fn open(&mut self, env: &Envelope) -> Result<Vec<u8>> {
        let key = self.keys.get(&env.epoch).ok_or(ProtoError::UnknownEpoch(env.epoch))?;
        let pt = key.open(env)?;
        self.replay.check(env)?;
        Ok(pt)
    }

    /// Forget keys older than `epoch` (used after rotation once peers caught up).
    pub fn retain_from(&mut self, epoch: u64) {
        self.keys.retain(|e, _| *e >= epoch);
        self.replay.forget_before(epoch);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup() -> (DeviceIdentity, EpochKey) {
        let id = DeviceIdentity::generate().unwrap();
        let k = EpochKey::generate(RoomId([7; 16]), 1).unwrap();
        (id, k)
    }

    #[test]
    fn seal_open_round_trip() {
        let (id, k) = setup();
        let env = k.seal(&id, 1, b"hello room").unwrap();
        assert_eq!(k.open(&env).unwrap(), b"hello room");
        assert!(!env.ciphertext.windows(10).any(|w| w == b"hello room"));
    }

    #[test]
    fn tampering_is_rejected() {
        let (id, k) = setup();
        let env = k.seal(&id, 1, b"hello").unwrap();
        let mut bad = env.clone();
        bad.ciphertext[0] ^= 1;
        assert_eq!(k.open(&bad), Err(ProtoError::BadSignature));
        let mut bad = env.clone();
        bad.seq += 1;
        assert_eq!(k.open(&bad), Err(ProtoError::BadSignature));
    }

    #[test]
    fn forged_sender_is_rejected() {
        let (id, k) = setup();
        let other = DeviceIdentity::generate().unwrap();
        let mut env = k.seal(&id, 1, b"hello").unwrap();
        env.sender = other.device_id();
        assert_eq!(k.open(&env), Err(ProtoError::BadSignature));
    }

    #[test]
    fn signed_but_wrong_key_fails_decrypt() {
        let (id, k) = setup();
        let k2 = EpochKey::generate(k.room, 1).unwrap();
        let env = k2.seal(&id, 1, b"hello").unwrap();
        assert_eq!(k.open(&env), Err(ProtoError::DecryptFailed));
    }

    #[test]
    fn replay_is_rejected() {
        let (id, k) = setup();
        let mut ring = Keyring::default();
        ring.insert(k.clone());
        let e1 = k.seal(&id, 1, b"a").unwrap();
        let e3 = k.seal(&id, 3, b"c").unwrap();
        let e2 = k.seal(&id, 2, b"b").unwrap();
        ring.open(&e1).unwrap();
        ring.open(&e3).unwrap();
        ring.open(&e2).unwrap(); // reordered but fresh
        assert_eq!(ring.open(&e1), Err(ProtoError::Replay));
        assert_eq!(ring.open(&e2), Err(ProtoError::Replay));
        let far = k.seal(&id, 5000, b"z").unwrap();
        ring.open(&far).unwrap();
        let old = k.seal(&id, 4, b"old").unwrap();
        assert_eq!(ring.open(&old), Err(ProtoError::Replay));
    }

    #[test]
    fn rotated_out_epoch_cannot_be_read() {
        let (id, k1) = setup();
        let k2 = EpochKey::generate(k1.room, 2).unwrap();
        let env = k2.seal(&id, 1, b"secret after rotation").unwrap();
        // A removed member only holds k1.
        let mut ring = Keyring::default();
        ring.insert(k1);
        assert_eq!(ring.open(&env), Err(ProtoError::UnknownEpoch(2)));
    }
}
