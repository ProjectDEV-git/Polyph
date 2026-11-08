//! Where agent clients keep their MCP settings, and the Polyph entry each
//! one needs. Used by `polyph connect`, `polyph disconnect`, `polyph start`
//! and `polyph doctor`, and by the app's first-run checklist through the
//! daemon (`setup.status`, `clients.plan`, `clients.connect`).
//!
//! Every change is planned first ([`plan_add`], [`plan_remove`]) so the
//! person sees the exact entry before anything is written, and [`apply`]
//! keeps a copy of the previous file next to it (`<file>.polyph-backup`).
//! Files with comments (VS Code and Zed allow them) are read by stripping
//! the comments; writing them back drops the comments, which the plan says.

use std::path::{Path, PathBuf};

use serde_json::{Map, Value, json};

/// Name of Polyph's entry in every client's config for the default data directory.
pub const ENTRY: &str = "polyph";

/// The entry name for a data directory: `polyph` for the default one
/// (`None`), `polyph-<8 hex of a hash of the path>` for any other, so two
/// homes never overwrite each other's entry.
pub fn entry_name(home_override: Option<&Path>) -> String {
    match home_override {
        None => ENTRY.to_string(),
        Some(h) => {
            use sha2::{Digest, Sha256};
            let d = Sha256::digest(h.to_string_lossy().as_bytes());
            format!("{ENTRY}-{}", hex::encode(&d[..4]))
        }
    }
}

/// The data directory a current entry starts Polyph with, if it sets one.
pub fn entry_home(v: &Value) -> Option<String> {
    ["env", "environment"].iter().find_map(|k| v.get(k)?.get("POLYPH_HOME")?.as_str().map(str::to_string))
}

/// The places a client's files can be, for this person on this system.
#[derive(Debug, Clone)]
pub struct Env {
    pub home: PathBuf,
    /// `~/.config` (or `$XDG_CONFIG_HOME`) on Linux and macOS; `%APPDATA%` on Windows.
    pub config: PathBuf,
    /// `~/Library/Application Support` on macOS; the same as `config` elsewhere.
    pub app_support: PathBuf,
    /// `$PATH`, for finding client programs.
    pub path: Option<std::ffi::OsString>,
    pub codex_home: Option<PathBuf>,
}

impl Env {
    pub fn current() -> Option<Self> {
        let home = std::env::var_os("HOME")
            .or_else(|| std::env::var_os("USERPROFILE"))
            .map(PathBuf::from)
            .filter(|p| !p.as_os_str().is_empty())?;
        let config = if cfg!(windows) {
            std::env::var_os("APPDATA").map(PathBuf::from).unwrap_or_else(|| home.join("AppData").join("Roaming"))
        } else {
            std::env::var_os("XDG_CONFIG_HOME").map(PathBuf::from).unwrap_or_else(|| home.join(".config"))
        };
        let app_support =
            if cfg!(target_os = "macos") { home.join("Library").join("Application Support") } else { config.clone() };
        Some(Self {
            home,
            config,
            app_support,
            path: std::env::var_os("PATH"),
            codex_home: std::env::var_os("CODEX_HOME").map(PathBuf::from),
        })
    }

    /// A sandbox for tests: everything under `root`.
    #[cfg(test)]
    pub fn at(root: &Path) -> Self {
        Self {
            home: root.to_path_buf(),
            config: root.join(".config"),
            app_support: root.join(".config"),
            path: None,
            codex_home: None,
        }
    }

    fn on_path(&self, program: &str) -> bool {
        let Some(path) = &self.path else { return false };
        std::env::split_paths(path).any(|d| {
            d.join(program).is_file()
                || (cfg!(windows) && ["exe", "cmd", "bat"].iter().any(|x| d.join(format!("{program}.{x}")).is_file()))
        })
    }
}

/// How a client's config file is shaped.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    /// `{"mcpServers": {"polyph": {...}}}` (Claude Code, Cursor, Gemini CLI, Windsurf).
    McpServers,
    /// VS Code's `mcp.json`: `{"servers": {"polyph": {...}}}`.
    VsCode,
    /// Zed's `settings.json`: `{"context_servers": {"polyph": {...}}}`.
    Zed,
    /// OpenCode's `opencode.json`: `{"mcp": {"polyph": {...}}}`.
    OpenCode,
    /// Codex's `config.toml`: `[mcp_servers.polyph]`.
    Codex,
}

