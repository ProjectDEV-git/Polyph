//! Delivery of epoch keys to member devices with HPKE (RFC 9180), base mode,
//! DHKEM(X25519, HKDF-SHA256), HKDF-SHA256, ChaCha20-Poly1305. The sealed key
//! is wrapped in a delivery record signed by the acting host.

use hpke::aead::ChaCha20Poly1305;
use hpke::kdf::HkdfSha256;
use hpke::kem::X25519HkdfSha256;
use hpke::{Deserializable, Kem as KemTrait, OpModeR, OpModeS, Serializable};
use rand_core_09::TryRngCore;
use serde::{Deserialize, Serialize};

use crate::error::{ProtoError, Result};
use crate::group::EpochKey;
use crate::identity::{DeviceIdentity, DevicePublic, ctx};
use crate::ids::{DeviceId, RoomId};
use crate::signed::{SignContext, Signed};

type Kem = X25519HkdfSha256;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct EpochKeyDelivery {
    pub room: RoomId,
    pub epoch: u64,
    pub recipient: DeviceId,
    #[serde(with = "crate::bytes::vec")]
    pub encapped: Vec<u8>,
    #[serde(with = "crate::bytes::vec")]
    pub sealed: Vec<u8>,
}

impl SignContext for EpochKeyDelivery {
    const CONTEXT: &'static str = ctx::KEY_DELIVERY;
}

fn info(room: &RoomId, epoch: u64) -> Vec<u8> {
    let mut v = b"polyph/v1/epoch-key".to_vec();
    v.extend_from_slice(&room.0);
    v.extend_from_slice(&epoch.to_be_bytes());
    v
}

/// Seals `key` to `recipient` and signs the delivery as `host`.
pub fn seal_epoch_key(
    host: &DeviceIdentity,
    recipient: &DevicePublic,
    key: &EpochKey,
) -> Result<Signed<EpochKeyDelivery>> {
    let pk =
        <Kem as KemTrait>::PublicKey::from_bytes(&recipient.kx).map_err(|_| ProtoError::Malformed("recipient key"))?;
    let (encapped, sealed) = hpke::single_shot_seal::<ChaCha20Poly1305, HkdfSha256, Kem, _>(
        &OpModeS::Base,
        &pk,
        &info(&key.room, key.epoch),
        key.secret_bytes(),
        &recipient.sign.0,
        &mut rand_core_09::OsRng.unwrap_err(),
    )
    .map_err(|_| ProtoError::Encode("hpke seal".into()))?;
    let d = EpochKeyDelivery {
        room: key.room,
        epoch: key.epoch,
        recipient: recipient.sign,
        encapped: encapped.to_bytes().to_vec(),
        sealed,
    };
    Signed::sign(host, &d)
}

/// Opens a delivery addressed to `me`. The caller must check that the signer
/// is the current acting host with a role that may rotate keys.
pub fn open_epoch_key(me: &DeviceIdentity, delivery: &Signed<EpochKeyDelivery>) -> Result<EpochKey> {
    let d = delivery.open()?;
    if d.recipient != me.device_id() {
        return Err(ProtoError::Malformed("delivery addressed to another device"));
    }
    let sk_bytes = me.kx_secret_bytes();
    let sk =
        <Kem as KemTrait>::PrivateKey::from_bytes(sk_bytes.as_ref()).map_err(|_| ProtoError::Malformed("own key"))?;
    let enc =
        <Kem as KemTrait>::EncappedKey::from_bytes(&d.encapped).map_err(|_| ProtoError::Malformed("encapped key"))?;
    let pt = zeroize::Zeroizing::new(
        hpke::single_shot_open::<ChaCha20Poly1305, HkdfSha256, Kem>(
            &OpModeR::Base,
            &sk,
            &enc,
            &info(&d.room, d.epoch),
            &d.sealed,
            &d.recipient.0,
        )
        .map_err(|_| ProtoError::DecryptFailed)?,
    );
    let key: [u8; 32] = pt.as_slice().try_into().map_err(|_| ProtoError::Malformed("epoch key length"))?;
    Ok(EpochKey::from_bytes(d.room, d.epoch, key))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn delivery_round_trip_and_wrong_recipient() {
        let host = DeviceIdentity::generate().unwrap();
        let alice = DeviceIdentity::generate().unwrap();
        let bob = DeviceIdentity::generate().unwrap();
        let key = EpochKey::generate(RoomId([4; 16]), 9).unwrap();
        let d = seal_epoch_key(&host, &alice.public(), &key).unwrap();
        assert_eq!(d.signer, host.device_id());
        let got = open_epoch_key(&alice, &d).unwrap();
        assert_eq!(got.secret_bytes(), key.secret_bytes());
        assert_eq!(got.epoch, 9);
        assert!(open_epoch_key(&bob, &d).is_err());
    }
}
