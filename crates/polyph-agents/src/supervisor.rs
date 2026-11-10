//! Runs one ACP agent process and services its requests.
//!
//! The daemon receives [`Output`]s: transcript events, and [`Request`]s that
//! need a decision (permission, file writes, commands). Everything the agent
//! does to the outside world passes through one of those requests, so claims
//! and approvals are enforced here (layer 2) on top of the sandbox.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Arc;
use std::time::Duration;

use polyph_proto::messages::{AgentEventKind, ToolCall, ToolKind, ToolStatus};
use polyph_sandbox::Wrapped;
use serde_json::{Value, json};
use tokio::io::AsyncReadExt;
use tokio::process::{Child, Command};
use tokio::sync::{Mutex, mpsc, oneshot};
use tokio::task::JoinHandle;
use tracing::debug;

use crate::convert::Converter;
use crate::policy::{Action, relative_in};
use crate::rpc::{self, Incoming, Peer, RpcError};
use crate::wire::*;

/// What the daemon decided about an action.
#[derive(Clone, Debug, PartialEq)]
pub enum Answer {
    Allow { always: bool },
    Deny { note: Option<String> },
}

#[derive(Debug)]
pub enum Request {
    /// Decide whether `action` may happen. `tool` describes it for humans.
    Permission { action: Action, tool: Box<ToolCall>, reply: oneshot::Sender<Answer> },
}

#[derive(Debug)]
pub enum Output {
    Event(AgentEventKind),
    Request(Request),
    /// Answered once every earlier output has been handled.
    Barrier(oneshot::Sender<()>),
}

/// Wraps a command for the sandbox, the same way the agent itself was wrapped.
pub type CommandWrapper = Arc<dyn Fn(&str, &[String]) -> anyhow::Result<Wrapped> + Send + Sync>;

/// Bytes of terminal output kept per command.
const TERMINAL_LIMIT: usize = 1024 * 1024;

#[derive(Default)]
struct Terminal {
    output: Vec<u8>,
    truncated: bool,
    exit: Option<ExitStatus>,
    child: Option<Child>,
    done: Option<tokio::sync::watch::Receiver<bool>>,
}

type Terminals = Arc<Mutex<HashMap<String, Arc<Mutex<Terminal>>>>>;

pub struct AgentProcess {
    peer: Peer,
    child: Mutex<Child>,
    pub session_id: String,
    conv: Arc<Mutex<Converter>>,
    out: mpsc::Sender<Output>,
    tasks: Vec<JoinHandle<()>>,
}

impl std::fmt::Debug for AgentProcess {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "AgentProcess({})", self.session_id)
    }
}

