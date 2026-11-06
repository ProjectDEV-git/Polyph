//! Shared terminals. A terminal runs on its owner's device (a PTY through
//! `polyph-term`); its output goes to the room, and every member keeps
//! scrollback for it. Only the owner's device writes to the PTY. The owner
//! types freely; anyone else asks for the keyboard (`RequestInput`), which
//! becomes an approval for the owner, and types only while they hold it.
//! The owner can take the keyboard back at any time.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use base64::Engine as _;
use polyph_proto::ids::{DeviceId, MemberId, ObjectId};
use polyph_proto::messages::{AgentRef, ApprovalKind, Decision, RoomMessage, TerminalMsg};
use polyph_term::{Scrollback, Term};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

use crate::daemon::{ApiError, Daemon, Shared};
use crate::events::ApiEvent;
use crate::runtime::{RoomCtx, RoomRuntime};

/// Scrollback kept per terminal, in bytes.
pub const SCROLLBACK: usize = 256 * 1024;
/// Output is gathered for this long before it is sent.
const BATCH: Duration = Duration::from_millis(25);
/// Largest output message.
const MAX_CHUNK: usize = 64 * 1024;

#[derive(Debug, Clone, Serialize)]
pub struct TermInfo {
    pub id: ObjectId,
    pub owner: MemberId,
    pub device: DeviceId,
    pub title: String,
    pub cols: u16,
    pub rows: u16,
    /// Member who holds the keyboard besides the owner.
    pub writer: Option<MemberId>,
    pub closed: bool,
    pub exit: Option<u32>,
}

#[derive(Debug)]
pub struct TermState {
    pub info: TermInfo,
    pub scrollback: Scrollback,
    seq: u64,
    /// Members whose request is waiting for the owner.
    asking: Vec<MemberId>,
}

struct Local {
    term: Arc<Term>,
    pump: JoinHandle<()>,
}

impl Drop for Local {
    fn drop(&mut self) {
        self.pump.abort();
        self.term.kill();
    }
}

#[derive(Default)]
pub struct Terminals {
    pub all: HashMap<ObjectId, TermState>,
    local: HashMap<ObjectId, Local>,
}

impl std::fmt::Debug for Terminals {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Terminals({} known, {} here)", self.all.len(), self.local.len())
    }
}

fn b64(d: &[u8]) -> String {
    base64::engine::general_purpose::STANDARD.encode(d)
}

/// The unfinished last word of a batch is held back (up to this many bytes)
/// and redacted together with what follows, so a secret the program writes
/// in pieces (a shell echoing typed keys) is still recognised.
const HOLD_MAX: usize = 512;
/// How long held-back output waits for more before it is sent anyway.
const HOLD: Duration = Duration::from_millis(150);

/// Splits off the unfinished last word of `data` (everything after the last
/// whitespace), unless it is longer than [`HOLD_MAX`].
fn take_tail(data: &mut Vec<u8>) -> Vec<u8> {
    let start = data.iter().rposition(|b| b.is_ascii_whitespace()).map_or(0, |i| i + 1);
    if data.len() - start > HOLD_MAX {
        return vec![];
    }
    data.split_off(start)
}

/// Output as sent: secrets redacted where the chunk is text.
fn clean(data: Vec<u8>) -> Vec<u8> {
    match String::from_utf8(data) {
        Ok(s) => polyph_core::redact::redact(&s).into_owned().into_bytes(),
        Err(e) => e.into_bytes(),
    }
}

fn emit(shared: &Shared, ctx: &RoomCtx, kind: &str, v: Value) {
    shared.emit(ApiEvent::room_kind(&ctx.core.id, kind, v));
}

