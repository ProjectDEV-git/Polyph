//! Minimal byte-buffer serde helper so byte vectors encode as CBOR byte
//! strings rather than arrays of integers.

use std::fmt;

use serde::de::{self, SeqAccess, Visitor};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

#[derive(Clone, PartialEq, Eq, Default)]
pub struct ByteBuf(pub Vec<u8>);

impl fmt::Debug for ByteBuf {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "ByteBuf({} bytes)", self.0.len())
    }
}

impl Serialize for ByteBuf {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_bytes(&self.0)
    }
}

struct BufVisitor;

impl<'de> Visitor<'de> for BufVisitor {
    type Value = ByteBuf;
    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("a byte string")
    }
    fn visit_bytes<E: de::Error>(self, v: &[u8]) -> Result<ByteBuf, E> {
        Ok(ByteBuf(v.to_vec()))
    }
    fn visit_byte_buf<E: de::Error>(self, v: Vec<u8>) -> Result<ByteBuf, E> {
        Ok(ByteBuf(v))
    }
    fn visit_str<E: de::Error>(self, v: &str) -> Result<ByteBuf, E> {
        use base64::Engine;
        base64::engine::general_purpose::STANDARD.decode(v).map(ByteBuf).map_err(E::custom)
    }
    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<ByteBuf, A::Error> {
        let mut out = Vec::with_capacity(seq.size_hint().unwrap_or(0).min(4096));
        while let Some(b) = seq.next_element::<u8>()? {
            out.push(b);
        }
        Ok(ByteBuf(out))
    }
}

impl<'de> Deserialize<'de> for ByteBuf {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        d.deserialize_byte_buf(BufVisitor)
    }
}

/// `#[serde(with = "polyph_proto::bytes::vec")]` for `Vec<u8>` fields:
/// raw bytes in CBOR, base64 in JSON.
pub mod vec {
    use super::*;

    pub fn serialize<S: Serializer>(v: &[u8], s: S) -> Result<S::Ok, S::Error> {
        if s.is_human_readable() {
            use base64::Engine;
            s.serialize_str(&base64::engine::general_purpose::STANDARD.encode(v))
        } else {
            s.serialize_bytes(v)
        }
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<u8>, D::Error> {
        if d.is_human_readable() {
            use base64::Engine;
            let s = String::deserialize(d)?;
            base64::engine::general_purpose::STANDARD.decode(s).map_err(de::Error::custom)
        } else {
            Ok(ByteBuf::deserialize(d)?.0)
        }
    }
}

/// Same as [`vec`] for fixed 32-byte arrays.
pub mod arr32 {
    use super::*;

    pub fn serialize<S: Serializer>(v: &[u8; 32], s: S) -> Result<S::Ok, S::Error> {
        super::vec::serialize(v, s)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<[u8; 32], D::Error> {
        let v = super::vec::deserialize(d)?;
        v.as_slice().try_into().map_err(|_| de::Error::custom("expected 32 bytes"))
    }
}