#[derive(Debug, Clone, Copy)]
pub struct Client {
    /// Name on the command line and in agent labels.
    pub id: &'static str,
    /// Product name, as people know it.
    pub name: &'static str,
    pub format: Format,
    /// Program on PATH that shows the client is installed.
    program: Option<&'static str>,
}

pub const CLIENTS: &[Client] = &[
    Client { id: "claude-code", name: "Claude Code", format: Format::McpServers, program: Some("claude") },
    Client { id: "codex", name: "Codex", format: Format::Codex, program: Some("codex") },
    Client { id: "cursor", name: "Cursor", format: Format::McpServers, program: Some("cursor") },
    Client { id: "vscode", name: "VS Code", format: Format::VsCode, program: Some("code") },
    Client { id: "gemini", name: "Gemini CLI", format: Format::McpServers, program: Some("gemini") },
    Client { id: "windsurf", name: "Windsurf", format: Format::McpServers, program: Some("windsurf") },
    Client { id: "zed", name: "Zed", format: Format::Zed, program: Some("zed") },
    Client { id: "opencode", name: "OpenCode", format: Format::OpenCode, program: Some("opencode") },
];

pub fn find(id: &str) -> Option<&'static Client> {
    let id = id.trim().to_lowercase();
    let alias = match id.as_str() {
        "claude" | "claudecode" | "claude_code" => "claude-code",
        "code" | "vs-code" | "copilot" => "vscode",
        "gemini-cli" => "gemini",
        other => other,
    };
    CLIENTS.iter().find(|c| c.id == alias)
}

impl Client {
    /// The file holding this client's MCP servers (user-wide).
    pub fn file(&self, env: &Env) -> PathBuf {
        match self.id {
            "claude-code" => env.home.join(".claude.json"),
            "codex" => env.codex_home.clone().unwrap_or_else(|| env.home.join(".codex")).join("config.toml"),
            "cursor" => env.home.join(".cursor").join("mcp.json"),
            "vscode" => env.app_support.join("Code").join("User").join("mcp.json"),
            "gemini" => env.home.join(".gemini").join("settings.json"),
            "windsurf" => env.home.join(".codeium").join("windsurf").join("mcp_config.json"),
            "zed" => {
                if cfg!(windows) {
                    env.config.join("Zed").join("settings.json")
                } else {
                    // Zed uses ~/.config/zed on macOS too.
                    env.home.join(".config").join("zed").join("settings.json")
                }
            }
            _ => env.config.join("opencode").join("opencode.json"),
        }
    }

    /// The client gets Polyph without a settings entry: VS Code with the
    /// Polyph extension (which registers Polyph for Copilot), or Claude Code
    /// with the Polyph plugin enabled. An entry would add Polyph twice.
    pub fn registered_by_extension(&self, env: &Env) -> bool {
        match self.id {
            "vscode" => std::fs::read_dir(env.home.join(".vscode").join("extensions"))
                .map(|d| d.flatten().any(|e| e.file_name().to_string_lossy().starts_with("polyph.polyph-vscode-")))
                .unwrap_or(false),
            "claude-code" => std::fs::read(env.home.join(".claude").join("settings.json"))
                .ok()
                .and_then(|b| parse_json(Path::new("settings.json"), &String::from_utf8_lossy(&b)).ok())
                .and_then(|(root, _)| root.get("enabledPlugins").cloned())
                .and_then(|p| p.as_object().cloned())
                .is_some_and(|p| p.iter().any(|(k, v)| k.starts_with("polyph@") && v == &Value::Bool(true))),
            _ => false,
        }
    }

    /// Whether the client looks installed: its config folder or its program.
    pub fn installed(&self, env: &Env) -> bool {
        let marker = match self.id {
            "claude-code" => env.home.join(".claude"),
            _ => self.file(env).parent().map(Path::to_path_buf).unwrap_or_default(),
        };
        marker.exists() || self.file(env).exists() || self.program.is_some_and(|p| env.on_path(p))
    }
}