/// Handles a terminal message from the room (inside the room's lock).
pub fn handle(ctx: &mut RoomCtx, shared: &Shared, from: DeviceId, member: MemberId, msg: TerminalMsg) {
    let me = ctx.core.me;
    match msg {
        TerminalMsg::Opened { id, owner, title, agent: _, cols, rows } => {
            ctx.terms.all.entry(id).or_insert(TermState {
                info: TermInfo { id, owner, device: from, title, cols, rows, writer: None, closed: false, exit: None },
                scrollback: Scrollback::new(SCROLLBACK),
                seq: 0,
                asking: vec![],
            });
            let info = ctx.terms.all.get(&id).map(|t| t.info.clone());
            emit(shared, ctx, "terminal_opened", json!({ "terminal": info }));
        }
        TerminalMsg::Output { id, seq, data } => {
            let Some(t) = ctx.terms.all.get_mut(&id).filter(|t| t.info.device == from) else { return };
            t.scrollback.push(&data);
            t.seq = seq;
            emit(shared, ctx, "terminal_output", json!({ "id": id, "seq": seq, "data": b64(&data) }));
        }
        TerminalMsg::Resize { id, cols, rows } => {
            let Some(t) = ctx.terms.all.get_mut(&id).filter(|t| t.info.device == from) else { return };
            t.info.cols = cols;
            t.info.rows = rows;
            emit(shared, ctx, "terminal_resized", json!({ "id": id, "cols": cols, "rows": rows }));
        }
        TerminalMsg::Control { id, writer } => {
            // The owner's device decides; the holder may only give it back.
            let Some(t) = ctx
                .terms
                .all
                .get_mut(&id)
                .filter(|t| t.info.device == from || (writer.is_none() && t.info.writer == Some(member)))
            else {
                return;
            };
            t.info.writer = writer;
            t.asking.retain(|m| Some(*m) != writer);
            emit(shared, ctx, "terminal_control", json!({ "id": id, "writer": writer }));
        }
        TerminalMsg::Closed { id, exit } => {
            let Some(t) = ctx.terms.all.get_mut(&id).filter(|t| t.info.device == from) else { return };
            t.info.closed = true;
            t.info.exit = exit;
            t.info.writer = None;
            emit(shared, ctx, "terminal_closed", json!({ "id": id, "exit": exit }));
        }
        // Only the owner's device acts on these.
        TerminalMsg::Input { id, typist, data } => {
            let Some(t) = ctx.terms.all.get(&id).filter(|t| t.info.device == me && !t.info.closed) else { return };
            if t.info.writer == Some(typist)
                && typist == member
                && let Some(l) = ctx.terms.local.get(&id)
            {
                let _ = l.term.write(&data);
            }
        }
        TerminalMsg::RequestInput { id, member: asker } => {
            let Some(t) = ctx.terms.all.get_mut(&id).filter(|t| t.info.device == me && !t.info.closed) else { return };
            if t.asking.contains(&asker) || t.info.writer == Some(asker) {
                return;
            }
            t.asking.push(asker);
            let (title, owner) = (t.info.title.clone(), t.info.owner);
            let name =
                crate::daemon::member_names(&ctx.core).get(&asker.to_hex()).cloned().unwrap_or_else(|| asker.short());
            let room = ctx.core.id;
            let shared_daemon = shared.daemon.get().cloned();
            tokio::spawn(async move {
                let Some(d) = shared_daemon.and_then(|w| w.upgrade()) else { return };
                let Ok(rt) = d.find(&room.to_hex()).await else { return };
                ask_owner(&d, &rt, id, owner, asker, &name, &title).await;
            });
        }
    }
}

/// Turns a request for the keyboard into an approval for the owner.
async fn ask_owner(
    d: &Arc<Daemon>,
    rt: &Arc<RoomRuntime>,
    id: ObjectId,
    owner: MemberId,
    asker: MemberId,
    name: &str,
    title: &str,
) {
    // Approvals are addressed to an agent's owner; the terminal stands in.
    let who = AgentRef {
        id,
        member: owner,
        device: d.shared.identity.device_id(),
        kind: "terminal".into(),
        label: format!("{name} on {title}"),
    };
    let decision =
        crate::agents::request_approval(rt, &d.shared, &who, ApprovalKind::TerminalInput { terminal: id }, None).await;
    let mut ctx = rt.ctx.lock().await;
    let granted = matches!(decision, Decision::Approve { .. });
    if let Some(t) = ctx.terms.all.get_mut(&id) {
        t.asking.retain(|m| *m != asker);
        if !granted || t.info.closed {
            emit(&d.shared, &ctx, "terminal_input_denied", json!({ "id": id, "member": asker }));
            return;
        }
        t.info.writer = Some(asker);
    } else {
        return;
    }
    crate::runtime::send(&mut ctx, &RoomMessage::Terminal(TerminalMsg::Control { id, writer: Some(asker) })).await;
    emit(&d.shared, &ctx, "terminal_control", json!({ "id": id, "writer": asker }));
}

