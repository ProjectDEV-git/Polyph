//! Protocol versioning. A version is `major << 8 | minor`. Peers accept the
//! same major with the current or previous minor, so every minor bump keeps
//! compatibility with the one before it.

pub const MAJOR: u8 = 1;
pub const MINOR: u8 = 0;
pub const PROTOCOL_VERSION: u16 = ((MAJOR as u16) << 8) | MINOR as u16;

const PREVIOUS_MINOR: Option<u8> = MINOR.checked_sub(1);

pub fn is_compatible(v: u16) -> bool {
    let major = (v >> 8) as u8;
    let minor = (v & 0xff) as u8;
    major == MAJOR && (minor == MINOR || Some(minor) == PREVIOUS_MINOR)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compat() {
        assert!(is_compatible(PROTOCOL_VERSION));
        assert!(!is_compatible(((MAJOR as u16 + 1) << 8) | MINOR as u16));
        assert!(!is_compatible(((MAJOR as u16) << 8) | (MINOR as u16 + 1)));
        assert!(!is_compatible(((MAJOR as u16) << 8) | 0xff));
    }
}