/// How the client reaches Polyph.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Transport {
    /// Starts `polyph mcp` (the default).
    Stdio { command: String, env: Vec<(String, String)> },
    /// Talks to the daemon's local web address with a connect key.
    Http { url: String, key: String },
}

impl Transport {
    /// `polyph mcp --client <id>` from this very binary.
    pub fn stdio(polyph: &Path, home_override: Option<&Path>) -> Self {
        let env = home_override.map(|h| vec![("POLYPH_HOME".to_string(), h.to_string_lossy().into_owned())]);
        Self::Stdio { command: polyph.to_string_lossy().into_owned(), env: env.unwrap_or_default() }
    }
}

fn env_obj(env: &[(String, String)]) -> Value {
    Value::Object(env.iter().map(|(k, v)| (k.clone(), json!(v))).collect())
}

/// Polyph's entry for `client` in its own JSON shape.
pub fn entry(client: &Client, t: &Transport) -> Value {
    let args = json!(["mcp", "--client", client.id]);
    match t {
        Transport::Stdio { command, env } => {
            let mut v = match client.format {
                Format::VsCode => json!({ "type": "stdio", "command": command, "args": args }),
                Format::Zed => json!({ "source": "custom", "command": command, "args": args }),
                Format::OpenCode => {
                    json!({ "type": "local", "command": [command, "mcp", "--client", client.id], "enabled": true })
                }
                _ if client.id == "claude-code" => json!({ "type": "stdio", "command": command, "args": args }),
                _ => json!({ "command": command, "args": args }),
            };
            if !env.is_empty() {
                let k = if client.format == Format::OpenCode { "environment" } else { "env" };
                v[k] = env_obj(env);
            }
            v
        }
        Transport::Http { url, key } => {
            let headers = json!({ "Authorization": format!("Bearer {key}") });
            match (client.format, client.id) {
                (Format::VsCode, _) | (_, "claude-code") => json!({ "type": "http", "url": url, "headers": headers }),
                (Format::OpenCode, _) => json!({ "type": "remote", "url": url, "headers": headers, "enabled": true }),
                (_, "gemini") => json!({ "httpUrl": url, "headers": headers }),
                (_, "windsurf") => json!({ "serverUrl": url, "headers": headers }),
                (Format::Zed, _) => json!({ "source": "custom", "url": url, "headers": headers }),
                _ => json!({ "url": url, "headers": headers }),
            }
        }
    }
}

fn section(format: Format) -> &'static str {
    match format {
        Format::McpServers => "mcpServers",
        Format::VsCode => "servers",
        Format::Zed => "context_servers",
        Format::OpenCode => "mcp",
        Format::Codex => "mcp_servers",
    }
}

/// A change to one client's file, ready to show and to write.
#[derive(Debug, Clone)]
pub struct Change {
    pub client: &'static Client,
    pub file: PathBuf,
    /// Polyph's entry before and after, as the person would read it.
    pub before: Option<String>,
    pub after: Option<String>,
    /// The whole new file.
    pub text: String,
    pub existed: bool,
    /// The file has comments, which rewriting it drops (a backup keeps them).
    pub drops_comments: bool,
}

impl Change {
    pub fn is_noop(&self) -> bool {
        self.before == self.after
    }

    /// The entry lines with `-`/`+` markers.
    pub fn diff(&self) -> Vec<String> {
        let mut out = vec![];
        for (mark, s) in [("-", &self.before), ("+", &self.after)] {
            if let Some(s) = s {
                out.extend(s.lines().map(|l| format!("{mark} {l}")));
            }
        }
        out
    }
}