impl Drop for AgentProcess {
    fn drop(&mut self) {
        for t in &self.tasks {
            t.abort();
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum StartError {
    #[error("could not start {program}: {reason}")]
    Spawn { program: String, reason: String },
    #[error("the agent did not complete the ACP handshake: {0}")]
    Handshake(String),
}

fn tool_for(fields: &ToolCallFields, conv_kind: ToolKind) -> ToolCall {
    ToolCall {
        id: fields.tool_call_id.clone(),
        kind: conv_kind,
        title: fields.title.clone().unwrap_or_default(),
        status: ToolStatus::Pending,
        paths: fields.locations.iter().flatten().map(|l| l.path.clone()).collect(),
        command: fields.raw_input.as_ref().and_then(|v| v.get("command")).and_then(|c| c.as_str()).map(str::to_string),
        query: None,
        lines_added: None,
        lines_removed: None,
        matches: None,
        tests_passed: None,
        tests_failed: None,
        exit_code: None,
        diff: None,
        output: None,
    }
}

/// Maps an ACP permission request to the action Polyph's policy reasons about.
pub fn action_for(fields: &ToolCallFields) -> Action {
    let title = fields.title.clone().unwrap_or_default();
    let kind = crate::convert::tool_kind(fields.kind.as_deref(), &title);
    let paths: Vec<PathBuf> = fields
        .locations
        .iter()
        .flatten()
        .map(|l| PathBuf::from(&l.path))
        .chain(
            fields
                .raw_input
                .as_ref()
                .and_then(|v| v.get("file_path").or_else(|| v.get("path")))
                .and_then(|p| p.as_str())
                .map(PathBuf::from),
        )
        .collect();
    match kind {
        ToolKind::Edit | ToolKind::Delete | ToolKind::Move => Action::Edit { paths, content: None },
        ToolKind::Read | ToolKind::Search => match paths.first() {
            Some(p) => Action::Read { path: p.clone() },
            None => Action::Other { title },
        },
        ToolKind::Execute => {
            let cmd = fields
                .raw_input
                .as_ref()
                .and_then(|v| v.get("command"))
                .map(|c| match c {
                    Value::String(s) => s.clone(),
                    Value::Array(a) => a.iter().filter_map(|x| x.as_str()).collect::<Vec<_>>().join(" "),
                    other => other.to_string(),
                })
                .unwrap_or(title);
            Action::Execute { command: cmd }
        }
        ToolKind::Fetch => {
            let url = fields
                .raw_input
                .as_ref()
                .and_then(|v| v.get("url"))
                .and_then(|u| u.as_str())
                .unwrap_or_default()
                .to_string();
            let host = url.split("://").nth(1).unwrap_or(&url).split(['/', ':']).next().unwrap_or_default().to_string();
            Action::Network { host, port: if url.starts_with("http://") { 80 } else { 443 } }
        }
        ToolKind::Mcp if title.contains("polyph") => Action::PolyphTool { name: title },
        _ => Action::Other { title },
    }
}

async fn ask(out: &mpsc::Sender<Output>, action: Action, tool: ToolCall) -> Answer {
    let (tx, rx) = oneshot::channel();
    if out.send(Output::Request(Request::Permission { action, tool: Box::new(tool), reply: tx })).await.is_err() {
        return Answer::Deny { note: None };
    }
    rx.await.unwrap_or(Answer::Deny { note: None })
}

impl AgentProcess {
    /// Spawns the (already sandbox-wrapped) agent and opens an ACP session.
    pub async fn start(
        spawn: &Wrapped,
        worktree: PathBuf,
        mcp: Vec<McpServerStdio>,
        out: mpsc::Sender<Output>,
        wrap: CommandWrapper,
    ) -> Result<Self, StartError> {
        let mut cmd = Command::new(&spawn.program);
        cmd.args(&spawn.args)
            .current_dir(&spawn.cwd)
            .envs(spawn.env.iter().cloned())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        #[cfg(unix)]
        {
            // SAFETY: setsid(2) only changes session/process-group state for the child.
            #[allow(unsafe_code)]
            unsafe {
                cmd.pre_exec(|| {
                    libc::setsid();
                    Ok(())
                });
            }
        }
        let mut child =
            cmd.spawn().map_err(|e| StartError::Spawn { program: spawn.program.clone(), reason: e.to_string() })?;
        let (Some(stdin), Some(stdout), Some(mut stderr)) =
            (child.stdin.take(), child.stdout.take(), child.stderr.take())
        else {
            return Err(StartError::Spawn { program: spawn.program.clone(), reason: "no stdio".into() });
        };
        // Drain stderr so a chatty agent never blocks; it is not logged
        // because it can contain content.
        // POLYPH_DEBUG_AGENT_STDERR=<file> copies it to a local file for debugging.
        let debug_file = std::env::var_os("POLYPH_DEBUG_AGENT_STDERR");
        let drain = tokio::spawn(async move {
            let mut buf = [0u8; 8192];
            let mut sink = debug_file.and_then(|p| std::fs::OpenOptions::new().create(true).append(true).open(p).ok());
            while let Ok(n) = stderr.read(&mut buf).await {
                if n == 0 {
                    break;
                }
                if let Some(f) = sink.as_mut() {
                    use std::io::Write;
                    let _ = f.write_all(&buf[..n]);
                }
            }
        });
        let conv = Arc::new(Mutex::new(Converter::default()));
        let on_notify: rpc::NotificationHandler = {
            let (conv, out) = (conv.clone(), out.clone());
            Arc::new(move |m: String, params: Value| {
                let (conv, out) = (conv.clone(), out.clone());
                Box::pin(async move {
                    if m != method::SESSION_UPDATE {
                        return;
                    }
                    if let Ok(n) = serde_json::from_value::<SessionNotification>(params) {
                        let events = conv.lock().await.update(&n.update);
                        for e in events {
                            let _ = out.send(Output::Event(e)).await;
                        }
                    }
                })
            })
        };
        let (peer, incoming, mut tasks) = rpc::connect_with(stdout, stdin, Some(on_notify));
        tasks.push(drain);

        let terminals: Terminals = Arc::new(Mutex::new(HashMap::new()));
        tasks.push(tokio::spawn(serve_incoming(
            incoming,
            out.clone(),
            conv.clone(),
            terminals,
            worktree.clone(),
            wrap,
        )));

        let init = InitializeRequest {
            protocol_version: PROTOCOL_VERSION,
            client_capabilities: ClientCapabilities {
                fs: FsCapabilities { read_text_file: true, write_text_file: true },
                terminal: true,
            },
            client_info: Some(Implementation { name: "polyph".into(), version: env!("CARGO_PKG_VERSION").into() }),
        };
        let _resp: InitializeResponse =
            tokio::time::timeout(Duration::from_secs(60), peer.request(method::INITIALIZE, &init))
                .await
                .map_err(|_| StartError::Handshake("timed out".into()))?
                .map_err(|e| StartError::Handshake(e.to_string()))?;
        let new = NewSessionRequest { cwd: worktree.to_string_lossy().into_owned(), mcp_servers: mcp };
        let s: NewSessionResponse =
            tokio::time::timeout(Duration::from_secs(60), peer.request(method::SESSION_NEW, &new))
                .await
                .map_err(|_| StartError::Handshake("timed out".into()))?
                .map_err(|e| StartError::Handshake(e.to_string()))?;
        Ok(Self { peer, child: Mutex::new(child), session_id: s.session_id, conv, out, tasks })
    }

    /// Sends a prompt and waits for the turn to end. Transcript events stream
    /// to the output channel while it runs.
    pub async fn prompt(&self, text: &str) -> Result<PromptResponse, RpcError> {
        let req = PromptRequest {
            session_id: self.session_id.clone(),
            prompt: vec![ContentBlock::Text { text: text.to_string() }],
        };
        let r: Result<PromptResponse, RpcError> = self.peer.request(method::SESSION_PROMPT, &req).await;
        for ev in self.conv.lock().await.flush() {
            let _ = self.out.send(Output::Event(ev)).await;
        }
        // Wait until the daemon has handled everything from this turn.
        let (tx, rx) = oneshot::channel();
        if self.out.send(Output::Barrier(tx)).await.is_ok() {
            let _ = rx.await;
        }
        r
    }

    pub async fn cancel(&self) {
        let _ = self.peer.notify(method::SESSION_CANCEL, &json!({ "sessionId": self.session_id })).await;
    }

    /// Asks the agent to switch models (router), if it supports it.
    pub async fn set_model(&self, model: &str) -> bool {
        self.peer
            .request::<_, Value>(method::SESSION_SET_MODEL, &json!({ "sessionId": self.session_id, "modelId": model }))
            .await
            .is_ok()
    }

    pub async fn stop(&self) {
        self.cancel().await;
        let mut c = self.child.lock().await;
        if tokio::time::timeout(Duration::from_millis(500), c.wait()).await.is_err() {
            let _ = c.kill().await;
        }
    }

    pub async fn exited(&self) -> bool {
        matches!(self.child.lock().await.try_wait(), Ok(Some(_)))
    }
}

fn err(code: i64, msg: impl Into<String>) -> Result<Value, (i64, String)> {
    Err((code, msg.into()))
}

async fn serve_incoming(
    mut rx: mpsc::Receiver<Incoming>,
    out: mpsc::Sender<Output>,
    conv: Arc<Mutex<Converter>>,
    terminals: Terminals,
    worktree: PathBuf,
    wrap: CommandWrapper,
) {
    while let Some(req) = rx.recv().await {
        let method_name = req.method.clone();
        if method_name == method::SESSION_UPDATE {
            if let Ok(n) = serde_json::from_value::<SessionNotification>(req.params.clone()) {
                let events = conv.lock().await.update(&n.update);
                for e in events {
                    let _ = out.send(Output::Event(e)).await;
                }
            }
            req.respond(Ok(Value::Null));
            continue;
        }
        // Requests can block on a human; handle each one on its own task.
        let (out, terminals, worktree, wrap, conv) =
            (out.clone(), terminals.clone(), worktree.clone(), wrap.clone(), conv.clone());
        tokio::spawn(async move {
            let params = req.params.clone();
            let r = match method_name.as_str() {
                method::REQUEST_PERMISSION => permission(params, &out).await,
                method::FS_READ => read_file(params, &worktree, &out).await,
                method::FS_WRITE => write_file(params, &worktree, &out).await,
                method::TERMINAL_CREATE => terminal_create(params, &worktree, &out, &terminals, &wrap, &conv).await,
                method::TERMINAL_OUTPUT => terminal_output(params, &terminals).await,
                method::TERMINAL_WAIT => terminal_wait(params, &terminals).await,
                method::TERMINAL_KILL => terminal_kill(params, &terminals, false).await,
                method::TERMINAL_RELEASE => terminal_kill(params, &terminals, true).await,
                other => {
                    debug!(method = other, "unsupported agent request");
                    err(-32601, format!("method {other} is not supported by Polyph"))
                }
            };
            req.respond(r);
        });
    }
}

async fn permission(params: Value, out: &mpsc::Sender<Output>) -> Result<Value, (i64, String)> {
    let Ok(p) = serde_json::from_value::<RequestPermissionRequest>(params) else {
        return err(-32602, "invalid permission request");
    };
    let action = action_for(&p.tool_call);
    let title = p.tool_call.title.clone().unwrap_or_default();
    let tool = tool_for(&p.tool_call, crate::convert::tool_kind(p.tool_call.kind.as_deref(), &title));
    let answer = ask(out, action, tool).await;
    let pick =
        |kinds: &[&str]| p.options.iter().find(|o| kinds.contains(&o.kind.as_str())).map(|o| o.option_id.clone());
    let outcome = match answer {
        Answer::Allow { always: true } => pick(&["allow_always", "allow_once"]),
        Answer::Allow { always: false } => pick(&["allow_once", "allow_always"]),
        Answer::Deny { .. } => pick(&["reject_once", "reject_always"]),
    };
    let resp = match outcome {
        Some(option_id) => RequestPermissionResponse { outcome: PermissionOutcome::Selected { option_id } },
        None => RequestPermissionResponse { outcome: PermissionOutcome::Cancelled },
    };
    serde_json::to_value(resp).map_err(|e| (-32603, e.to_string()))
}

fn resolve(worktree: &Path, p: &str) -> (PathBuf, bool) {
    let path = PathBuf::from(p);
    let abs = if path.is_absolute() { path } else { worktree.join(path) };
    let inside = relative_in(worktree, &abs).is_some();
    (abs, inside)
}

fn file_tool(id: &str, kind: ToolKind, path: &str) -> ToolCall {
    let mut t = tool_for(&ToolCallFields { tool_call_id: id.into(), ..Default::default() }, kind);
    t.title = path.to_string();
    t.paths = vec![path.to_string()];
    t
}

async fn read_file(params: Value, worktree: &Path, out: &mpsc::Sender<Output>) -> Result<Value, (i64, String)> {
    let Ok(p) = serde_json::from_value::<ReadTextFileRequest>(params) else {
        return err(-32602, "invalid read request");
    };
    let (abs, inside) = resolve(worktree, &p.path);
    if !inside
        && let Answer::Deny { note } =
            ask(out, Action::Read { path: abs.clone() }, file_tool("fs-read", ToolKind::Read, &p.path)).await
    {
        return err(
            -32000,
            note.unwrap_or_else(|| format!("reading {} outside the worktree was not approved", p.path)),
        );
    }
    let text = tokio::fs::read_to_string(&abs).await.map_err(|e| (-32000, format!("cannot read {}: {e}", p.path)))?;
    let content = match (p.line, p.limit) {
        (None, None) => text,
        (line, limit) => {
            let start = line.unwrap_or(1).saturating_sub(1) as usize;
            let n = limit.map(|l| l as usize).unwrap_or(usize::MAX);
            text.lines().skip(start).take(n).collect::<Vec<_>>().join("\n")
        }
    };
    Ok(json!({ "content": content }))
}

async fn write_file(params: Value, worktree: &Path, out: &mpsc::Sender<Output>) -> Result<Value, (i64, String)> {
    let Ok(p) = serde_json::from_value::<WriteTextFileRequest>(params) else {
        return err(-32602, "invalid write request");
    };
    let (abs, _) = resolve(worktree, &p.path);
    match ask(
        out,
        Action::Edit { paths: vec![abs.clone()], content: Some(p.content.clone()) },
        file_tool("fs-write", ToolKind::Edit, &p.path),
    )
    .await
    {
        Answer::Deny { note } => err(-32000, note.unwrap_or_else(|| format!("writing {} was not allowed", p.path))),
        Answer::Allow { .. } => {
            if let Some(parent) = abs.parent() {
                let _ = tokio::fs::create_dir_all(parent).await;
            }
            tokio::fs::write(&abs, p.content.as_bytes())
                .await
                .map_err(|e| (-32000, format!("cannot write {}: {e}", p.path)))?;
            Ok(Value::Null)
        }
    }
}

fn shell_join(command: &str, args: &[String]) -> String {
    let mut s = command.to_string();
    for a in args {
        s.push(' ');
        if a.contains(char::is_whitespace) {
            s.push_str(&format!("'{}'", a.replace('\'', "'\\''")))
        } else {
            s.push_str(a)
        }
    }
    s
}

async fn terminal_create(
    params: Value,
    worktree: &Path,
    out: &mpsc::Sender<Output>,
    terminals: &Terminals,
    wrap: &CommandWrapper,
    conv: &Arc<Mutex<Converter>>,
) -> Result<Value, (i64, String)> {
    let Ok(p) = serde_json::from_value::<CreateTerminalRequest>(params) else {
        return err(-32602, "invalid terminal request");
    };
    let line = shell_join(&p.command, &p.args);
    let mut tool = tool_for(
        &ToolCallFields { tool_call_id: "terminal".into(), title: Some(line.clone()), ..Default::default() },
        ToolKind::Execute,
    );
    tool.command = Some(line.clone());
    if let Answer::Deny { note } = ask(out, Action::Execute { command: line.clone() }, tool).await {
        return err(-32000, note.unwrap_or_else(|| format!("running `{line}` was not approved")));
    }
    let cwd = p.cwd.as_deref().map(|c| resolve(worktree, c).0).unwrap_or_else(|| worktree.to_path_buf());
    let w = wrap(&p.command, &p.args).map_err(|e| (-32000, e.to_string()))?;
    let mut cmd = Command::new(&w.program);
    cmd.args(&w.args)
        .current_dir(if w.mechanism == polyph_sandbox::Mechanism::Bubblewrap { &w.cwd } else { &cwd })
        .envs(w.env.iter().cloned());
    for e in &p.env {
        cmd.env(&e.name, &e.value);
    }
    cmd.stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped()).kill_on_drop(true);
    let mut child = cmd.spawn().map_err(|e| (-32000, format!("cannot run {}: {e}", p.command)))?;
    let id = format!("term-{}", polyph_proto::ids::ObjectId::random().map(|o| o.short()).unwrap_or_default());
    let limit = p.output_byte_limit.map(|l| l as usize).unwrap_or(TERMINAL_LIMIT).min(TERMINAL_LIMIT);
    let (done_tx, done_rx) = tokio::sync::watch::channel(false);
    let term = Arc::new(Mutex::new(Terminal { done: Some(done_rx), ..Default::default() }));
    let (stdout, stderr) = (child.stdout.take(), child.stderr.take());
    term.lock().await.child = Some(child);
    terminals.lock().await.insert(id.clone(), term.clone());
    let (t2, conv2, id2) = (term.clone(), conv.clone(), id.clone());
    tokio::spawn(async move {
        let pump = |mut r: Box<dyn tokio::io::AsyncRead + Unpin + Send>, t: Arc<Mutex<Terminal>>| async move {
            let mut buf = [0u8; 8192];
            while let Ok(n) = r.read(&mut buf).await {
                if n == 0 {
                    break;
                }
                let mut g = t.lock().await;
                g.output.extend_from_slice(&buf[..n]);
                if g.output.len() > limit {
                    let cut = g.output.len() - limit;
                    g.output.drain(..cut);
                    g.truncated = true;
                }
            }
        };
        let a = stdout.map(|s| tokio::spawn(pump(Box::new(s), t2.clone())));
        let b = stderr.map(|s| tokio::spawn(pump(Box::new(s), t2.clone())));
        for h in [a, b].into_iter().flatten() {
            let _ = h.await;
        }
        let status = {
            let mut g = t2.lock().await;
            match g.child.as_mut() {
                Some(c) => c.wait().await.ok(),
                None => None,
            }
        };
        let mut g = t2.lock().await;
        g.exit = Some(ExitStatus { exit_code: status.and_then(|s| s.code()), signal: None });
        let text = String::from_utf8_lossy(&g.output).into_owned();
        conv2.lock().await.terminal_output.insert(id2, (text, g.exit.as_ref().and_then(|e| e.exit_code)));
        let _ = done_tx.send(true);
    });
    Ok(json!({ "terminalId": id }))
}

async fn get_term(params: &Value, terminals: &Terminals) -> Result<Arc<Mutex<Terminal>>, (i64, String)> {
    let Ok(r) = serde_json::from_value::<TerminalRef>(params.clone()) else {
        return Err((-32602, "invalid terminal reference".into()));
    };
    terminals.lock().await.get(&r.terminal_id).cloned().ok_or((-32000, "no such terminal".into()))
}

async fn terminal_output(params: Value, terminals: &Terminals) -> Result<Value, (i64, String)> {
    let t = get_term(&params, terminals).await?;
    let g = t.lock().await;
    Ok(json!({ "output": String::from_utf8_lossy(&g.output), "truncated": g.truncated, "exitStatus": g.exit }))
}

async fn terminal_wait(params: Value, terminals: &Terminals) -> Result<Value, (i64, String)> {
    let t = get_term(&params, terminals).await?;
    let mut rx = t.lock().await.done.clone().ok_or((-32000, "terminal has no process".into()))?;
    while !*rx.borrow() {
        if rx.changed().await.is_err() {
            break;
        }
    }
    let g = t.lock().await;
    serde_json::to_value(g.exit.clone().unwrap_or_default()).map_err(|e| (-32603, e.to_string()))
}

async fn terminal_kill(params: Value, terminals: &Terminals, release: bool) -> Result<Value, (i64, String)> {
    let t = get_term(&params, terminals).await?;
    if let Some(c) = t.lock().await.child.as_mut() {
        let _ = c.start_kill();
    }
    if release && let Ok(r) = serde_json::from_value::<TerminalRef>(params) {
        terminals.lock().await.remove(&r.terminal_id);
    }
    Ok(Value::Null)
}
