//! Room genesis, membership certificates and role chains.
//!
//! A room's id is the hash of its signed genesis record, which names the
//! owner device. Every other member holds a chain of certificates leading
//! back to that owner. Display names are not in certificates: a certificate
//! commits to the name with a salted hash so the relay, which may verify
//! chains, never learns names.

use serde::{Deserialize, Serialize};

use crate::codec::to_cbor;
use crate::error::{ProtoError, Result};
use crate::identity::{DeviceIdentity, DevicePublic, ctx};
use crate::ids::{DeviceId, MemberId, RoomId};
use crate::signed::{SignContext, Signed};

/// Roles in increasing order of authority.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    Viewer,
    Member,
    Maintainer,
    Owner,
}

impl Role {
    pub fn can_approve_joins(self) -> bool {
        self >= Role::Maintainer
    }
    pub fn can_issue_certs(self) -> bool {
        self >= Role::Maintainer
    }
    pub fn can_rotate_keys(self) -> bool {
        self >= Role::Maintainer
    }
    /// Approving agent actions (subject to the per-request owner-first routing).
    pub fn can_approve_agents(self) -> bool {
        self >= Role::Member
    }
    pub fn can_write(self) -> bool {
        self >= Role::Member
    }
    pub fn as_str(self) -> &'static str {
        match self {
            Role::Viewer => "viewer",
            Role::Member => "member",
            Role::Maintainer => "maintainer",
            Role::Owner => "owner",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoomGenesis {
    pub protocol: u16,
    pub owner: DevicePublic,
    pub owner_member: MemberId,
    pub created_at: i64,
    /// Quick sessions are throwaway rooms deleted when the host ends them.
    pub quick: bool,
    #[serde(with = "crate::bytes::arr32")]
    pub nonce: [u8; 32],
}

impl SignContext for RoomGenesis {
    const CONTEXT: &'static str = ctx::GENESIS;
}

/// Derives a room id from a signed genesis: hash of its signed body bytes.
pub fn room_id_for(genesis: &Signed<RoomGenesis>) -> RoomId {
    let h = blake3::derive_key("polyph v1 room id", &genesis.body);
    let mut id = [0u8; 16];
    id.copy_from_slice(&h[..16]);
    RoomId(id)
}

impl RoomGenesis {
    pub fn create(
        owner: &DeviceIdentity,
        owner_member: MemberId,
        created_at: i64,
        quick: bool,
    ) -> Result<(Signed<RoomGenesis>, RoomId)> {
        let mut nonce = [0u8; 32];
        getrandom::getrandom(&mut nonce).map_err(|_| ProtoError::Rng)?;
        let g = RoomGenesis {
            protocol: crate::version::PROTOCOL_VERSION,
            owner: owner.public(),
            owner_member,
            created_at,
            quick,
            nonce,
        };
        let signed = Signed::sign(owner, &g)?;
        let id = room_id_for(&signed);
        Ok((signed, id))
    }
}

/// Opens a signed genesis and checks it is self-signed by its owner.
pub fn open_genesis(g: &Signed<RoomGenesis>) -> Result<(RoomGenesis, RoomId)> {
    let body = g.open()?;
    if body.owner.sign != g.signer {
        return Err(ProtoError::InvalidChain("genesis not signed by its owner"));
    }
    Ok((body, room_id_for(g)))
}

/// Salted commitment to a display name.
pub fn name_commitment(name: &str, salt: &[u8; 16]) -> [u8; 32] {
    let mut h = blake3::Hasher::new_derive_key("polyph v1 display name");
    h.update(salt);
    h.update(name.as_bytes());
    *h.finalize().as_bytes()
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MembershipCert {
    pub room: RoomId,
    pub member: MemberId,
    pub device: DevicePublic,
    #[serde(with = "crate::bytes::arr32")]
    pub name_commit: [u8; 32],
    pub role: Role,
    pub issued_at: i64,
    pub expires_at: Option<i64>,
    /// Device capabilities, e.g. "agent-host" for a cloud agent server.
    #[serde(default)]
    pub capabilities: Vec<String>,
}

impl SignContext for MembershipCert {
    const CONTEXT: &'static str = ctx::CERT;
}

/// A device's proof of membership: the room genesis plus certificates from
/// the owner down to the device.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CertChain {
    pub genesis: Signed<RoomGenesis>,
    pub certs: Vec<Signed<MembershipCert>>,
}

pub const MAX_CHAIN: usize = 8;

/// What a verified chain proves.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VerifiedMember {
    pub room: RoomId,
    pub member: MemberId,
    pub device: DevicePublic,
    pub role: Role,
    pub name_commit: [u8; 32],
    /// When the leaf certificate was issued; newer certificates supersede older ones.
    pub issued_at: i64,
    pub expires_at: Option<i64>,
    pub capabilities: Vec<String>,
    pub owner: DeviceId,
}

impl CertChain {
    /// Issues a certificate from `issuer` (whose own chain is `self`) for a new device.
    pub fn issue(&self, issuer: &DeviceIdentity, cert: &MembershipCert) -> Result<CertChain> {
        let mut certs = self.certs.clone();
        certs.push(Signed::sign(issuer, cert)?);
        Ok(CertChain { genesis: self.genesis.clone(), certs })
    }