/// Removes `//` and `/* */` comments and trailing commas outside strings.
pub fn strip_jsonc(s: &str) -> (String, bool) {
    let b: Vec<char> = s.chars().collect();
    let mut out = String::with_capacity(s.len());
    let (mut i, mut in_str, mut comments) = (0, false, false);
    while i < b.len() {
        let c = b[i];
        if in_str {
            out.push(c);
            if c == '\\' && i + 1 < b.len() {
                out.push(b[i + 1]);
                i += 2;
                continue;
            }
            if c == '"' {
                in_str = false;
            }
            i += 1;
            continue;
        }
        match c {
            '"' => {
                in_str = true;
                out.push(c);
                i += 1;
            }
            '/' if b.get(i + 1) == Some(&'/') => {
                comments = true;
                while i < b.len() && b[i] != '\n' {
                    i += 1;
                }
            }
            '/' if b.get(i + 1) == Some(&'*') => {
                comments = true;
                i += 2;
                while i + 1 < b.len() && !(b[i] == '*' && b[i + 1] == '/') {
                    i += 1;
                }
                i += 2;
            }
            ',' => {
                // Drop a comma whose next non-space character closes a block.
                let mut j = i + 1;
                while j < b.len() && b[j].is_whitespace() {
                    j += 1;
                }
                if !matches!(b.get(j), Some('}') | Some(']')) {
                    out.push(c);
                }
                i += 1;
            }
            _ => {
                out.push(c);
                i += 1;
            }
        }
    }
    (out, comments)
}

