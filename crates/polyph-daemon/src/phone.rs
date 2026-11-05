//! Pairing a phone: a link that opens the phone PWA with a join code, the
//! person's member id (so the phone joins as another device of theirs and can
//! answer their agents' approvals) and the room's relay. The phone still
//! knocks like anyone else; an Owner or Maintainer lets it in.

use qrcode::QrCode;
use qrcode::render::{svg, unicode};

/// Where the website lives, without the scheme: the project's GitHub Pages
/// site, which hosts the phone app and the join page. A build can name
/// another with `POLYPH_DOMAIN` (a host, optionally with a path).
pub const DOMAIN: &str = match option_env!("POLYPH_DOMAIN") {
    Some(v) if !v.is_empty() => v,
    _ => "projectdev-git.github.io/Polyph",
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

/// A join link for anyone: opens the phone app, which joins the room from
/// the browser as a new person. As with pairing, the code and relay ride in
/// the fragment.
pub fn join_link(code: &str, relay: &str) -> String {
    format!("{}#join={}&relay={}", default_base(), enc(code), enc(relay))
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

    #[test]
    fn join_links_open_the_phone_app_without_a_member() {
        let l = join_link("A1B2-CDEF", "wss://relay.example/ws");
        assert_eq!(l, format!("{}#join=A1B2-CDEF&relay=wss%3A%2F%2Frelay.example%2Fws", default_base()));
        assert!(!l.contains("member="));
    }
}
