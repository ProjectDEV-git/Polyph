//! Polyph protocol: wire types, end-to-end encryption, the join handshake,
//! membership certificates, CRDT layouts and the status feed schema.
//!
//! This crate builds for native targets and for `wasm32-unknown-unknown`
//! (feature `wasm`), so browsers use the same implementation as daemons.

pub mod bytes;
pub mod cert;
pub mod codec;
#[cfg(feature = "crdt")]
pub mod crdt;
pub mod delivery;
pub mod envelope;
pub mod error;
pub mod group;
pub mod identity;
pub mod ids;
pub mod join;
pub mod joincode;
pub mod messages;
pub mod pake;
pub mod plan;
pub mod relay_wire;
pub mod signed;
pub mod status;
pub mod version;
#[cfg(feature = "wasm")]
pub mod wasm;

pub use error::{ProtoError, Result};
pub use version::PROTOCOL_VERSION;

/// Current Unix time in seconds.
pub fn now() -> i64 {
    #[cfg(all(target_arch = "wasm32", feature = "wasm"))]
    {
        (js_sys::Date::now() / 1000.0) as i64
    }
    #[cfg(not(all(target_arch = "wasm32", feature = "wasm")))]
    {
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0)
    }
}
