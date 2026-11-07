//! `polyph feedback`: a prefilled GitHub issue link. The report holds only
//! the Polyph version, OS, architecture, Linux distro, the surface, whether
//! the daemon runs and whether the relay is the default. It never holds room
//! names, paths, chat, code, user or host names, member ids or relay URLs.

use polyph_i18n::{Localizer, tr};

use crate::{Ctx, client};

const NEW_ISSUE: &str = "https://github.com/ProjectDEV-git/Polyph/issues/new";

/// Surfaces a report can come from. A fixed list keeps free text out of it.
pub const SURFACES: [&str; 6] = ["cli", "tui", "web", "desktop", "phone", "vscode"];

pub struct Info {
    pub version: String,
    pub os: String,
    pub arch: String,
    pub distro: Option<String>,
    pub surface: String,
    pub daemon_running: bool,
    pub custom_relay: bool,
}

/// `ID` and `VERSION_ID` from os-release contents, such as `ubuntu 24.04`.
pub fn parse_os_release(s: &str) -> Option<String> {
    let get = |key: &str| {
        s.lines()
            .find_map(|l| l.strip_prefix(key)?.strip_prefix('='))
            .map(|v| v.trim().trim_matches('"').trim_matches('\'').to_string())
            .filter(|v| !v.is_empty())
    };
    let id = get("ID")?;
    Some(match get("VERSION_ID") {
        Some(v) => format!("{id} {v}"),
        None => id,
    })
}

async fn gather(ctx: &Ctx, surface: &str) -> Info {
    let daemon_running = match client::daemon_port(&ctx.paths) {
        Some(p) => client::healthy(p).await,
        None => false,
    };
    let custom_relay = polyph_daemon::config::UserConfig::load(&ctx.paths)
        .map(|c| c.relay != polyph_daemon::config::DEFAULT_RELAY)
        .unwrap_or(false);
    let distro = if cfg!(target_os = "linux") {
        std::fs::read_to_string("/etc/os-release").ok().and_then(|s| parse_os_release(&s))
    } else {
        None
    };
    Info {
        version: env!("CARGO_PKG_VERSION").to_string(),
        os: std::env::consts::OS.to_string(),
        arch: std::env::consts::ARCH.to_string(),
        distro,
        surface: surface.to_string(),
        daemon_running,
        custom_relay,
    }
}

fn encode(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~') {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

pub fn build_url(l: &Localizer, i: &Info) -> String {
    let os = match &i.distro {
        Some(d) => format!("{} ({d}), {}", i.os, i.arch),
        None => format!("{}, {}", i.os, i.arch),
    };
    let daemon = l.t(if i.daemon_running { "feedback-daemon-running" } else { "feedback-daemon-stopped" });
    let relay = l.t(if i.custom_relay { "feedback-relay-custom" } else { "feedback-relay-default" });
    let body = format!(
        "{}\n\n- {}\n- {}\n- {}\n- {}\n- {}\n\n### {}\n\n\n### {}\n\n",
        l.t("feedback-intro"),
        tr!(l, "feedback-version", value = i.version.as_str()),
        tr!(l, "feedback-os", value = os),
        tr!(l, "feedback-surface", value = i.surface.as_str()),
        tr!(l, "feedback-daemon", value = daemon),
        tr!(l, "feedback-relay", value = relay),
        l.t("feedback-happened"),
        l.t("feedback-expected"),
    );
    format!("{NEW_ISSUE}?title={}&body={}", encode(&l.t("feedback-title")), encode(&body))
}

pub async fn run(ctx: &Ctx, surface: String, print: bool) -> anyhow::Result<()> {
    let l = &ctx.l;
    let info = gather(ctx, &surface).await;
    let url = build_url(l, &info);
    if ctx.json {
        ctx.print_json(&serde_json::json!({ "url": url }));
    } else if print || crate::open_browser(&url).is_err() {
        ctx.out(tr!(l, "feedback-link", url = url.as_str()));
    } else {
        ctx.out(l.t("feedback-opened"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn decode(s: &str) -> String {
        let b = s.as_bytes();
        let mut out = vec![];
        let mut i = 0;
        while i < b.len() {
            if b[i] == b'%' {
                out.push(u8::from_str_radix(&s[i + 1..i + 3], 16).unwrap());
                i += 3;
            } else {
                out.push(b[i]);
                i += 1;
            }
        }
        String::from_utf8(out).unwrap()
    }

    #[test]
    fn url_has_only_the_allowed_facts() {
        let home = std::env::var("HOME").unwrap_or_default();
        let host = std::fs::read_to_string("/etc/hostname").unwrap_or_default().trim().to_string();
        let cwd = std::env::current_dir().unwrap().display().to_string();
        let user = std::env::var("USER").unwrap_or_default();
        for lang in polyph_i18n::available() {
            let l = Localizer::new(lang);
            let i = Info {
                version: "1.2.3".into(),
                os: "linux".into(),
                arch: "x86_64".into(),
                distro: Some("ubuntu 24.04".into()),
                surface: "cli".into(),
                daemon_running: true,
                custom_relay: true,
            };
            let url = build_url(&l, &i);
            assert!(url.starts_with(NEW_ISSUE));
            let all = format!("{url} {}", decode(&url));
            for secret in [&home, &host, &cwd, &user] {
                if secret.len() > 3 {
                    assert!(!all.contains(secret.as_str()), "{lang}: leaked {secret}");
                }
            }
            assert!(!all.contains("wss://") && !all.contains("ws://"));
            assert!(decode(&url).contains("1.2.3") && decode(&url).contains("ubuntu 24.04"));
        }
    }

    #[test]
    fn os_release_is_parsed() {
        let s = "NAME=\"Ubuntu\"\nID=ubuntu\nVERSION_ID=\"24.04\"\nHOME_URL=x\n";
        assert_eq!(parse_os_release(s).as_deref(), Some("ubuntu 24.04"));
        assert_eq!(parse_os_release("ID=arch\n").as_deref(), Some("arch"));
        assert_eq!(parse_os_release("NAME=x\n"), None);
    }
}
