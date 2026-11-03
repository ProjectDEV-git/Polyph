//! User settings (no secrets) in `config.toml`.

use serde::{Deserialize, Serialize};

use crate::paths::Paths;

/// Hosted relay used when the user has not chosen one. Set at build time with
/// `POLYPH_DEFAULT_RELAY`; the product domain is not final (see STUBS.md).
pub const DEFAULT_RELAY: &str = match option_env!("POLYPH_DEFAULT_RELAY") {
    Some(v) => v,
    None => "wss://relay.polyph.dev/ws",
};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct UserConfig {
    /// Display name shown to teammates.
    pub name: Option<String>,
    /// Relay URL for new rooms.
    pub relay: String,
    /// This person's member id (hex). Generated once; shared by linked devices.
    pub member: Option<String>,
    /// UI language override, e.g. "th".
    pub lang: Option<String>,
    /// "dark", "light" or "system".
    pub theme: String,
    /// Local API port; 0 picks a free port.
    pub api_port: u16,
}

impl Default for UserConfig {
    fn default() -> Self {
        Self {
            name: None,
            relay: DEFAULT_RELAY.into(),
            member: None,
            lang: None,
            theme: "dark".into(),
            api_port: 47_850,
        }
    }
}

impl UserConfig {
    pub fn load(paths: &Paths) -> anyhow::Result<Self> {
        match std::fs::read_to_string(paths.config()) {
            Ok(s) => toml::from_str(&s).map_err(|e| anyhow::anyhow!("{} is not valid: {e}", paths.config().display())),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(e) => Err(e.into()),
        }
    }

    pub fn save(&self, paths: &Paths) -> anyhow::Result<()> {
        std::fs::write(paths.config(), toml::to_string_pretty(self)?)?;
        Ok(())
    }

    /// Sets one key from a string, as `polyph config set <key> <value>` does.
    pub fn set(&mut self, key: &str, value: &str) -> anyhow::Result<()> {
        match key {
            "name" => self.name = Some(value.to_string()),
            "relay" => {
                if !(value.starts_with("ws://") || value.starts_with("wss://")) {
                    anyhow::bail!("relay must be a ws:// or wss:// URL, for example wss://relay.example.com/ws");
                }
                self.relay = value.to_string();
            }
            "lang" => self.lang = Some(value.to_string()),
            "theme" => {
                if !["dark", "light", "system"].contains(&value) {
                    anyhow::bail!("theme must be dark, light or system");
                }
                self.theme = value.to_string();
            }
            "api_port" => self.api_port = value.parse().map_err(|_| anyhow::anyhow!("api_port must be a number"))?,
            other => anyhow::bail!("unknown setting {other}; known: name, relay, lang, theme, api_port"),
        }
        Ok(())
    }

    pub fn get(&self, key: &str) -> Option<String> {
        match key {
            "name" => self.name.clone(),
            "relay" => Some(self.relay.clone()),
            "member" => self.member.clone(),
            "lang" => self.lang.clone(),
            "theme" => Some(self.theme.clone()),
            "api_port" => Some(self.api_port.to_string()),
            _ => None,
        }
    }
}
