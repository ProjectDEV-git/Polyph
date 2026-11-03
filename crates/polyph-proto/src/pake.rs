//! The join handshake: symmetric SPAKE2 over Ed25519 keyed by the full join
//! code, explicit key confirmation, then an encrypted channel.
//!
//! The transcript binds the rendezvous id, both PAKE messages, the role of
//! each side and an optional channel binding (the LAN relay's TLS certificate
//! fingerprint), so a man in the middle without the code cannot complete it.

use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{XChaCha20Poly1305, XNonce};
use hkdf::Hkdf;
use hmac::{Hmac, Mac};
use sha2::Sha256;
use spake2::{Ed25519Group, Identity, Password, Spake2};
use subtle::ConstantTimeEq;
use zeroize::Zeroizing;

use crate::error::{ProtoError, Result};
use crate::joincode::JoinCode;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PakeRole {
    Host,
    Joiner,
}

pub struct PakeStart {
    state: Spake2<Ed25519Group>,
    role: PakeRole,
    outbound: Vec<u8>,
    rendezvous: [u8; 16],
    binding: Vec<u8>,
}

impl std::fmt::Debug for PakeStart {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "PakeStart({:?})", self.role)
    }
}

/// Keys after the PAKE, before the peer's confirmation is checked.
pub struct PakeFinished {
    my_confirm: [u8; 32],
    peer_confirm: [u8; 32],
    channel: SecureChannel,
}

impl std::fmt::Debug for PakeFinished {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("PakeFinished")
    }
}

impl PakeStart {
    /// `binding` is extra data both sides must agree on (for example the LAN
    /// relay's certificate fingerprint); pass an empty slice when unused.
    pub fn new(code: &JoinCode, role: PakeRole, binding: &[u8]) -> Self {
        let rendezvous = code.rendezvous().0;
        let mut ident = b"polyph/v1/join/".to_vec();
        ident.extend_from_slice(&rendezvous);
        let (state, outbound) =
            Spake2::<Ed25519Group>::start_symmetric(&Password::new(code.password()), &Identity::new(&ident));
        Self { state, role, outbound, rendezvous, binding: binding.to_vec() }
    }

    pub fn message(&self) -> &[u8] {
        &self.outbound
    }

    pub fn finish(self, peer_message: &[u8]) -> Result<PakeFinished> {
        if peer_message.len() > 128 {
            return Err(ProtoError::TooLarge { len: peer_message.len(), max: 128 });
        }
        let shared = Zeroizing::new(self.state.finish(peer_message).map_err(|_| ProtoError::PakeFailed)?);
        let (host_msg, joiner_msg) = match self.role {
            PakeRole::Host => (self.outbound.as_slice(), peer_message),
            PakeRole::Joiner => (peer_message, self.outbound.as_slice()),
        };
        let mut transcript = Vec::new();
        for part in [&self.rendezvous[..], host_msg, joiner_msg, &self.binding] {
            transcript.extend_from_slice(&(part.len() as u32).to_be_bytes());
            transcript.extend_from_slice(part);
        }
        let hk = Hkdf::<Sha256>::new(Some(&transcript), shared.as_ref());
        let expand = |label: &[u8]| -> Result<Zeroizing<[u8; 32]>> {
            let mut k = Zeroizing::new([0u8; 32]);
            hk.expand(label, k.as_mut()).map_err(|_| ProtoError::PakeFailed)?;
            Ok(k)
        };
        let ck_host = expand(b"polyph/v1/join/confirm/host")?;
        let ck_joiner = expand(b"polyph/v1/join/confirm/joiner")?;
        let k_h2j = expand(b"polyph/v1/join/channel/h2j")?;
        let k_j2h = expand(b"polyph/v1/join/channel/j2h")?;
        let tag = |k: &[u8; 32]| -> Result<[u8; 32]> {
            let mut m = <Hmac<Sha256> as Mac>::new_from_slice(k).map_err(|_| ProtoError::PakeFailed)?;
            m.update(&transcript);
            Ok(m.finalize().into_bytes().into())
        };
        let host_tag = tag(&ck_host)?;
        let joiner_tag = tag(&ck_joiner)?;
        let (my_confirm, peer_confirm, send, recv) = match self.role {
            PakeRole::Host => (host_tag, joiner_tag, k_h2j, k_j2h),
            PakeRole::Joiner => (joiner_tag, host_tag, k_j2h, k_h2j),
        };
        Ok(PakeFinished { my_confirm, peer_confirm, channel: SecureChannel { send, recv, send_ctr: 0, recv_ctr: 0 } })
    }
}

