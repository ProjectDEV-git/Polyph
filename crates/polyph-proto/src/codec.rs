//! Size-limited CBOR encoding for every structured payload.

use serde::{Serialize, de::DeserializeOwned};

use crate::error::{ProtoError, Result};

/// Upper bound for any single CBOR document Polyph decodes.
pub const MAX_DOCUMENT: usize = 4 * 1024 * 1024;
/// Nesting limit to stop stack exhaustion from crafted input.
const RECURSION_LIMIT: usize = 64;

pub fn to_cbor<T: Serialize>(value: &T) -> Result<Vec<u8>> {
    let mut out = Vec::new();
    ciborium::ser::into_writer(value, &mut out).map_err(|e| ProtoError::Encode(e.to_string()))?;
    Ok(out)
}

pub fn from_cbor<T: DeserializeOwned>(bytes: &[u8]) -> Result<T> {
    from_cbor_limited(bytes, MAX_DOCUMENT)
}

pub fn from_cbor_limited<T: DeserializeOwned>(bytes: &[u8], max: usize) -> Result<T> {
    if bytes.len() > max {
        return Err(ProtoError::TooLarge { len: bytes.len(), max });
    }
    ciborium::de::from_reader_with_recursion_limit(bytes, RECURSION_LIMIT)
        .map_err(|e| ProtoError::Decode(e.to_string()))
}