fn read(file: &Path) -> anyhow::Result<Option<String>> {
    match std::fs::read_to_string(file) {
        Ok(s) => Ok(Some(s)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(anyhow::anyhow!("{}: {e}", file.display())),
    }
}

fn parse_json(file: &Path, text: &str) -> anyhow::Result<(Map<String, Value>, bool)> {
    if text.trim().is_empty() {
        return Ok((Map::new(), false));
    }
    let (clean, comments) = strip_jsonc(text);
    match serde_json::from_str::<Value>(&clean) {
        Ok(Value::Object(m)) => Ok((m, comments)),
        Ok(_) => anyhow::bail!("{} does not hold a JSON object", file.display()),
        Err(e) => anyhow::bail!("{} is not valid JSON ({e}); fix or move it, then try again", file.display()),
    }
}

fn pretty(v: &Value) -> String {
    serde_json::to_string_pretty(v).unwrap_or_default()
}

fn toml_entry(t: &Transport, client: &Client) -> toml_edit::Table {
    let mut tbl = toml_edit::Table::new();
    match t {
        Transport::Stdio { command, env } => {
            tbl["command"] = toml_edit::value(command.as_str());
            let mut args = toml_edit::Array::new();
            for a in ["mcp", "--client", client.id] {
                args.push(a);
            }
            tbl["args"] = toml_edit::value(args);
            if !env.is_empty() {
                let mut e = toml_edit::InlineTable::new();
                for (k, v) in env {
                    e.insert(k, v.as_str().into());
                }
                tbl["env"] = toml_edit::value(e);
            }
        }
        Transport::Http { url, key } => {
            tbl["url"] = toml_edit::value(url.as_str());
            let mut h = toml_edit::InlineTable::new();
            h.insert("Authorization", format!("Bearer {key}").into());
            tbl["http_headers"] = toml_edit::value(h);
        }
    }
    tbl
}

fn toml_show(t: &toml_edit::Table) -> String {
    let mut doc = toml_edit::DocumentMut::new();
    let mut outer = toml_edit::Table::new();
    outer.set_implicit(true);
    outer.insert(ENTRY, toml_edit::Item::Table(t.clone()));
    doc.insert(section(Format::Codex), toml_edit::Item::Table(outer));
    doc.to_string().trim().to_string()
}

/// Plans adding (or updating) Polyph in `client`'s file; `t: None` plans removing it.
fn plan(client: &'static Client, env: &Env, name: &str, t: Option<&Transport>) -> anyhow::Result<Change> {
    let file = client.file(env);
    let old = read(&file)?;
    let existed = old.is_some();
    let old = old.unwrap_or_default();
    if client.format == Format::Codex {
        let mut doc: toml_edit::DocumentMut = old.parse().map_err(|e| {
            anyhow::anyhow!("{} is not valid TOML ({e}); fix or move it, then try again", file.display())
        })?;
        let sec = section(Format::Codex);
        let before = doc.get(sec).and_then(|s| s.get(name)).and_then(|e| e.as_table()).map(toml_show);
        match t {
            Some(t) => {
                if doc.get(sec).and_then(|s| s.as_table()).is_none() {
                    let mut outer = toml_edit::Table::new();
                    outer.set_implicit(true);
                    doc.insert(sec, toml_edit::Item::Table(outer));
                }
                let entry = toml_entry(t, client);
                let after = Some(toml_show(&entry));
                if let Some(outer) = doc.get_mut(sec).and_then(|s| s.as_table_mut()) {
                    outer.insert(name, toml_edit::Item::Table(entry));
                }
                Ok(Change { client, file, before, after, text: doc.to_string(), existed, drops_comments: false })
            }
            None => {
                if let Some(outer) = doc.get_mut(sec).and_then(|s| s.as_table_mut()) {
                    outer.remove(name);
                }
                Ok(Change { client, file, before, after: None, text: doc.to_string(), existed, drops_comments: false })
            }
        }
    } else {
        let (mut root, comments) = parse_json(&file, &old)?;
        let sec = section(client.format);
        let before = root.get(sec).and_then(|s| s.get(name)).map(pretty);
        let after = t.map(|t| entry(client, t));
        match &after {
            Some(e) => {
                let s = root.entry(sec).or_insert_with(|| json!({}));
                if !s.is_object() {
                    anyhow::bail!("{}: \"{sec}\" is not an object; fix it, then try again", file.display());
                }
                s[name] = e.clone();
            }
            None => {
                if let Some(s) = root.get_mut(sec).and_then(|s| s.as_object_mut()) {
                    s.remove(name);
                }
            }
        }
        let text = pretty(&Value::Object(root)) + "\n";
        let after = after.as_ref().map(pretty);
        let drops_comments = comments && before != after;
        Ok(Change { client, file, before, after, text, existed, drops_comments })
    }
}

pub fn plan_add(client: &'static Client, env: &Env, name: &str, t: &Transport) -> anyhow::Result<Change> {
    plan(client, env, name, Some(t))
}

/// Plans removing Polyph; `None` when the client has no Polyph entry.
pub fn plan_remove(client: &'static Client, env: &Env, name: &str) -> anyhow::Result<Option<Change>> {
    let c = plan(client, env, name, None)?;
    Ok(c.before.is_some().then_some(c))
}

/// Polyph's current entry for `client`, if any, as JSON (TOML read into JSON).
pub fn current(client: &'static Client, env: &Env, name: &str) -> Option<Value> {
    let file = client.file(env);
    let text = read(&file).ok()??;
    if client.format == Format::Codex {
        let v: toml::Table = toml::from_str(&text).ok()?;
        let e = v.get(section(Format::Codex))?.get(name)?;
        serde_json::to_value(e).ok()
    } else {
        let (root, _) = parse_json(&file, &text).ok()?;
        root.get(section(client.format))?.get(name).cloned()
    }
}

/// Where a current entry points: the command it starts, or the URL.
pub fn target(v: &Value) -> Option<String> {
    for k in ["url", "httpUrl", "serverUrl"] {
        if let Some(u) = v.get(k).and_then(|u| u.as_str()) {
            return Some(u.to_string());
        }
    }
    match v.get("command") {
        Some(Value::String(s)) => Some(s.clone()),
        Some(Value::Array(a)) => a.first().and_then(|s| s.as_str()).map(str::to_string),
        _ => None,
    }
}

/// Writes a planned change, keeping the previous file as `<file>.polyph-backup`.
pub fn apply(c: &Change) -> std::io::Result<Option<PathBuf>> {
    if let Some(dir) = c.file.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let backup = if c.existed {
        let mut name = c.file.file_name().map(|n| n.to_os_string()).unwrap_or_default();
        name.push(".polyph-backup");
        let b = c.file.with_file_name(name);
        std::fs::copy(&c.file, &b)?;
        Some(b)
    } else {
        None
    };
    let tmp = c.file.with_extension("polyph-tmp");
    std::fs::write(&tmp, &c.text)?;
    // Keep the original file's permissions (~/.claude.json may be private).
    if let Ok(meta) = std::fs::metadata(&c.file) {
        let _ = std::fs::set_permissions(&tmp, meta.permissions());
    }
    std::fs::rename(&tmp, &c.file)?;
    Ok(backup)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    fn stdio() -> Transport {
        Transport::stdio(Path::new("/opt/polyph/bin/polyph"), None)
    }

    #[test]
    fn jsonc_comments_and_trailing_commas() {
        let (s, c) = strip_jsonc("{\n // hi\n \"a\": \"x // not a comment\", /* b */ \"b\": [1,2,],\n}");
        assert!(c);
        let v: Value = serde_json::from_str(&s).unwrap();
        assert_eq!(v["a"], "x // not a comment");
        assert_eq!(v["b"], json!([1, 2]));
        assert!(!strip_jsonc("{\"a\": \"\\\"/*\"}").1);
    }

    #[test]
    fn adds_and_removes_in_every_client_keeping_other_settings() {
        let root = tempfile::tempdir().unwrap();
        let env = Env::at(root.path());
        for client in CLIENTS {
            let file = client.file(&env);
            std::fs::create_dir_all(file.parent().unwrap()).unwrap();
            let other = if client.format == Format::Codex {
                "model = \"o3\"\n\n[mcp_servers.other]\ncommand = \"other\"\n".to_string()
            } else {
                format!(
                    "{{\"theme\": \"dark\", \"{}\": {{\"other\": {{\"command\": \"other\"}}}}}}",
                    section(client.format)
                )
            };
            std::fs::write(&file, &other).unwrap();
            assert!(client.installed(&env));
            let c = plan_add(client, &env, ENTRY, &stdio()).unwrap();
            assert!(
                c.before.is_none() && c.after.as_deref().unwrap().contains("/opt/polyph/bin/polyph"),
                "{}",
                client.id
            );
            assert!(c.diff().iter().all(|l| l.starts_with("+ ")));
            let backup = apply(&c).unwrap().unwrap();
            assert_eq!(std::fs::read_to_string(backup).unwrap(), other);
            let cur = current(client, &env, ENTRY).unwrap();
            assert_eq!(target(&cur).unwrap(), "/opt/polyph/bin/polyph", "{}", client.id);
            let text = std::fs::read_to_string(&file).unwrap();
            assert!(text.contains("other") && (text.contains("dark") || text.contains("o3")), "{}: {text}", client.id);
            // Connecting again changes nothing.
            assert!(plan_add(client, &env, ENTRY, &stdio()).unwrap().is_noop());
            // Over the web address instead.
            let http = Transport::Http { url: "http://127.0.0.1:7171/mcp".into(), key: "pk_1".into() };
            let c = plan_add(client, &env, ENTRY, &http).unwrap();
            assert!(c.before.is_some() && c.after.as_deref().unwrap().contains("Bearer pk_1"), "{}", client.id);
            apply(&c).unwrap();
            assert_eq!(target(&current(client, &env, ENTRY).unwrap()).unwrap(), "http://127.0.0.1:7171/mcp");
            let c = plan_remove(client, &env, ENTRY).unwrap().unwrap();
            assert!(c.after.is_none());
            apply(&c).unwrap();
            assert!(current(client, &env, ENTRY).is_none(), "{}", client.id);
            assert!(std::fs::read_to_string(&file).unwrap().contains("other"));
            assert!(plan_remove(client, &env, ENTRY).unwrap().is_none());
        }
    }

    #[test]
    fn new_files_comments_and_bad_files() {
        let root = tempfile::tempdir().unwrap();
        let env = Env::at(root.path());
        let cursor = find("cursor").unwrap();
        assert!(!cursor.installed(&env));
        let c = plan_add(cursor, &env, ENTRY, &stdio()).unwrap();
        assert!(!c.existed);
        assert_eq!(apply(&c).unwrap(), None, "no backup of a file that did not exist");
        let zed = find("zed").unwrap();
        let f = zed.file(&env);
        std::fs::create_dir_all(f.parent().unwrap()).unwrap();
        std::fs::write(&f, "// Zed settings\n{\n  \"vim_mode\": true,\n}\n").unwrap();
        let c = plan_add(zed, &env, ENTRY, &stdio()).unwrap();
        assert!(c.drops_comments);
        let gemini = find("gemini-cli").unwrap();
        let f = gemini.file(&env);
        std::fs::create_dir_all(f.parent().unwrap()).unwrap();
        std::fs::write(&f, "{ not json").unwrap();
        assert!(plan_add(gemini, &env, ENTRY, &stdio()).unwrap_err().to_string().contains("not valid JSON"));
        assert_eq!(find("Claude").unwrap().id, "claude-code");
        assert!(find("emacs").is_none());
    }

    #[test]
    fn vscode_with_the_extension() {
        let root = tempfile::tempdir().unwrap();
        let env = Env::at(root.path());
        let vscode = find("vscode").unwrap();
        assert!(!vscode.registered_by_extension(&env));
        std::fs::create_dir_all(root.path().join(".vscode/extensions/polyph.polyph-vscode-0.1.1")).unwrap();
        assert!(vscode.registered_by_extension(&env));
        assert!(!find("cursor").unwrap().registered_by_extension(&env));
        let claude = find("claude-code").unwrap();
        std::fs::create_dir_all(root.path().join(".claude")).unwrap();
        std::fs::write(root.path().join(".claude/settings.json"), r#"{"enabledPlugins": {"polyph@polyph": false}}"#)
            .unwrap();
        assert!(!claude.registered_by_extension(&env));
        std::fs::write(root.path().join(".claude/settings.json"), r#"{"enabledPlugins": {"polyph@polyph": true}}"#)
            .unwrap();
        assert!(claude.registered_by_extension(&env));
    }

    #[test]
    fn entries_match_each_clients_format() {
        let t = Transport::Stdio { command: "polyph".into(), env: vec![("POLYPH_HOME".into(), "/h".into())] };
        let oc = entry(find("opencode").unwrap(), &t);
        assert_eq!(oc["command"], json!(["polyph", "mcp", "--client", "opencode"]));
        assert_eq!(oc["environment"]["POLYPH_HOME"], "/h");
        assert_eq!(entry(find("vscode").unwrap(), &t)["type"], "stdio");
        assert_eq!(entry(find("zed").unwrap(), &t)["source"], "custom");
        let h = Transport::Http { url: "u".into(), key: "k".into() };
        assert_eq!(entry(find("gemini").unwrap(), &h)["httpUrl"], "u");
        assert_eq!(entry(find("windsurf").unwrap(), &h)["serverUrl"], "u");
        assert_eq!(entry(find("claude-code").unwrap(), &h)["type"], "http");
    }
}

#[cfg(test)]
mod entry_name_tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    #[test]
    fn named_after_the_home_unless_default() {
        assert_eq!(entry_name(None), "polyph");
        let a = entry_name(Some(Path::new("/tmp/a")));
        let b = entry_name(Some(Path::new("/tmp/b")));
        assert!(a.starts_with("polyph-") && a.len() == "polyph-".len() + 8, "{a}");
        assert_ne!(a, b);
        assert_eq!(a, entry_name(Some(Path::new("/tmp/a"))));
    }

    #[test]
    fn two_homes_keep_separate_entries() {
        let root = tempfile::tempdir().unwrap();
        let env = Env::at(root.path());
        let client = find("claude-code").unwrap();
        let t = Transport::stdio(Path::new("/bin/polyph"), None);
        apply(&plan_add(client, &env, ENTRY, &t).unwrap()).unwrap();
        let home = Path::new("/tmp/h");
        let t2 = Transport::stdio(Path::new("/dev/polyph"), Some(home));
        let name = entry_name(Some(home));
        apply(&plan_add(client, &env, &name, &t2).unwrap()).unwrap();
        assert_eq!(target(&current(client, &env, ENTRY).unwrap()).unwrap(), "/bin/polyph");
        let own = current(client, &env, &name).unwrap();
        assert_eq!(target(&own).unwrap(), "/dev/polyph");
        assert_eq!(entry_home(&own).unwrap(), "/tmp/h");
        assert_eq!(entry_home(&current(client, &env, ENTRY).unwrap()), None);
    }
}
