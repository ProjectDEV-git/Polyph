//! Long-term device identity: an Ed25519 signing key and an X25519 key
//! agreement key. Every signature is domain separated by a context label.

use ed25519_dalek::{Signature, Signer, SigningKey, VerifyingKey};
use serde::{Deserialize, Serialize};
use x25519_dalek::{PublicKey as XPublic, StaticSecret};
use zeroize::Zeroizing;

use crate::error::{ProtoError, Result};
use crate::ids::{DeviceId, Sig};

/// Prefix for every signed message, followed by the context label and a NUL.
const SIG_DOMAIN: &[u8] = b"polyph/v1/sig/";

/// Signing contexts. Using distinct labels prevents a signature made for one
/// purpose from being replayed as another.
pub mod ctx {
    pub const ENVELOPE: &str = "envelope";
    pub const GENESIS: &str = "genesis";
    pub const CERT: &str = "cert";
    pub const KEY_DELIVERY: &str = "key-delivery";
    pub const RELAY_AUTH: &str = "relay-auth";
    pub const ROOM_END: &str = "room-end";
    pub const USAGE: &str = "usage-summary";
    pub const SPECTATOR: &str = "spectator";
}

fn domain_message(context: &str, msg: &[u8]) -> Vec<u8> {
    let mut m = Vec::with_capacity(SIG_DOMAIN.len() + context.len() + 1 + msg.len());
    m.extend_from_slice(SIG_DOMAIN);
    m.extend_from_slice(context.as_bytes());
    m.push(0);
    m.extend_from_slice(msg);
    m
}

/// Public half of a device identity.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DevicePublic {
    pub sign: DeviceId,
    #[serde(with = "crate::bytes::arr32")]
    pub kx: [u8; 32],
}

/// A device's secret keys. Secrets are zeroized on drop.
pub struct DeviceIdentity {
    sign: SigningKey,
    kx: StaticSecret,
}

impl std::fmt::Debug for DeviceIdentity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "DeviceIdentity({})", self.device_id().short())
    }
}

impl DeviceIdentity {
    pub fn generate() -> Result<Self> {
        let mut seed = Zeroizing::new([0u8; 64]);
        getrandom::getrandom(seed.as_mut()).map_err(|_| ProtoError::Rng)?;
        Ok(Self::from_secret_bytes(&seed))
    }

    /// Restores an identity from 64 secret bytes (32 signing seed + 32 X25519 secret).
    pub fn from_secret_bytes(bytes: &[u8; 64]) -> Self {
        let mut s = [0u8; 32];
        let mut k = [0u8; 32];
        s.copy_from_slice(&bytes[..32]);
        k.copy_from_slice(&bytes[32..]);
        let id = Self { sign: SigningKey::from_bytes(&s), kx: StaticSecret::from(k) };
        zeroize::Zeroize::zeroize(&mut s);
        zeroize::Zeroize::zeroize(&mut k);
        id
    }

    pub fn to_secret_bytes(&self) -> Zeroizing<[u8; 64]> {
        let mut out = Zeroizing::new([0u8; 64]);
        out[..32].copy_from_slice(&self.sign.to_bytes());
        out[32..].copy_from_slice(&self.kx.to_bytes());
        out
    }

    pub fn device_id(&self) -> DeviceId {
        DeviceId(self.sign.verifying_key().to_bytes())
    }

    pub fn public(&self) -> DevicePublic {
        DevicePublic { sign: self.device_id(), kx: XPublic::from(&self.kx).to_bytes() }
    }

    pub fn sign(&self, context: &str, msg: &[u8]) -> Sig {
        Sig(self.sign.sign(&domain_message(context, msg)).to_bytes())
    }

    pub(crate) fn kx_secret_bytes(&self) -> Zeroizing<[u8; 32]> {
        Zeroizing::new(self.kx.to_bytes())
    }
}

/// Verifies a domain-separated signature made with [`DeviceIdentity::sign`].
pub fn verify(signer: &DeviceId, context: &str, msg: &[u8], sig: &Sig) -> Result<()> {
    let vk = VerifyingKey::from_bytes(&signer.0).map_err(|_| ProtoError::BadSignature)?;
    let sig = Signature::from_bytes(&sig.0);
    vk.verify_strict(&domain_message(context, msg), &sig).map_err(|_| ProtoError::BadSignature)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sign_verify_and_context_separation() {
        let id = DeviceIdentity::generate().unwrap();
        let sig = id.sign(ctx::ENVELOPE, b"hello");
        verify(&id.device_id(), ctx::ENVELOPE, b"hello", &sig).unwrap();
        assert!(verify(&id.device_id(), ctx::CERT, b"hello", &sig).is_err());
        assert!(verify(&id.device_id(), ctx::ENVELOPE, b"hellp", &sig).is_err());
    }

    #[test]
    fn secret_round_trip() {
        let id = DeviceIdentity::generate().unwrap();
        let restored = DeviceIdentity::from_secret_bytes(&id.to_secret_bytes());
        assert_eq!(id.public(), restored.public());
    }
}
