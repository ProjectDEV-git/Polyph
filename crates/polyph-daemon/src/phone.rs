//! Pairing a phone: a link that opens the phone PWA with a join code, the
//! person's member id (so the phone joins as another device of theirs and can
//! answer their agents' approvals) and the room's relay. The phone still
//! knocks like anyone else; an Owner or Maintainer lets it in.

use qrcode::QrCode;
use qrcode::render::{svg, unicode};

/// Website domain for links. Set at build time; the product domain is not
/// final (see STUBS.md).
pub const DOMAIN: &str = match option_env!("POLYPH_DOMAIN") {
    Some(v) => v,
    None => "polyph.dev",
};

/// Where the phone app lives unless a caller names another address.
pub fn default_base() -> String {
    format!("https://{DOMAIN}/phone/")
}

/// Percent-encodes a value for a URL fragment parameter.
fn enc(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => out.push(b as char),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

/// The pairing link. Everything rides in the fragment, which browsers never
/// send to the server that hosts the page.
pub fn link(base: &str, code: &str, member: &str, relay: &str) -> String {
    format!("{base}#join={}&member={}&relay={}", enc(code), enc(member), enc(relay))
}

/// The link as an SVG QR code (dark modules in `currentColor`, so the page
/// themes it).
pub fn qr_svg(link: &str) -> Option<String> {
    let code = QrCode::new(link.as_bytes()).ok()?;
    Some(
        code.render::<svg::Color>()
            .min_dimensions(200, 200)
            .quiet_zone(true)
            .dark_color(svg::Color("currentColor"))
            .light_color(svg::Color("transparent"))
            .build(),
    )
}

/// The link as a QR code for a terminal (two rows per character).
pub fn qr_text(link: &str) -> Option<String> {
    let code = QrCode::new(link.as_bytes()).ok()?;
    Some(code.render::<unicode::Dense1x2>().quiet_zone(true).build())
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn links_carry_everything_in_the_fragment() {
        let l = link("https://polyph.dev/phone/", "A1B2-CDEF", "ab12", "wss://relay.example/ws");
        assert_eq!(l, "https://polyph.dev/phone/#join=A1B2-CDEF&member=ab12&relay=wss%3A%2F%2Frelay.example%2Fws");
        assert!(qr_svg(&l).unwrap().contains("<svg"));
        assert!(qr_text(&l).unwrap().lines().count() > 10);
    }
}
