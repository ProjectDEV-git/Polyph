//! Join codes: 8 Crockford base32 characters shown as `A1B2-CDEF`.
//!
//! The relay only ever sees a hash of the first half (the rendezvous id). The
//! whole code is the PAKE password, so the relay cannot complete a join.

use std::fmt;

use crate::error::{ProtoError, Result};
use crate::ids::RendezvousId;

const ALPHABET: &[u8; 32] = b"0123456789ABCDEFGHJKMNPQRSTVWXYZ";
pub const CODE_LEN: usize = 8;

#[derive(Clone, PartialEq, Eq, Hash)]
pub struct JoinCode([u8; CODE_LEN]);

impl fmt::Debug for JoinCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Never print the second half; it is the secret part.
        write!(f, "JoinCode({}-****)", self.first_half())
    }
}

impl fmt::Display for JoinCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = self.as_str();
        write!(f, "{}-{}", &s[..4], &s[4..])
    }
}

fn decode_char(c: char) -> Option<u8> {
    let c = c.to_ascii_uppercase();
    let c = match c {
        'I' | 'L' => '1',
        'O' => '0',
        other => other,
    };
    ALPHABET.iter().position(|&a| a as char == c).map(|p| ALPHABET[p])
}

impl JoinCode {
    pub fn generate() -> Result<Self> {
        let mut out = [0u8; CODE_LEN];
        let mut rnd = [0u8; CODE_LEN];
        getrandom::getrandom(&mut rnd).map_err(|_| ProtoError::Rng)?;
        for (o, r) in out.iter_mut().zip(rnd) {
            // 256 is a multiple of 32 so this is unbiased.
            *o = ALPHABET[(r & 31) as usize];
        }
        Ok(Self(out))
    }

    /// Parses user input: case-insensitive, ignores dashes and spaces, maps
    /// the Crockford look-alikes I, L to 1 and O to 0, and rejects U.
    pub fn parse(input: &str) -> Result<Self> {
        let mut out = [0u8; CODE_LEN];
        let mut n = 0;
        for ch in input.chars() {
            if ch == '-' || ch.is_whitespace() {
                continue;
            }
            if n >= CODE_LEN {
                return Err(ProtoError::InvalidCode("too many characters"));
            }
            out[n] = decode_char(ch).ok_or(ProtoError::InvalidCode("character not in the code alphabet"))?;
            n += 1;
        }
        if n != CODE_LEN {
            return Err(ProtoError::InvalidCode("a code has 8 characters"));
        }
        Ok(Self(out))
    }

    pub fn as_str(&self) -> &str {
        // Every byte comes from ALPHABET, which is ASCII.
        std::str::from_utf8(&self.0).unwrap_or("")
    }

    pub fn first_half(&self) -> &str {
        &self.as_str()[..4]
    }

    pub fn rendezvous(&self) -> RendezvousId {
        let h = blake3::derive_key("polyph v1 rendezvous", self.first_half().as_bytes());
        let mut id = [0u8; 16];
        id.copy_from_slice(&h[..16]);
        RendezvousId(id)
    }

    pub(crate) fn password(&self) -> &[u8] {
        &self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generate_and_parse() {
        for _ in 0..100 {
            let c = JoinCode::generate().unwrap();
            let shown = c.to_string();
            assert_eq!(shown.len(), 9);
            assert_eq!(JoinCode::parse(&shown).unwrap(), c);
            assert_eq!(JoinCode::parse(&shown.to_lowercase()).unwrap(), c);
            assert!(!shown.contains(['I', 'L', 'O', 'U']));
        }
    }

    #[test]
    fn lookalikes_and_invalid() {
        assert_eq!(JoinCode::parse("ilo1-2345").unwrap().as_str(), "11012345");
        assert!(JoinCode::parse("U000-0000").is_err());
        assert!(JoinCode::parse("A1B2-CDE").is_err());
        assert!(JoinCode::parse("A1B2-CDEFG").is_err());
        assert_eq!(JoinCode::parse("a1b2 cdef").unwrap().to_string(), "A1B2-CDEF");
    }

    #[test]
    fn rendezvous_depends_only_on_first_half() {
        let a = JoinCode::parse("A1B2-CDEF").unwrap();
        let b = JoinCode::parse("A1B2-0000").unwrap();
        let c = JoinCode::parse("A1B3-CDEF").unwrap();
        assert_eq!(a.rendezvous(), b.rendezvous());
        assert_ne!(a.rendezvous(), c.rendezvous());
        assert!(!format!("{a:?}").contains("CDEF"));
    }
}