#[derive(Debug, Deserialize)]
pub struct OpenParams {
    pub room: String,
    pub cwd: Option<PathBuf>,
    /// Program and arguments; default the user's shell.
    #[serde(default)]
    pub command: Vec<String>,
    pub title: Option<String>,
    pub cols: Option<u16>,
    pub rows: Option<u16>,
}

fn default_shell() -> Vec<String> {
    if cfg!(windows) {
        vec!["powershell.exe".into(), "-NoLogo".into()]
    } else {
        vec![std::env::var("SHELL").unwrap_or_else(|_| "sh".into())]
    }
}

/// Opens a terminal on this device and shares it with the room.
pub async fn open(d: &Arc<Daemon>, p: OpenParams) -> Result<Value, ApiError> {
    let rt = d.find(&p.room).await?;
    let cwd = match p.cwd {
        Some(c) => polyph_git::canonical(&c).map_err(|e| ApiError::BadRequest(format!("{}: {e}", c.display())))?,
        None => polyph_sandbox::home_dir().unwrap_or_else(std::env::temp_dir),
    };
    let command = if p.command.is_empty() { default_shell() } else { p.command };
    let (cols, rows) = (p.cols.unwrap_or(100).clamp(10, 500), p.rows.unwrap_or(30).clamp(4, 300));
    let title = p.title.unwrap_or_else(|| command.join(" "));
    let title = polyph_core::redact::redact(&title).into_owned();
    let (prog, args) = command.split_first().ok_or_else(|| ApiError::BadRequest("empty command".into()))?;
    let (term, out) =
        Term::spawn(prog, args, &cwd, &[], cols, rows).map_err(|e| ApiError::Failed(format!("{prog}: {e}")))?;
    let id = ObjectId::random().map_err(|e| ApiError::Failed(e.to_string()))?;
    let mut ctx = rt.ctx.lock().await;
    if !ctx.core.role().can_write() {
        return Err(ApiError::Forbidden("viewers cannot open terminals".into()));
    }
    let owner = ctx.core.my_member;
    let info = TermInfo {
        id,
        owner,
        device: ctx.core.me,
        title: title.clone(),
        cols,
        rows,
        writer: None,
        closed: false,
        exit: None,
    };
    ctx.terms
        .all
        .insert(id, TermState { info: info.clone(), scrollback: Scrollback::new(SCROLLBACK), seq: 0, asking: vec![] });
    let pump = tokio::spawn(pump(rt.clone(), d.shared.clone(), id, term.clone(), out));
    ctx.terms.local.insert(id, Local { term, pump });
    crate::runtime::send(
        &mut ctx,
        &RoomMessage::Terminal(TerminalMsg::Opened { id, owner, title, agent: None, cols, rows }),
    )
    .await;
    emit(&d.shared, &ctx, "terminal_opened", json!({ "terminal": info }));
    crate::feed::log_locked(
        &rt,
        &mut ctx,
        &d.shared,
        "terminal_opened",
        None,
        json!({ "terminal": id.to_hex(), "title": info.title }),
    )
    .await;
    Ok(json!({ "id": id.to_hex(), "terminal": info }))
}