    /// Owner's own chain: genesis plus a self-issued Owner certificate.
    pub fn for_owner(
        owner: &DeviceIdentity,
        genesis: Signed<RoomGenesis>,
        member: MemberId,
        name_commit: [u8; 32],
        now: i64,
    ) -> Result<CertChain> {
        let room = room_id_for(&genesis);
        let cert = MembershipCert {
            room,
            member,
            device: owner.public(),
            name_commit,
            role: Role::Owner,
            issued_at: now,
            expires_at: None,
            capabilities: vec![],
        };
        Ok(CertChain { genesis, certs: vec![Signed::sign(owner, &cert)?] })
    }

    /// Verifies the whole chain. `revoked` reports devices removed from the room.
    pub fn verify(&self, now: i64, revoked: &dyn Fn(&DeviceId) -> bool) -> Result<VerifiedMember> {
        if self.certs.is_empty() || self.certs.len() > MAX_CHAIN {
            return Err(ProtoError::InvalidChain("chain length out of range"));
        }
        let (genesis, room) = open_genesis(&self.genesis)?;
        let mut issuer_dev = genesis.owner.sign;
        let mut issuer_role = Role::Owner;
        let mut last: Option<MembershipCert> = None;
        for (i, signed) in self.certs.iter().enumerate() {
            if signed.signer != issuer_dev {
                return Err(ProtoError::InvalidChain("certificate not signed by the previous link"));
            }
            if i > 0 && !issuer_role.can_issue_certs() {
                return Err(ProtoError::InvalidChain("issuer role cannot issue certificates"));
            }
            if revoked(&issuer_dev) {
                return Err(ProtoError::InvalidChain("issuer was removed"));
            }
            let c = signed.open()?;
            if c.room != room {
                return Err(ProtoError::InvalidChain("certificate for another room"));
            }
            if c.role > issuer_role {
                return Err(ProtoError::InvalidChain("certificate grants more than its issuer holds"));
            }
            if c.expires_at.is_some_and(|e| e <= now) {
                return Err(ProtoError::InvalidChain("certificate expired"));
            }
            if c.issued_at > now + 300 {
                return Err(ProtoError::InvalidChain("certificate issued in the future"));
            }
            // Only the owner's first certificate is its own issuer; a device
            // re-certifying itself could otherwise restamp an old role as new.
            if i > 0 && c.device.sign == issuer_dev {
                return Err(ProtoError::InvalidChain("certificate issued by its own device"));
            }
            issuer_dev = c.device.sign;
            issuer_role = c.role;
            last = Some(c);
        }
        let c = last.ok_or(ProtoError::InvalidChain("empty chain"))?;
        if revoked(&c.device.sign) {
            return Err(ProtoError::InvalidChain("device was removed"));
        }
        Ok(VerifiedMember {
            room,
            member: c.member,
            device: c.device,
            role: c.role,
            name_commit: c.name_commit,
            issued_at: c.issued_at,
            expires_at: c.expires_at,
            capabilities: c.capabilities,
            owner: genesis.owner.sign,
        })
    }

    pub fn room(&self) -> RoomId {
        room_id_for(&self.genesis)
    }

