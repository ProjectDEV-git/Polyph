//! Known agents, custom agents from `polyph.toml`, and `agents doctor`.

use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// `[agents.<name>]` in `polyph.toml`.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct AgentConfig {
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub env: BTreeMap<String, String>,
    /// Extra writable directories (the agent's own config/cache), `~` expanded.
    #[serde(default)]
    pub writable: Vec<String>,
    /// Hosts this agent needs (its model API), added to the proxy allowlist.
    #[serde(default)]
    pub hosts: Vec<String>,
    /// Whether the agent bills per token (false for subscription logins).
    #[serde(default)]
    pub metered: Option<bool>,
}

#[derive(Clone, Debug, Serialize, PartialEq)]
pub struct AgentDef {
    pub name: String,
    pub config: AgentConfig,
    /// The underlying CLI that must be installed and logged in, if any.
    pub cli: Option<String>,
    /// Translation key + args for how to install the adapter or CLI.
    pub install_hint: Option<(String, String)>,
    pub login_hint: Option<(String, String)>,
    /// Files whose presence means "logged in", relative to home.
    pub login_files: Vec<String>,
    /// Environment variables that also count as logged in.
    pub login_env: Vec<String>,
    pub builtin: bool,
}

fn home() -> PathBuf {
    polyph_sandbox::home_dir().unwrap_or_default()
}

pub fn expand(p: &str) -> PathBuf {
    match p.strip_prefix("~/") {
        Some(rest) => home().join(rest),
        None => PathBuf::from(p),
    }
}

/// Built-in definitions. Commands are the ACP adapters' executables.
pub fn builtins() -> Vec<AgentDef> {
    let s = |v: &[&str]| v.iter().map(|x| x.to_string()).collect::<Vec<_>>();
    vec![
        AgentDef {
            name: "claude".into(),
            config: AgentConfig {
                command: "claude-agent-acp".into(),
                writable: s(&["~/.claude", "~/.claude.json", "~/.cache/claude-cli-nodejs"]),
                hosts: s(&["api.anthropic.com", "statsig.anthropic.com", "console.anthropic.com", "claude.ai"]),
                ..Default::default()
            },
            cli: Some("claude".into()),
            install_hint: Some((
                "doctor-install-npm".into(),
                "npm install -g @zed-industries/claude-agent-acp @anthropic-ai/claude-code".into(),
            )),
            login_hint: Some(("doctor-login".into(), "claude /login".into())),
            login_files: s(&[".claude/.credentials.json", ".claude.json"]),
            login_env: s(&["ANTHROPIC_API_KEY", "CLAUDE_CODE_OAUTH_TOKEN"]),
            builtin: true,
        },
        AgentDef {
            name: "codex".into(),
            config: AgentConfig {
                command: "codex-acp".into(),
                writable: s(&["~/.codex"]),
                hosts: s(&["api.openai.com", "chatgpt.com", "auth.openai.com"]),
                ..Default::default()
            },
            cli: Some("codex".into()),
            install_hint: Some((
                "doctor-install-npm".into(),
                "npm install -g @zed-industries/codex-acp @openai/codex".into(),
            )),
            login_hint: Some(("doctor-login".into(), "codex login".into())),
            login_files: s(&[".codex/auth.json"]),
            login_env: s(&["OPENAI_API_KEY", "CODEX_API_KEY"]),
            builtin: true,
        },
        AgentDef {
            name: "gemini".into(),
            config: AgentConfig {
                command: "gemini".into(),
                args: s(&["--experimental-acp"]),
                writable: s(&["~/.gemini"]),
                hosts: s(&[
                    "generativelanguage.googleapis.com",
                    "oauth2.googleapis.com",
                    "cloudcode-pa.googleapis.com",
                ]),
                ..Default::default()
            },
            cli: None,
            install_hint: Some(("doctor-install-npm".into(), "npm install -g @google/gemini-cli".into())),
            login_hint: Some(("doctor-login".into(), "gemini".into())),
            login_files: s(&[".gemini/oauth_creds.json"]),
            login_env: s(&["GEMINI_API_KEY", "GOOGLE_API_KEY"]),
            builtin: true,
        },
        AgentDef {
            name: "opencode".into(),
            config: AgentConfig {
                command: "opencode".into(),
                args: s(&["acp"]),
                writable: s(&["~/.local/share/opencode", "~/.config/opencode", "~/.cache/opencode"]),
                ..Default::default()
            },
            cli: None,
            install_hint: Some(("doctor-install-npm".into(), "npm install -g opencode-ai".into())),
            login_hint: Some(("doctor-login".into(), "opencode auth login".into())),
            login_files: s(&[".local/share/opencode/auth.json"]),
            login_env: vec![],
            builtin: true,
        },
        AgentDef {
            name: "local".into(),
            config: AgentConfig { command: "polyph-localagent".into(), metered: Some(false), ..Default::default() },
            cli: None,
            install_hint: Some(("doctor-install-local".into(), "cargo install polyph-localagent".into())),
            login_hint: None,
            login_files: vec![],
            login_env: vec![],
            builtin: true,
        },
    ]
}

/// Built-ins overridden or extended by `[agents.*]` from `polyph.toml`.
pub fn all(custom: &BTreeMap<String, AgentConfig>) -> Vec<AgentDef> {
    let mut v = builtins();
    for (name, cfg) in custom {
        match v.iter_mut().find(|d| &d.name == name) {
            Some(d) => d.config = cfg.clone(),
            None => v.push(AgentDef {
                name: name.clone(),
                config: cfg.clone(),
                cli: None,
                install_hint: None,
                login_hint: None,
                login_files: vec![],
                login_env: vec![],
                builtin: false,
            }),
        }
    }
    v
}

#[derive(Clone, Debug, Serialize, PartialEq)]
pub struct DoctorItem {
    pub name: String,
    pub adapter_installed: bool,
    pub cli_installed: Option<bool>,
    pub logged_in: Option<bool>,
    /// Translation key + command for each problem, in the order to fix them.
    pub fixes: Vec<(String, String)>,
}

/// Finds an agent command on PATH or next to the running executable (where
/// installers put `polyph-localagent` beside `polyph`).
pub fn resolve_command(cmd: &str) -> Option<PathBuf> {
    if let Ok(p) = which::which(cmd) {
        return Some(p);
    }
    let exe = std::env::current_exe().ok()?;
    let sibling = exe.parent()?.join(if cfg!(windows) { format!("{cmd}.exe") } else { cmd.to_string() });
    sibling.exists().then_some(sibling)
}

pub fn doctor(defs: &[AgentDef]) -> Vec<DoctorItem> {
    defs.iter()
        .map(|d| {
            let adapter_installed = resolve_command(&d.config.command).is_some();
            let cli_installed = d.cli.as_ref().map(|c| which::which(c).is_ok());
            let logged_in = if d.login_files.is_empty() && d.login_env.is_empty() {
                None
            } else {
                Some(
                    d.login_files.iter().any(|f| home().join(f).exists())
                        || d.login_env.iter().any(|e| std::env::var_os(e).is_some()),
                )
            };
            let mut fixes = vec![];
            if (!adapter_installed || cli_installed == Some(false))
                && let Some(h) = &d.install_hint
            {
                fixes.push(h.clone());
            }
            if logged_in == Some(false)
                && let Some(h) = &d.login_hint
            {
                fixes.push(h.clone());
            }
            DoctorItem { name: d.name.clone(), adapter_installed, cli_installed, logged_in, fixes }
        })
        .collect()
}
