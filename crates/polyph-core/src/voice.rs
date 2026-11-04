//! Voice colors. Each member gets a hue; their agents use lighter and darker
//! steps of it. Mirrors `packages/ui/src/voice.ts` exactly (see DESIGN.md).

/// Hues reserved for status colors (running, waiting, blocked, error).
pub const STATUS_HUES: [f64; 4] = [150.0, 80.0, 50.0, 25.0];
const STATUS_GAP: f64 = 18.0;
const USED_GAP: f64 = 12.0;
const START: f64 = 255.0;
const GOLDEN: f64 = 137.508;

fn hue_distance(a: f64, b: f64) -> f64 {
    let d = (a - b).rem_euclid(360.0);
    d.min(360.0 - d)
}

/// Picks the next member hue given hues already used in the room.
pub fn assign_hue(used: &[u16]) -> u16 {
    let mut best: Option<(f64, u16)> = None;
    for k in 0..720 {
        let h = (START + k as f64 * GOLDEN).rem_euclid(360.0);
        if STATUS_HUES.iter().any(|s| hue_distance(h, *s) < STATUS_GAP) {
            continue;
        }
        let nearest = used.iter().map(|u| hue_distance(h, *u as f64)).fold(f64::INFINITY, f64::min);
        if nearest >= USED_GAP {
            return h.round() as u16 % 360;
        }
        // Remember the most distant fallback in case the wheel is crowded.
        if best.is_none_or(|(d, _)| nearest > d) {
            best = Some((nearest, h.round() as u16 % 360));
        }
    }
    best.map(|(_, h)| h).unwrap_or(START as u16)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Theme {
    Dark,
    Light,
}

/// Which step of a voice: the person, or their nth agent (0-based).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Step {
    Person,
    Agent(u8),
}

fn base(theme: Theme) -> (f64, f64) {
    match theme {
        Theme::Dark => (0.76, 0.12),
        Theme::Light => (0.50, 0.13),
    }
}

fn step_offset(step: Step) -> f64 {
    match step {
        Step::Person => 0.0,
        Step::Agent(n) => match n % 4 {
            0 => 0.08,
            1 => -0.08,
            2 => 0.14,
            _ => -0.14,
        },
    }
}

/// Surface the voice text must stay readable on (`--surface-2`).
fn surface(theme: Theme) -> (f64, f64, f64) {
    match theme {
        Theme::Dark => (0.225, 0.010, 265.0),
        Theme::Light => (1.0, 0.0, 0.0),
    }
}

/// OKLCH lightness, chroma, hue for a voice step, clamped so text reaches
/// 4.5:1 contrast against the theme's card surface.
pub fn voice_oklch(hue: u16, step: Step, theme: Theme) -> (f64, f64, f64) {
    let (l0, c) = base(theme);
    let mut l = (l0 + step_offset(step)).clamp(0.2, 0.95);
    let bg = srgb_from_oklch(surface(theme));
    for _ in 0..40 {
        if contrast(srgb_from_oklch((l, c, hue as f64)), bg) >= 4.5 {
            break;
        }
        l = match theme {
            Theme::Dark => (l + 0.01).min(0.98),
            Theme::Light => (l - 0.01).max(0.05),
        };
    }
    (l, c, hue as f64)
}

pub fn voice_css(hue: u16, step: Step, theme: Theme) -> String {
    let (l, c, h) = voice_oklch(hue, step, theme);
    format!("oklch({l:.3} {c:.3} {h:.0})")
}

/// 24-bit RGB for terminals.
pub fn voice_rgb(hue: u16, step: Step, theme: Theme) -> (u8, u8, u8) {
    let (r, g, b) = srgb_from_oklch(voice_oklch(hue, step, theme));
    let q = |v: f64| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
    (q(r), q(g), q(b))
}

/// OKLCH to gamma-encoded sRGB (components may fall slightly outside 0..1).
pub fn srgb_from_oklch((l, c, h): (f64, f64, f64)) -> (f64, f64, f64) {
    let hr = h.to_radians();
    let (a, b) = (c * hr.cos(), c * hr.sin());
    let l_ = l + 0.396_337_777_4 * a + 0.215_803_757_3 * b;
    let m_ = l - 0.105_561_345_8 * a - 0.063_854_172_8 * b;
    let s_ = l - 0.089_484_177_5 * a - 1.291_485_548 * b;
    let (l3, m3, s3) = (l_.powi(3), m_.powi(3), s_.powi(3));
    let r = 4.076_741_662_1 * l3 - 3.307_711_591_3 * m3 + 0.230_969_929_2 * s3;
    let g = -1.268_438_004_6 * l3 + 2.609_757_401_1 * m3 - 0.341_319_396_5 * s3;
    let bl = -0.004_196_086_3 * l3 - 0.703_418_614_7 * m3 + 1.707_614_701 * s3;
    let enc = |x: f64| {
        let x = x.clamp(0.0, 1.0);
        if x <= 0.003_130_8 { 12.92 * x } else { 1.055 * x.powf(1.0 / 2.4) - 0.055 }
    };
    (enc(r), enc(g), enc(bl))
}

