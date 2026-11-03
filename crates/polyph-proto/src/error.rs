use thiserror::Error;

/// Errors produced by protocol parsing, cryptography and verification.
///
/// Messages never include key material or plaintext so they are safe to log.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum ProtoError {
    #[error("input too large: {len} bytes exceeds the {max} byte limit")]
    TooLarge { len: usize, max: usize },
    #[error("malformed input: {0}")]
    Malformed(&'static str),
    #[error("unsupported protocol version {0:#06x}")]
    UnsupportedVersion(u16),
    #[error("signature verification failed")]
    BadSignature,
    #[error("decryption failed")]
    DecryptFailed,
    #[error("message replayed or too old")]
    Replay,
    #[error("unknown epoch {0}")]
    UnknownEpoch(u64),
    #[error("certificate chain invalid: {0}")]
    InvalidChain(&'static str),
    #[error("join code invalid: {0}")]
    InvalidCode(&'static str),
    #[error("key exchange failed")]
    PakeFailed,
    #[error("encoding failed: {0}")]
    Encode(String),
    #[error("decoding failed: {0}")]
    Decode(String),
    #[error("randomness unavailable")]
    Rng,
}

pub type Result<T> = std::result::Result<T, ProtoError>;
