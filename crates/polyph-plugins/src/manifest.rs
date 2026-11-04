//! `polyph-plugin.toml`:
//!
//! ```toml
//! name = "release-notes"
//! version = "0.1.0"
//! description = "Drafts release notes from merged cards."
//!
//! [mcp]                     # optional: tools for agents (an MCP stdio server)
//! command = "node"
//! args = ["server.js"]      # run in the plugin's folder
//!
//! [[hooks]]                 # optional: run on room events, event JSON on stdin
//! on = ["merge_queue", "chat"]
//! command = "sh"
//! args = ["hook.sh"]
//! timeout_secs = 10
//!
//! [permissions]
//! hosts = ["api.example.com"]   # network the server and hooks may reach
//! ```
//!
//! A hook may print lines of JSON to act: `{"chat": "text"}` posts to the
//! room chat, labeled with the plugin's name.

use std::collections::BTreeMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::{Error, Result, valid_name};

pub const FILE: &str = "polyph-plugin.toml";

/// Room events hooks can run on (the daemon's event `type`s).
pub const EVENTS: &[&str] = &[
    "chat",
    "approval_pending",
    "approval_resolved",
    "merge_queue",
    "member_joined",
    "member_removed",
    "knock",
    "claim_revoked",
    "doc_changed",
];

/// Longest a hook may run, and the default.
pub const MAX_TIMEOUT: u64 = 60;
pub const DEFAULT_TIMEOUT: u64 = 10;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Command {
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub env: BTreeMap<String, String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Hook {
    pub on: Vec<String>,
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub timeout_secs: Option<u64>,
}

impl Hook {
    pub fn timeout(&self) -> u64 {
        self.timeout_secs.unwrap_or(DEFAULT_TIMEOUT).clamp(1, MAX_TIMEOUT)
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Permissions {
    /// Exact hosts or `*.suffix` wildcards, as in `[sandbox] allow_hosts`.
    #[serde(default)]
    pub hosts: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    pub name: String,
    pub version: String,
    #[serde(default)]
    pub description: String,
    pub mcp: Option<Command>,
    #[serde(default)]
    pub hooks: Vec<Hook>,
    #[serde(default)]
    pub permissions: Permissions,
}

impl Manifest {
    pub fn parse(text: &str) -> Result<Self> {
        let m: Manifest = toml::from_str(text).map_err(|e| Error::Invalid(format!("{FILE}: {e}")))?;
        m.validate()?;
        Ok(m)
    }

    pub fn load(dir: &Path) -> Result<Self> {
        let p = dir.join(FILE);
        let text = std::fs::read_to_string(&p).map_err(|e| match e.kind() {
            std::io::ErrorKind::NotFound => Error::NotFound(format!("{} has no {FILE}", dir.display())),
            _ => Error::Io(e),
        })?;
        Self::parse(&text)
    }

    pub fn validate(&self) -> Result<()> {
        if !valid_name(&self.name) {
            return Err(Error::Invalid(format!(
                "plugin name \"{}\" must be lower-case letters, digits and dashes",
                self.name
            )));
        }
        if self.mcp.is_none() && self.hooks.is_empty() {
            return Err(Error::Invalid(format!("plugin {} has neither [mcp] nor [[hooks]]", self.name)));
        }
        for h in &self.hooks {
            if h.on.is_empty() {
                return Err(Error::Invalid(format!("a hook of {} lists no events in `on`", self.name)));
            }
            if let Some(bad) = h.on.iter().find(|e| !EVENTS.contains(&e.as_str())) {
                return Err(Error::Invalid(format!(
                    "unknown event \"{bad}\" in {}; known: {}",
                    self.name,
                    EVENTS.join(", ")
                )));
            }
        }
        for h in &self.permissions.hosts {
            let bare = h.trim_start_matches("*.");
            if bare.is_empty() || bare.contains(['/', ':', ' ', '*']) {
                return Err(Error::Invalid(format!(
                    "permission host \"{h}\" must be a host name like api.example.com"
                )));
            }
        }
        Ok(())
    }

    /// Hooks that run on an event type.
    pub fn hooks_for<'a>(&'a self, event: &'a str) -> impl Iterator<Item = &'a Hook> + 'a {
        self.hooks.iter().filter(move |h| h.on.iter().any(|e| e == event))
    }
}

/// What a hook asked for, one JSON object per output line.
#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct Action {
    pub chat: Option<String>,
}

/// Parses a hook's output: JSON lines that are actions; other lines are ignored.
pub fn actions(stdout: &str) -> Vec<Action> {
    stdout
        .lines()
        .filter_map(|l| serde_json::from_str::<Action>(l.trim()).ok())
        .filter(|a| a.chat.as_deref().is_some_and(|t| !t.trim().is_empty()))
        .collect()
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    const OK: &str = r#"
name = "release-notes"
version = "0.1.0"
[mcp]
command = "node"
args = ["server.js"]
[[hooks]]
on = ["merge_queue"]
command = "sh"
args = ["hook.sh"]
[permissions]
hosts = ["api.example.com", "*.example.org"]
"#;

    #[test]
    fn parses_and_validates() {
        let m = Manifest::parse(OK).unwrap();
        assert_eq!(m.hooks_for("merge_queue").count(), 1);
        assert_eq!(m.hooks_for("chat").count(), 0);
        assert_eq!(m.hooks[0].timeout(), DEFAULT_TIMEOUT);
        for (bad, why) in [
            (OK.replace("release-notes", "Release Notes"), "name"),
            (OK.replace("merge_queue", "explode"), "unknown event"),
            (OK.replace("api.example.com", "https://api.example.com/"), "host name"),
            (OK.replace("[permissions]", "[permissions]\nsecret = 1"), "unknown field"),
            ("name = \"x\"\nversion = \"1\"\n".into(), "neither"),
        ] {
            let e = Manifest::parse(&bad).unwrap_err().to_string();
            assert!(e.contains(why), "{why}: {e}");
        }
    }

    #[test]
    fn hook_actions() {
        let a = actions("progress...\n{\"chat\": \"v1.2 notes drafted\"}\n{\"chat\": \"  \"}\n{\"other\": 1}\n");
        assert_eq!(a, vec![Action { chat: Some("v1.2 notes drafted".into()) }]);
    }
}