    pub fn encoded_len(&self) -> usize {
        to_cbor(self).map(|v| v.len()).unwrap_or(usize::MAX)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: i64 = 1_800_000_000;

    fn no_revoked(_: &DeviceId) -> bool {
        false
    }

    fn room() -> (DeviceIdentity, CertChain) {
        let owner = DeviceIdentity::generate().unwrap();
        let (g, _) = RoomGenesis::create(&owner, MemberId([1; 16]), NOW, false).unwrap();
        let chain = CertChain::for_owner(&owner, g, MemberId([1; 16]), [0; 32], NOW).unwrap();
        (owner, chain)
    }

    fn cert_for(chain: &CertChain, dev: &DeviceIdentity, role: Role) -> MembershipCert {
        MembershipCert {
            room: chain.room(),
            member: MemberId::random().unwrap(),
            device: dev.public(),
            name_commit: name_commitment("Mai", &[3; 16]),
            role,
            issued_at: NOW,
            expires_at: Some(NOW + 3600),
            capabilities: vec![],
        }
    }

    #[test]
    fn owner_chain_verifies() {
        let (_, chain) = room();
        let v = chain.verify(NOW, &no_revoked).unwrap();
        assert_eq!(v.role, Role::Owner);
    }

    #[test]
    fn maintainer_can_add_member_but_not_owner() {
        let (owner, chain) = room();
        let m = DeviceIdentity::generate().unwrap();
        let mchain = chain.issue(&owner, &cert_for(&chain, &m, Role::Maintainer)).unwrap();
        assert_eq!(mchain.verify(NOW, &no_revoked).unwrap().role, Role::Maintainer);

        let u = DeviceIdentity::generate().unwrap();
        let uchain = mchain.issue(&m, &cert_for(&chain, &u, Role::Member)).unwrap();
        assert_eq!(uchain.verify(NOW, &no_revoked).unwrap().role, Role::Member);

        let bad = mchain.issue(&m, &cert_for(&chain, &u, Role::Owner)).unwrap();
        assert!(bad.verify(NOW, &no_revoked).is_err());
    }

    #[test]
    fn member_cannot_issue() {
        let (owner, chain) = room();
        let m = DeviceIdentity::generate().unwrap();
        let mchain = chain.issue(&owner, &cert_for(&chain, &m, Role::Member)).unwrap();
        let u = DeviceIdentity::generate().unwrap();
        let bad = mchain.issue(&m, &cert_for(&chain, &u, Role::Viewer)).unwrap();
        assert!(bad.verify(NOW, &no_revoked).is_err());
    }

    #[test]
    fn only_the_owner_certifies_itself() {
        let (owner, chain) = room();
        let m = DeviceIdentity::generate().unwrap();
        let mchain = chain.issue(&owner, &cert_for(&chain, &m, Role::Maintainer)).unwrap();
        // A Maintainer restamping its own certificate with a later date.
        let mut again = cert_for(&chain, &m, Role::Maintainer);
        again.issued_at = NOW + 60;
        let restamped = mchain.issue(&m, &again).unwrap();
        assert!(restamped.verify(NOW + 60, &no_revoked).is_err());
        // The owner's own chain starts with a self-issued certificate.
        assert!(chain.verify(NOW, &no_revoked).is_ok());
    }

    #[test]
    fn expiry_revocation_and_forgery() {
        let (owner, chain) = room();
        let m = DeviceIdentity::generate().unwrap();
        let mchain = chain.issue(&owner, &cert_for(&chain, &m, Role::Member)).unwrap();
        assert!(mchain.verify(NOW + 7200, &no_revoked).is_err());
        let dev = m.device_id();
        assert!(mchain.verify(NOW, &|d: &DeviceId| *d == dev).is_err());

        // A stranger signing a cert in place of the owner.
        let stranger = DeviceIdentity::generate().unwrap();
        let forged = chain.issue(&stranger, &cert_for(&chain, &m, Role::Member)).unwrap();
        assert!(forged.verify(NOW, &no_revoked).is_err());

        // Splicing a cert onto a different room's genesis.
        let (_, other) = room();
        let spliced = CertChain { genesis: other.genesis.clone(), certs: mchain.certs.clone() };
        assert!(spliced.verify(NOW, &no_revoked).is_err());
    }
}
