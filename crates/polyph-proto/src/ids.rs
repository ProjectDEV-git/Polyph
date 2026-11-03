//! Fixed-size identifiers. They serialize as raw bytes in CBOR and as
//! lowercase hex in human-readable formats such as JSON.

use std::fmt;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::error::{ProtoError, Result};

macro_rules! byte_id {
    ($(#[$meta:meta])* $name:ident, $len:expr) => {
        $(#[$meta])*
        #[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
        pub struct $name(pub [u8; $len]);

        impl $name {
            pub const LEN: usize = $len;

            pub fn from_slice(bytes: &[u8]) -> Result<Self> {
                let arr: [u8; $len] = bytes
                    .try_into()
                    .map_err(|_| ProtoError::Malformed(concat!(stringify!($name), " has the wrong length")))?;
                Ok(Self(arr))
            }

            pub fn random() -> Result<Self> {
                let mut b = [0u8; $len];
                getrandom::getrandom(&mut b).map_err(|_| ProtoError::Rng)?;
                Ok(Self(b))
            }

            pub fn as_bytes(&self) -> &[u8; $len] {
                &self.0
            }

            pub fn to_hex(&self) -> String {
                hex::encode(self.0)
            }

            pub fn from_hex(s: &str) -> Result<Self> {
                let v = hex::decode(s).map_err(|_| ProtoError::Malformed("invalid hex"))?;
                Self::from_slice(&v)
            }

            /// Short form for display in logs and UIs (first 8 hex chars).
            pub fn short(&self) -> String {
                let mut s = self.to_hex();
                s.truncate(8);
                s
            }
        }

        impl fmt::Debug for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, concat!(stringify!($name), "({})"), self.short())
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(&self.to_hex())
            }
        }

        impl Serialize for $name {
            fn serialize<S: Serializer>(&self, s: S) -> std::result::Result<S::Ok, S::Error> {
                if s.is_human_readable() {
                    s.serialize_str(&self.to_hex())
                } else {
                    s.serialize_bytes(&self.0)
                }
            }
        }

        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D: Deserializer<'de>>(d: D) -> std::result::Result<Self, D::Error> {
                if d.is_human_readable() {
                    let s = String::deserialize(d)?;
                    Self::from_hex(&s).map_err(serde::de::Error::custom)
                } else {
                    let b = crate::bytes::ByteBuf::deserialize(d)?;
                    Self::from_slice(&b.0).map_err(serde::de::Error::custom)
                }
            }
        }
    };
}

byte_id!(
    /// A room, derived from the hash of its signed genesis record.
    RoomId, 16
);
byte_id!(
    /// A device: its Ed25519 public signing key.
    DeviceId, 32
);
byte_id!(
    /// A person. Random for incognito users, stable per account otherwise.
    MemberId, 16
);
byte_id!(
    /// The relay-visible half of a join code, hashed.
    RendezvousId, 16
);
byte_id!(
    /// Generic 16-byte id used for tasks, cards, approvals and messages.
    ObjectId, 16
);

/// Ed25519 signature bytes.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct Sig(pub [u8; 64]);

impl fmt::Debug for Sig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Sig({}..)", hex::encode(&self.0[..4]))
    }
}

impl Serialize for Sig {
    fn serialize<S: Serializer>(&self, s: S) -> std::result::Result<S::Ok, S::Error> {
        if s.is_human_readable() { s.serialize_str(&hex::encode(self.0)) } else { s.serialize_bytes(&self.0) }
    }
}

impl<'de> Deserialize<'de> for Sig {
    fn deserialize<D: Deserializer<'de>>(d: D) -> std::result::Result<Self, D::Error> {
        let v = if d.is_human_readable() {
            let s = String::deserialize(d)?;
            hex::decode(s).map_err(serde::de::Error::custom)?
        } else {
            crate::bytes::ByteBuf::deserialize(d)?.0
        };
        let arr: [u8; 64] =
            v.as_slice().try_into().map_err(|_| serde::de::Error::custom("signature has the wrong length"))?;
        Ok(Sig(arr))
    }
}