impl PakeFinished {
    /// The confirmation tag to send to the peer.
    pub fn confirmation(&self) -> [u8; 32] {
        self.my_confirm
    }

    /// Checks the peer's tag in constant time; a mismatch means the codes differed.
    pub fn verify_peer(self, tag: &[u8]) -> Result<SecureChannel> {
        if tag.len() == 32 && bool::from(tag.ct_eq(&self.peer_confirm)) {
            Ok(self.channel)
        } else {
            Err(ProtoError::PakeFailed)
        }
    }
}

/// Ordered, encrypted channel derived from the PAKE. Messages must be opened
/// in the order they were sealed.
pub struct SecureChannel {
    send: Zeroizing<[u8; 32]>,
    recv: Zeroizing<[u8; 32]>,
    send_ctr: u64,
    recv_ctr: u64,
}

impl std::fmt::Debug for SecureChannel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "SecureChannel(sent {}, received {})", self.send_ctr, self.recv_ctr)
    }
}

fn nonce_for(ctr: u64) -> [u8; 24] {
    let mut n = [0u8; 24];
    n[16..].copy_from_slice(&ctr.to_be_bytes());
    n
}

impl SecureChannel {
    pub fn seal(&mut self, plaintext: &[u8]) -> Result<Vec<u8>> {
        let c = XChaCha20Poly1305::new_from_slice(self.send.as_ref()).map_err(|_| ProtoError::PakeFailed)?;
        let n = nonce_for(self.send_ctr);
        self.send_ctr += 1;
        c.encrypt(XNonce::from_slice(&n), Payload { msg: plaintext, aad: b"polyph/v1/join" })
            .map_err(|_| ProtoError::Encode("aead".into()))
    }

    pub fn open(&mut self, ciphertext: &[u8]) -> Result<Vec<u8>> {
        let c = XChaCha20Poly1305::new_from_slice(self.recv.as_ref()).map_err(|_| ProtoError::PakeFailed)?;
        let n = nonce_for(self.recv_ctr);
        let pt = c
            .decrypt(XNonce::from_slice(&n), Payload { msg: ciphertext, aad: b"polyph/v1/join" })
            .map_err(|_| ProtoError::DecryptFailed)?;
        self.recv_ctr += 1;
        Ok(pt)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(host_code: &str, joiner_code: &str, hb: &[u8], jb: &[u8]) -> Result<(SecureChannel, SecureChannel)> {
        let h = PakeStart::new(&JoinCode::parse(host_code)?, PakeRole::Host, hb);
        let j = PakeStart::new(&JoinCode::parse(joiner_code)?, PakeRole::Joiner, jb);
        let hm = h.message().to_vec();
        let jm = j.message().to_vec();
        let hf = h.finish(&jm)?;
        let jf = j.finish(&hm)?;
        let ht = hf.confirmation();
        let jt = jf.confirmation();
        Ok((hf.verify_peer(&jt)?, jf.verify_peer(&ht)?))
    }

    #[test]
    fn same_code_connects() {
        let (mut h, mut j) = run("A1B2-CDEF", "a1b2cdef", b"", b"").unwrap();
        let c = j.seal(b"join request").unwrap();
        assert_eq!(h.open(&c).unwrap(), b"join request");
        let c = h.seal(b"welcome").unwrap();
        assert_eq!(j.open(&c).unwrap(), b"welcome");
    }

    #[test]
    fn wrong_code_fails() {
        assert!(run("A1B2-CDEF", "A1B2-CDEG", b"", b"").is_err());
    }

    #[test]
    fn mismatched_binding_fails() {
        assert!(run("A1B2-CDEF", "A1B2-CDEF", b"fingerprint-a", b"fingerprint-b").is_err());
    }

    #[test]
    fn channel_rejects_reordering() {
        let (mut h, mut j) = run("A1B2-CDEF", "A1B2-CDEF", b"", b"").unwrap();
        let c1 = j.seal(b"1").unwrap();
        let c2 = j.seal(b"2").unwrap();
        assert!(h.open(&c2).is_err());
        assert_eq!(h.open(&c1).unwrap(), b"1");
        assert_eq!(h.open(&c2).unwrap(), b"2");
    }
}