/// Sends a local terminal's output to the room in small batches.
async fn pump(
    rt: Arc<RoomRuntime>,
    shared: Arc<Shared>,
    id: ObjectId,
    term: Arc<Term>,
    mut out: mpsc::UnboundedReceiver<Vec<u8>>,
) {
    let mut held: Vec<u8> = vec![];
    loop {
        // `None`: the program ended. `Some(None)`: held output waited long
        // enough and goes out as it is.
        let next = if held.is_empty() {
            out.recv().await.map(Some)
        } else {
            match tokio::time::timeout(HOLD, out.recv()).await {
                Ok(next) => next.map(Some),
                Err(_) => Some(None),
            }
        };
        let mut data = std::mem::take(&mut held);
        let ended = next.is_none();
        if let Some(Some(first)) = next {
            data.extend(first);
            let deadline = tokio::time::Instant::now() + BATCH;
            while data.len() < MAX_CHUNK {
                match tokio::time::timeout_at(deadline, out.recv()).await {
                    Ok(Some(more)) => data.extend(more),
                    _ => break,
                }
            }
            held = take_tail(&mut data);
        }
        if data.is_empty() {
            if ended {
                break;
            }
            continue;
        }
        let data = clean(data);
        let mut ctx = rt.ctx.lock().await;
        let Some(t) = ctx.terms.all.get_mut(&id) else { return };
        t.seq += 1;
        let seq = t.seq;
        t.scrollback.push(&data);
        emit(&shared, &ctx, "terminal_output", json!({ "id": id, "seq": seq, "data": b64(&data) }));
        crate::runtime::send(&mut ctx, &RoomMessage::Terminal(TerminalMsg::Output { id, seq, data })).await;
    }
    // The program ended.
    let mut exit = term.try_exit();
    for _ in 0..50 {
        if exit.is_some() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
        exit = term.try_exit();
    }
    let mut ctx = rt.ctx.lock().await;
    if let Some(t) = ctx.terms.all.get_mut(&id) {
        t.info.closed = true;
        t.info.exit = exit;
        t.info.writer = None;
    }
    crate::runtime::send(&mut ctx, &RoomMessage::Terminal(TerminalMsg::Closed { id, exit })).await;
    emit(&shared, &ctx, "terminal_closed", json!({ "id": id, "exit": exit }));
}

fn find<'a>(ctx: &'a mut RoomCtx, q: &str) -> Result<&'a mut TermState, ApiError> {
    let ql = q.to_lowercase();
    let ids: Vec<ObjectId> =
        ctx.terms.all.keys().filter(|k| !ql.is_empty() && k.to_hex().starts_with(&ql)).copied().collect();
    match ids.as_slice() {
        [id] => ctx.terms.all.get_mut(id).ok_or_else(|| ApiError::NotFound("no such terminal".into())),
        [] => Err(ApiError::NotFound("no such terminal".into())),
        _ => Err(ApiError::BadRequest("that id matches several terminals".into())),
    }
}

/// Bytes to type: `text` as UTF-8, or base64 `data`.
pub fn input_bytes(text: Option<String>, data: Option<String>) -> Result<Vec<u8>, ApiError> {
    match (text, data) {
        (Some(t), _) => Ok(t.into_bytes()),
        (None, Some(d)) => base64::engine::general_purpose::STANDARD
            .decode(d)
            .map_err(|_| ApiError::BadRequest("data is not base64".into())),
        (None, None) => Err(ApiError::BadRequest("give text or data".into())),
    }
}

pub async fn input(d: &Arc<Daemon>, room: &str, term: &str, bytes: Vec<u8>) -> Result<Value, ApiError> {
    let rt = d.find(room).await?;
    let mut ctx = rt.ctx.lock().await;
    let me = ctx.core.my_member;
    let here = ctx.core.me;
    let t = find(&mut ctx, term)?;
    if t.info.closed {
        return Err(ApiError::BadRequest("that terminal has closed".into()));
    }
    let id = t.info.id;
    if t.info.device == here {
        // The owner types on their own device.
        let l = ctx.terms.local.get(&id).ok_or_else(|| ApiError::Failed("the terminal is not running here".into()))?;
        l.term.write(&bytes)?;
        return Ok(json!({}));
    }
    if t.info.writer != Some(me) {
        return Err(ApiError::Forbidden("you do not have the keyboard; ask with `polyph term ask`".into()));
    }
    crate::runtime::send(&mut ctx, &RoomMessage::Terminal(TerminalMsg::Input { id, typist: me, data: bytes })).await;
    Ok(json!({}))
}

/// Ask for the keyboard, give it back (`writer: None` by the writer), or
/// take it back (owner).
pub async fn control(d: &Arc<Daemon>, room: &str, term: &str, action: &str) -> Result<Value, ApiError> {
    let rt = d.find(room).await?;
    let mut ctx = rt.ctx.lock().await;
    let me = ctx.core.my_member;
    let here = ctx.core.me;
    let t = find(&mut ctx, term)?;
    let id = t.info.id;
    let owner_here = t.info.device == here;
    match action {
        "ask" => {
            if t.info.owner == me {
                return Err(ApiError::BadRequest("it is your terminal; type in it".into()));
            }
            crate::runtime::send(&mut ctx, &RoomMessage::Terminal(TerminalMsg::RequestInput { id, member: me })).await;
            Ok(json!({ "asked": true }))
        }
        "release" | "revoke" => {
            let allowed = if action == "release" { t.info.writer == Some(me) } else { owner_here };
            if !allowed {
                return Err(ApiError::Forbidden(
                    "only the keyboard holder can give it back, and only the owner can take it".into(),
                ));
            }
            t.info.writer = None;
            crate::runtime::send(&mut ctx, &RoomMessage::Terminal(TerminalMsg::Control { id, writer: None })).await;
            emit(&d.shared, &ctx, "terminal_control", json!({ "id": id, "writer": null }));
            Ok(json!({}))
        }
        _ => Err(ApiError::BadRequest(format!("unknown action {action}"))),
    }
}

