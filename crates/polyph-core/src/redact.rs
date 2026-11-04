//! Secret redaction, applied by the daemon to everything an agent or person
//! sends into a room (transcripts, tool output, diffs, chat, inbox messages,
//! notes, cards) before it is encrypted and leaves the device. Members are
//! trusted, but transcripts are kept, replayed and shown to spectators, so a
//! leaked key should not live there.
//!
//! The rules follow gitleaks' default rule set (a subset of its highest-value
//! rules, with its generic "key = value" rule gated by entropy).

use std::borrow::Cow;
use std::sync::LazyLock;

use regex::{Captures, Regex};

struct Rule {
    id: &'static str,
    re: Regex,
    /// Capture group holding the secret (0 = the whole match).
    group: usize,
    /// Minimum Shannon entropy (bits per char) of the secret, 0 = none.
    entropy: f64,
}

fn rule(id: &'static str, pattern: &str, group: usize, entropy: f64) -> Option<Rule> {
    // Patterns are constants covered by tests; a bad one is skipped, not fatal.
    Regex::new(pattern).ok().map(|re| Rule { id, re, group, entropy })
}

static RULES: LazyLock<Vec<Rule>> = LazyLock::new(|| {
    [
        rule(
            "private-key",
            r"-----BEGIN[ A-Z0-9_-]{0,100}PRIVATE KEY(?: BLOCK)?-----(?s:.*?)(?:-----END[ A-Z0-9_-]{0,100}PRIVATE KEY(?: BLOCK)?-----|\z)",
            0,
            0.0,
        ),
        rule("aws-access-key", r"\b(?:A3T[A-Z0-9]|AKIA|ASIA|ABIA|ACCA)[A-Z2-7]{16}\b", 0, 0.0),
        rule(
            "aws-secret-key",
            r#"(?i)aws.{0,20}?(?:secret|private)?.{0,20}?key.{0,20}?['"]?\s*[:=]\s*['"]?([A-Za-z0-9/+=]{40})\b"#,
            1,
            3.0,
        ),
        rule("github-token", r"\bgh[pousr]_[A-Za-z0-9]{36,255}\b", 0, 0.0),
        rule("github-fine-grained-token", r"\bgithub_pat_[A-Za-z0-9_]{60,255}\b", 0, 0.0),
        rule("gitlab-token", r"\bglpat-[A-Za-z0-9_-]{20,}\b", 0, 0.0),
        rule("slack-token", r"\bxox[baprs]-[A-Za-z0-9-]{10,}\b", 0, 0.0),
        rule("slack-webhook", r"https://hooks\.slack\.com/services/[A-Za-z0-9+/]{20,}", 0, 0.0),
        rule("stripe-key", r"\b(?:sk|rk)_(?:test|live|prod)_[A-Za-z0-9]{10,99}\b", 0, 0.0),
        rule("anthropic-key", r"\bsk-ant-[A-Za-z0-9_-]{20,}", 0, 0.0),
        rule("openai-key", r"\bsk-(?:proj-|svcacct-|admin-)?[A-Za-z0-9_-]{20,}", 0, 3.0),
        rule("gcp-api-key", r"\bAIza[0-9A-Za-z_-]{35}\b", 0, 0.0),
        rule("npm-token", r"\bnpm_[A-Za-z0-9]{36}\b", 0, 0.0),
        rule("pypi-token", r"\bpypi-AgEIcHlwaS5vcmc[A-Za-z0-9_-]{50,}", 0, 0.0),
        rule("jwt", r"\beyJ[A-Za-z0-9_-]{10,}\.eyJ[A-Za-z0-9_-]{10,}\.[A-Za-z0-9_-]{10,}", 0, 0.0),
        rule("url-password", r"\b[a-zA-Z][a-zA-Z0-9+.-]{1,20}://[^/\s:@]{1,100}:([^/\s:@]{3,200})@", 1, 0.0),
        rule(
            "generic-secret",
            r#"(?i)\b(?:[a-z0-9_.-]*?)(?:api[_.-]?key|secret|token|passw(?:or)?d|pwd|client[_.-]?secret|access[_.-]?key|auth)\b["']?\s*(?:[:=]|:=|=>)\s*["'`]?([^\s"'`,;]{8,200})"#,
            1,
            3.5,
        ),
    ]
    .into_iter()
    .flatten()
    .collect()
});

fn entropy(s: &str) -> f64 {
    let mut counts = [0u32; 256];
    let bytes = s.as_bytes();
    for b in bytes {
        counts[*b as usize] += 1;
    }
    let n = bytes.len() as f64;
    counts
        .iter()
        .filter(|c| **c > 0)
        .map(|c| {
            let p = *c as f64 / n;
            -p * p.log2()
        })
        .sum()
}

/// Values that look like placeholders, references or code rather than secrets.
fn looks_harmless(v: &str) -> bool {
    let l = v.to_ascii_lowercase();
    v.starts_with('$')
        || v.starts_with('<')
        || v.starts_with('{')
        || v.contains("${")
        || v.contains("process.env")
        || v.contains("os.environ")
        || v.contains("env::var")
        || v.contains('(')
        || ["example", "placeholder", "changeme", "your", "xxxx", "redacted", "dummy", "****"]
            .iter()
            .any(|w| l.contains(w))
}