fn luminance((r, g, b): (f64, f64, f64)) -> f64 {
    let lin = |x: f64| if x <= 0.040_45 { x / 12.92 } else { ((x + 0.055) / 1.055).powf(2.4) };
    0.2126 * lin(r) + 0.7152 * lin(g) + 0.0722 * lin(b)
}

/// WCAG 2 contrast ratio.
pub fn contrast(a: (f64, f64, f64), b: (f64, f64, f64)) -> f64 {
    let (la, lb) = (luminance(a), luminance(b));
    let (hi, lo) = if la > lb { (la, lb) } else { (lb, la) };
    (hi + 0.05) / (lo + 0.05)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hues_avoid_status_and_each_other() {
        let mut used = vec![];
        for _ in 0..10 {
            let h = assign_hue(&used);
            for s in STATUS_HUES {
                assert!(hue_distance(h as f64, s) >= STATUS_GAP - 0.5, "hue {h} too close to status {s}");
            }
            for u in &used {
                assert!(hue_distance(h as f64, *u as f64) >= USED_GAP - 0.5, "hue {h} too close to {u}");
            }
            used.push(h);
        }
        assert_eq!(assign_hue(&[]), 255);
    }

    #[test]
    fn all_steps_meet_contrast_in_both_themes() {
        for h in (0..360).step_by(7) {
            for theme in [Theme::Dark, Theme::Light] {
                let bg = srgb_from_oklch(surface(theme));
                for step in [Step::Person, Step::Agent(0), Step::Agent(1), Step::Agent(2), Step::Agent(3)] {
                    let fg = srgb_from_oklch(voice_oklch(h, step, theme));
                    assert!(contrast(fg, bg) >= 4.5, "hue {h} {step:?} {theme:?}: {}", contrast(fg, bg));
                }
            }
        }
    }

    #[test]
    fn agent_steps_differ_from_person() {
        let p = voice_oklch(255, Step::Person, Theme::Dark);
        let a = voice_oklch(255, Step::Agent(0), Theme::Dark);
        let b = voice_oklch(255, Step::Agent(1), Theme::Dark);
        assert!(a.0 > p.0 && b.0 < p.0);
        assert!(voice_css(255, Step::Person, Theme::Light).starts_with("oklch(0.5"));
    }
}

/// Golden vectors shared with `packages/ui/src/voice.ts`. Run with
/// `POLYPH_UPDATE_GOLDEN=1` to rewrite the file after a deliberate change.
#[cfg(test)]
mod golden {
    use super::*;

    fn vectors() -> serde_json::Value {
        let mut hues = vec![];
        let mut used = vec![];
        for _ in 0..12 {
            let h = assign_hue(&used);
            hues.push(h);
            used.push(h);
        }
        let mut colors = vec![];
        for h in (0u16..360).step_by(13) {
            for (tn, theme) in [("dark", Theme::Dark), ("light", Theme::Light)] {
                for (sn, step) in [
                    (serde_json::Value::Null, Step::Person),
                    (0.into(), Step::Agent(0)),
                    (1.into(), Step::Agent(1)),
                    (2.into(), Step::Agent(2)),
                    (3.into(), Step::Agent(3)),
                ] {
                    colors.push(serde_json::json!([h, sn, tn, voice_css(h, step, theme)]));
                }
            }
        }
        serde_json::json!({ "hues": hues, "colors": colors })
    }

    #[test]
    fn matches_the_typescript_vectors() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../packages/ui/test/voice-golden.json");
        let want = vectors();
        if std::env::var_os("POLYPH_UPDATE_GOLDEN").is_some() {
            std::fs::write(&path, serde_json::to_string_pretty(&want).unwrap() + "\n").unwrap();
        }
        // Compare values, not text: key order depends on serde_json features.
        let have: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap_or_default()).unwrap_or_default();
        assert_eq!(have, want, "voice vectors changed; rerun with POLYPH_UPDATE_GOLDEN=1 and check voice.ts");
    }
}