pub async fn resize(d: &Arc<Daemon>, room: &str, term: &str, cols: u16, rows: u16) -> Result<Value, ApiError> {
    let rt = d.find(room).await?;
    let mut ctx = rt.ctx.lock().await;
    let here = ctx.core.me;
    let t = find(&mut ctx, term)?;
    if t.info.device != here {
        return Err(ApiError::Forbidden("only the owner resizes a terminal".into()));
    }
    let (cols, rows) = (cols.clamp(10, 500), rows.clamp(4, 300));
    t.info.cols = cols;
    t.info.rows = rows;
    let id = t.info.id;
    if let Some(l) = ctx.terms.local.get(&id) {
        l.term.resize(cols, rows)?;
    }
    crate::runtime::send(&mut ctx, &RoomMessage::Terminal(TerminalMsg::Resize { id, cols, rows })).await;
    Ok(json!({}))
}

pub async fn close(d: &Arc<Daemon>, room: &str, term: &str) -> Result<Value, ApiError> {
    let rt = d.find(room).await?;
    let mut ctx = rt.ctx.lock().await;
    let here = ctx.core.me;
    let t = find(&mut ctx, term)?;
    if t.info.device != here {
        return Err(ApiError::Forbidden("only the owner closes a terminal".into()));
    }
    let id = t.info.id;
    // Ending the program ends the pump, which announces the close.
    if let Some(l) = ctx.terms.local.get(&id) {
        l.term.kill();
    }
    Ok(json!({}))
}

pub async fn list(rt: &RoomRuntime) -> Vec<TermInfo> {
    let ctx = rt.ctx.lock().await;
    let mut v: Vec<TermInfo> = ctx.terms.all.values().map(|t| t.info.clone()).collect();
    v.sort_by(|a, b| a.title.cmp(&b.title));
    v
}

pub async fn read(d: &Arc<Daemon>, room: &str, term: &str) -> Result<Value, ApiError> {
    let rt = d.find(room).await?;
    let mut ctx = rt.ctx.lock().await;
    let t = find(&mut ctx, term)?;
    let data = t.scrollback.contents();
    Ok(json!({ "terminal": t.info, "seq": t.seq, "data": b64(&data), "text": String::from_utf8_lossy(&data) }))
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    #[test]
    fn holds_the_unfinished_last_word() {
        let mut d = b"$ echo hi ghp_aBc".to_vec();
        assert_eq!(take_tail(&mut d), b"ghp_aBc");
        assert_eq!(d, b"$ echo hi ");
        let mut d = b"done\n".to_vec();
        assert!(take_tail(&mut d).is_empty());
        let mut d = vec![b'x'; HOLD_MAX + 1];
        assert!(take_tail(&mut d).is_empty(), "too long to hold");
        assert_eq!(d.len(), HOLD_MAX + 1);
    }

    /// A token echoed in two pieces is redacted once the pieces are joined.
    #[test]
    fn a_token_split_across_batches_is_redacted() {
        let key = ["gh", "p_aBcDeFgHiJkLmNoPqRsTuVwXyZ0123456789"].concat();
        let (one, two) = key.split_at(12);
        let mut first = format!("$ echo {one}").into_bytes();
        let held = take_tail(&mut first);
        let sent_first = String::from_utf8(clean(first)).unwrap();
        let mut second = held;
        second.extend(format!("{two}\n").into_bytes());
        let sent_second = String::from_utf8(clean(second)).unwrap();
        let all = format!("{sent_first}{sent_second}");
        assert!(!all.contains(one) || !all.contains(two), "{all}");
        assert!(all.contains("[REDACTED:github-token]"), "{all}");
    }
}