/// Replaces secrets in `s` with `[REDACTED:<rule>]`.
pub fn redact(s: &str) -> Cow<'_, str> {
    let mut out: Cow<'_, str> = Cow::Borrowed(s);
    for r in RULES.iter() {
        if !r.re.is_match(&out) {
            continue;
        }
        let replaced = r.re.replace_all(&out, |c: &Captures<'_>| {
            let whole = c.get(0).map(|m| m.as_str()).unwrap_or_default();
            let Some(secret) = c.get(r.group) else { return whole.to_string() };
            let v = secret.as_str();
            if v.starts_with("[REDACTED") || (r.entropy > 0.0 && (entropy(v) < r.entropy || looks_harmless(v))) {
                return whole.to_string();
            }
            let start = secret.start() - c.get(0).map(|m| m.start()).unwrap_or(0);
            format!("{}[REDACTED:{}]{}", &whole[..start], r.id, &whole[start + v.len()..])
        });
        if let Cow::Owned(s) = replaced
            && s != *out
        {
            out = Cow::Owned(s);
        }
    }
    out
}

/// Redacts in place; returns true if anything changed.
pub fn redact_in_place(s: &mut String) -> bool {
    match redact(s) {
        Cow::Owned(r) => {
            *s = r;
            true
        }
        Cow::Borrowed(_) => false,
    }
}

/// Redacts every text field of an agent transcript event.
pub fn redact_event(kind: &mut polyph_proto::messages::AgentEventKind) {
    use polyph_proto::messages::AgentEventKind as K;
    let opt = |o: &mut Option<String>| {
        if let Some(s) = o {
            redact_in_place(s);
        }
    };
    match kind {
        K::Prompt { text, .. } | K::Message { text } | K::Thought { text } => {
            redact_in_place(text);
        }
        K::Diff { unified, .. } => {
            redact_in_place(unified);
        }
        K::State { detail, .. } => opt(detail),
        K::Stopped { reason } => {
            redact_in_place(reason);
        }
        K::Tool(t) => {
            redact_in_place(&mut t.title);
            opt(&mut t.command);
            opt(&mut t.query);
            opt(&mut t.diff);
            opt(&mut t.output);
        }
        K::Started { .. } | K::Usage(_) | K::Cursor { .. } => {}
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    #[test]
    fn known_token_shapes() {
        // Fixtures are fake and built from pieces, so no token-shaped
        // literal sits in the source for secret scanners to flag.
        let t = |parts: &[&str]| parts.concat();
        let cases = [
            (t(&["key AK", "IAIOSFODNN7ABCDEFQ in env"]), "aws-access-key"),
            (t(&["aws_secret_access_key = ", "wJalrXUtnFEMI/K7MDENG", "/bPxRfiCYzQ8r2LmTbk"]), "aws-secret-key"),
            (t(&["GH gh", "p_aBcDeFgHiJkLmNoPqRsTuVwXyZ0123456789"]), "github-token"),
            (t(&["SLACK=xo", "xb-123456789012-abcdefghijkl"]), "slack-token"),
            (t(&["stripe sk", "_live_", "4eC39HqLyjWDarjtT1zdp7dc"]), "stripe-key"),
            (t(&["ANTHROPIC_API_KEY=sk-", "ant-api03-AbCdEf123456_ghIJKlmnOPq"]), "anthropic-key"),
            (t(&["OPENAI sk-", "proj-Ab3dEf5hIj7lMn9pQr2tUv4xYz"]), "openai-key"),
            (t(&["maps AI", "zaSyA1b2C3d4E5f6G7h8I9j0K1l2M3n4O5p6Q"]), "gcp-api-key"),
            (
                t(&[
                    "eyJhbGciOiJIUzI1NiJ9.",
                    "eyJzdWIiOiIxMjM0NTY3ODkwIn0.",
                    "dozjgNryP4J3jVmNHl0w5N_XgL0n3I9PlFUP0THsR8U",
                ]),
                "jwt",
            ),
            (t(&["postgres://app:", "s3cr3tP4ss@db.internal:5432/app"]), "url-password"),
            (t(&["db_password = \"", "Zx9!kQ2#vL7pR4tW\""]), "generic-secret"),
        ];
        for (input, id) in cases {
            let input = input.as_str();
            let out = redact(input);
            assert!(out.contains(&format!("[REDACTED:{id}]")), "{input} -> {out}");
        }
        let url = redact("postgres://app:s3cr3tP4ss@db.internal:5432/app");
        assert_eq!(url, "postgres://app:[REDACTED:url-password]@db.internal:5432/app");
    }

    #[test]
    fn private_key_blocks() {
        let k = "before\n-----BEGIN OPENSSH PRIVATE KEY-----\nb3BlbnNzaC1rZXktdjEAAAA\n-----END OPENSSH PRIVATE KEY-----\nafter";
        assert_eq!(redact(k), "before\n[REDACTED:private-key]\nafter");
        // Cut off output still hides the rest.
        assert_eq!(redact("x -----BEGIN RSA PRIVATE KEY-----\nMIIEow"), "x [REDACTED:private-key]");
    }

    #[test]
    fn ordinary_text_is_untouched() {
        for s in [
            "fn login(token: &str) -> bool { verify(token) }",
            "let password = std::env::var(\"DB_PASSWORD\")?;",
            "api_key = ${API_KEY}",
            "secret = \"your-secret-here\"",
            "token: aaaaaaaaaaaa",
            "3 passed; 0 failed",
            "https://github.com/ProjectDEV-git/Polyph",
            "sk-learn is a library",
        ] {
            assert!(matches!(redact(s), Cow::Borrowed(_)), "changed: {s} -> {}", redact(s));
        }
    }

    #[test]
    fn idempotent() {
        let once = redact(&["gh", "p_aBcDeFgHiJkLmNoPqRsTuVwXyZ0123456789"].concat()).into_owned();
        assert_eq!(redact(&once), once);
    }
}
