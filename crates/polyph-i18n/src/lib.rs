//! Fluent localization for Polyph's Rust surfaces. The `.ftl` files in
//! `locales/` are the single source for every surface.

use std::borrow::Cow;

use fluent_bundle::concurrent::FluentBundle;
use fluent_bundle::{FluentArgs, FluentResource};
use fluent_langneg::{NegotiationStrategy, negotiate_languages};
use unic_langid::LanguageIdentifier;

include!(concat!(env!("OUT_DIR"), "/locales.rs"));

pub const FALLBACK: &str = "en";

/// Locales compiled into this binary.
pub fn available() -> Vec<&'static str> {
    LOCALES.iter().map(|(l, _)| *l).collect()
}

pub struct Localizer {
    lang: String,
    bundle: FluentBundle<FluentResource>,
    fallback: FluentBundle<FluentResource>,
}

impl std::fmt::Debug for Localizer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Localizer({})", self.lang)
    }
}

fn bundle_for(lang: &str) -> FluentBundle<FluentResource> {
    let id: LanguageIdentifier = lang.parse().unwrap_or_default();
    let mut b = FluentBundle::new_concurrent(vec![id]);
    // Terminals render the bidi isolation marks as visible junk.
    b.set_use_isolating(false);
    if let Some((_, files)) = LOCALES.iter().find(|(l, _)| *l == lang) {
        for src in *files {
            match FluentResource::try_new((*src).to_string()) {
                Ok(r) => {
                    let _ = b.add_resource(r);
                }
                Err((r, _)) => {
                    let _ = b.add_resource(r);
                }
            }
        }
    }
    b
}

/// Picks the best available locale for the requested ones (most preferred first).
pub fn negotiate(requested: &[String]) -> String {
    let req: Vec<LanguageIdentifier> = requested.iter().filter_map(|r| normalize(r).parse().ok()).collect();
    let avail: Vec<LanguageIdentifier> = available().iter().filter_map(|a| a.parse().ok()).collect();
    let default: LanguageIdentifier = FALLBACK.parse().unwrap_or_default();
    let got = negotiate_languages(&req, &avail, Some(&default), NegotiationStrategy::Filtering);
    got.first().map(|l| l.to_string()).unwrap_or_else(|| FALLBACK.to_string())
}

/// Normalizes OS locale strings like `th_TH.UTF-8` or `zh-CN` to BCP 47 forms we ship.
fn normalize(s: &str) -> String {
    let base = s.split(['.', '@']).next().unwrap_or(s).replace('_', "-");
    let lower = base.to_lowercase();
    if lower.starts_with("zh") {
        // Simplified Chinese is all we ship; map regions that use it.
        if lower.contains("hant") || lower.ends_with("-tw") || lower.ends_with("-hk") || lower.ends_with("-mo") {
            return "zh-Hant".into();
        }
        return "zh-Hans".into();
    }
    base
}

/// Locale order for this process: explicit choice, then `POLYPH_LANG`, then the OS.
pub fn detect(explicit: Option<&str>) -> String {
    let mut req = vec![];
    if let Some(e) = explicit {
        req.push(e.to_string());
    }
    if let Ok(e) = std::env::var("POLYPH_LANG") {
        req.push(e);
    }
    req.extend(sys_locale::get_locales());
    negotiate(&req)
}

impl Localizer {
    pub fn new(lang: &str) -> Self {
        let lang = negotiate(&[lang.to_string()]);
        Self { bundle: bundle_for(&lang), fallback: bundle_for(FALLBACK), lang }
    }

    pub fn lang(&self) -> &str {
        &self.lang
    }

    /// Looks up `key` with arguments, falling back to English, then to the key itself.
    pub fn t_args(&self, key: &str, args: &[(&str, FluentValue<'_>)]) -> String {
        let mut fa = FluentArgs::new();
        for (k, v) in args {
            fa.set(*k, v.clone());
        }
        for b in [&self.bundle, &self.fallback] {
            if let Some(msg) = b.get_message(key)
                && let Some(p) = msg.value()
            {
                let mut errs = vec![];
                let s: Cow<'_, str> = b.format_pattern(p, Some(&fa), &mut errs);
                return s.into_owned();
            }
        }
        key.to_string()
    }

    pub fn t(&self, key: &str) -> String {
        self.t_args(key, &[])
    }

    /// Whether every locale has `key` (used by tests and the CI check).
    pub fn has(&self, key: &str) -> bool {
        self.bundle.has_message(key)
    }
}

/// Convenience: `tr!(l, "key", name = value, ...)`.
#[macro_export]
macro_rules! tr {
    ($l:expr, $key:expr) => { $l.t($key) };
    ($l:expr, $key:expr, $($name:ident = $val:expr),+ $(,)?) => {
        $l.t_args($key, &[$((stringify!($name), $crate::FluentValue::from($val))),+])
    };
}

pub use fluent_bundle::FluentValue;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ships_four_locales() {
        let a = available();
        for l in ["en", "th", "ja", "zh-Hans"] {
            assert!(a.contains(&l), "{l} missing from {a:?}");
        }
    }

    #[test]
    fn negotiation() {
        assert_eq!(negotiate(&["th_TH.UTF-8".into()]), "th");
        assert_eq!(negotiate(&["ja-JP".into()]), "ja");
        assert_eq!(negotiate(&["zh_CN".into()]), "zh-Hans");
        assert_eq!(negotiate(&["zh-SG".into()]), "zh-Hans");
        assert_eq!(negotiate(&["fr-FR".into(), "th".into()]), "th");
        assert_eq!(negotiate(&["de".into()]), "en");
    }

    #[test]
    fn lookup_and_fallback() {
        let th = Localizer::new("th");
        assert_eq!(th.t("role-owner"), "เจ้าของ");
        assert_eq!(th.t("no-such-key"), "no-such-key");
        let ja = Localizer::new("ja");
        assert_eq!(ja.t("state-running"), "実行中");
    }
}
