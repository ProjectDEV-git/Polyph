//! Hosted mode: one shared working copy on the host's machine, mirrored to
//! every member who opens it. Members' agents work in their mirror; their
//! changes go to the host as proposals, which the host applies only if the
//! change was made on the current version of the file and nobody else holds
//! a claim on it. The host's own changes (its people and agents) are sent to
//! everyone. Nothing here touches git: the host's working copy is the truth.

use std::collections::HashMap;
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use notify::{EventKind, RecursiveMode, Watcher};
use polyph_core::docs;
use polyph_proto::ids::{DeviceId, ObjectId};
use polyph_proto::messages::{
    AgentRef, AgentState, ClaimTarget, DocName, MirrorEntry, MirrorFile, MirrorMsg, RoomMessage,
};
use serde::Serialize;
use serde_json::{Value, json};
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

use crate::daemon::{ApiError, Daemon, Shared};
use crate::runtime::{RoomCtx, RoomRuntime, apply};

/// Files larger than this are not mirrored (listed as too large).
pub const MAX_FILE: usize = 512 * 1024;
/// Most files mirrored from one working copy.
pub const MAX_FILES: usize = 20_000;
/// Content per `Files` message, under the envelope limit.
const CHUNK: usize = 700 * 1024;
/// Settings key naming the hosting device.
pub const SETTING: &str = "hosted";

pub type Hash = [u8; 32];

fn hash(data: &[u8]) -> Hash {
    *blake3::hash(data).as_bytes()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Side {
    Host,
    Member,
}

/// This device's part in a room's mirror.
#[derive(Debug)]
pub struct Mirror {
    pub side: Side,
    pub root: PathBuf,
    pub host: DeviceId,
    /// The host's version of every file (on the host: the working copy).
    pub files: HashMap<String, Hash>,
    pub too_large: Vec<String>,
    /// Member: proposals waiting for the host, by id: (path, agent).
    pending: HashMap<ObjectId, (String, Option<AgentRef>)>,
    /// Member: hash of the latest proposed content per path, so quick
    /// successive edits build on each other instead of on a stale base.
    inflight: HashMap<String, Hash>,
    watcher: Option<JoinHandle<()>>,
}

impl Drop for Mirror {
    fn drop(&mut self) {
        if let Some(w) = self.watcher.take() {
            w.abort();
        }
    }
}

/// A repository-relative path that stays inside the root, or None.
pub fn safe_path(p: &str) -> Option<String> {
    let path = Path::new(p);
    if p.is_empty() || p.contains('\0') || path.is_absolute() {
        return None;
    }
    // One name per file: no empty, `.` or `..` segments (the path parser
    // drops a `.` in the middle, so check the text too).
    if path.components().any(|c| !matches!(c, Component::Normal(_)))
        || p.split(['/', '\\']).any(|seg| seg.is_empty() || seg == "." || seg == "..")
    {
        return None;
    }
    let s = p.replace('\\', "/");
    crate::watch::relevant(&s).then_some(s)
}

/// Every mirrored file under `root`: (files with hashes, too large).
pub fn scan(root: &Path) -> (HashMap<String, Hash>, Vec<String>) {
    let mut found: Vec<String> = vec![];
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(rd) = std::fs::read_dir(&dir) else { continue };
        for e in rd.flatten() {
            let p = e.path();
            let Ok(rel) = p.strip_prefix(root) else { continue };
            let rel = rel.to_string_lossy().replace('\\', "/");
            if !crate::watch::relevant(&rel) {
                continue;
            }
            match e.file_type() {
                Ok(t) if t.is_dir() => stack.push(p),
                Ok(t) if t.is_file() => found.push(rel),
                _ => {}
            }
        }
        if found.len() > MAX_FILES {
            break;
        }
    }
    if let Ok(r) = polyph_git::Repo::open(root)
        && let Ok(ign) = r.ignored(&found)
    {
        found.retain(|f| !ign.contains(f));
    }
    found.truncate(MAX_FILES);
    let mut files = HashMap::new();
    let mut big = vec![];
    for f in found {
        match polyph_plugins::read_under(root, Path::new(&f), MAX_FILE as u64) {
            Ok(d) => {
                files.insert(f, hash(&d));
            }
            Err(e) if e.kind() == std::io::ErrorKind::FileTooLarge => big.push(f),
            Err(_) => {}
        }
    }
    big.sort();
    (files, big)
}

/// Whether the host applies a proposal: the base must be the current
/// version, and no one but the proposer may hold the file.
pub fn judge(current: Option<&Hash>, base: &[u8], holder: Option<&str>) -> Result<(), String> {
    match (current, base.is_empty()) {
        (None, true) => {}
        (Some(c), false) if c.as_slice() == base => {}
        (None, false) => return Err("the file no longer exists on the host".into()),
        _ => return Err("the file changed on the host first; your mirror has the new version now".into()),
    }
    match holder {
        Some(who) => Err(format!("{who} holds this file")),
        None => Ok(()),
    }
}

fn write_file(root: &Path, path: &str, data: &[u8], deleted: bool) {
    let p = root.join(path);
    if deleted {
        let _ = std::fs::remove_file(&p);
        return;
    }
    if let Some(d) = p.parent() {
        let _ = std::fs::create_dir_all(d);
    }
    let _ = std::fs::write(&p, data);
}

fn index(m: &Mirror) -> MirrorMsg {
    let mut files: Vec<MirrorEntry> = m.files.iter().map(|(p, h)| MirrorEntry { path: p.clone(), hash: *h }).collect();
    files.sort_by(|a, b| a.path.cmp(&b.path));
    MirrorMsg::Index { files, too_large: m.too_large.clone() }
}

/// Handles one mirror message inside the room's lock. Returns what to send.
pub fn handle(ctx: &mut RoomCtx, shared: &Shared, from: DeviceId, msg: MirrorMsg) -> Vec<MirrorMsg> {
    let now = shared.clock.now();
    let Some(m) = ctx.mirror.as_mut() else { return vec![] };
    let mut out = vec![];
    match (m.side, msg) {
        (Side::Host, MirrorMsg::IndexRequest) => out.push(index(m)),
        (Side::Host, MirrorMsg::Fetch { paths }) => {
            let mut batch = vec![];
            let mut size = 0;
            for p in paths {
                if !m.files.contains_key(&p) {
                    continue;
                }
                let Ok(data) = polyph_plugins::read_under(&m.root, Path::new(&p), MAX_FILE as u64) else { continue };
                if size + data.len() > CHUNK && !batch.is_empty() {
                    out.push(MirrorMsg::Files { files: std::mem::take(&mut batch) });
                    size = 0;
                }
                size += data.len();
                batch.push(MirrorFile { path: p, data });
            }
            if !batch.is_empty() {
                out.push(MirrorMsg::Files { files: batch });
            }
        }
        (Side::Host, MirrorMsg::Propose { id, agent, path, base, data, deleted }) => {
            let Some(path) = safe_path(&path).filter(|_| data.len() <= MAX_FILE) else {
                return vec![MirrorMsg::Rejected { id, path, reason: "that path or size cannot be mirrored".into() }];
            };
            // Free, or held by an agent on the proposing device (its own
            // agent, or its person editing next to that agent).
            let target = ClaimTarget { path: path.clone(), symbol: None };
            let holder = ctx
                .core
                .claims
                .all()
                .into_iter()
                .filter(|c| c.expires_at > now && c.owner.device != from)
                .find(|c| polyph_core::claims::targets_conflict(&c.target, &target))
                .map(|c| c.owner.label);
            let Some(m) = ctx.mirror.as_mut() else { return vec![] };
            match judge(m.files.get(&path), &base, holder.as_deref()) {
                Ok(()) => {
                    // Record first, so the host's own watcher sees no change.
                    if deleted {
                        m.files.remove(&path);
                    } else {
                        m.files.insert(path.clone(), hash(&data));
                    }
                    write_file(&m.root, &path, &data, deleted);
                    out.push(MirrorMsg::Changed { path, data, deleted, by: agent, proposal: Some(id) });
                }
                Err(reason) => out.push(MirrorMsg::Rejected { id, path, reason }),
            }
        }
        // Members only listen to the host.
        (Side::Member, _) if from != m.host => {}
        (Side::Member, MirrorMsg::Index { files, too_large }) => {
            let (local, _) = scan(&m.root);
            let want: HashMap<String, Hash> = files.into_iter().map(|e| (e.path, e.hash)).collect();
            for p in local.keys() {
                if !want.contains_key(p) {
                    write_file(&m.root, p, &[], true);
                }
            }
            let fetch: Vec<String> =
                want.iter().filter(|(p, h)| local.get(*p) != Some(*h)).map(|(p, _)| p.clone()).collect();
            m.files = want;
            m.too_large = too_large;
            if !fetch.is_empty() {
                out.push(MirrorMsg::Fetch { paths: fetch });
            }
        }
        (Side::Member, MirrorMsg::Files { files }) => {
            for f in files {
                let Some(p) = safe_path(&f.path) else { continue };
                if m.files.get(&p) == Some(&hash(&f.data)) {
                    write_file(&m.root, &p, &f.data, false);
                }
            }
        }
        (Side::Member, MirrorMsg::Changed { path, data, deleted, by, proposal }) => {
            let Some(p) = safe_path(&path) else { return vec![] };
            if deleted {
                m.files.remove(&p);
            } else {
                m.files.insert(p.clone(), hash(&data));
            }
            let ours = proposal.is_some_and(|id| m.pending.remove(&id).is_some());
            if m.inflight.get(&p) == m.files.get(&p) {
                m.inflight.remove(&p);
            }
            // Our own accepted change is on disk already, maybe with newer
            // edits on top; anyone else's is written over the mirror.
            if !ours && !m.inflight.contains_key(&p) {
                write_file(&m.root, &p, &data, deleted);
            }
            shared.emit(crate::events::ApiEvent::room_kind(
                &ctx.core.id,
                "mirror_changed",
                json!({ "path": p, "deleted": deleted, "by": by.map(|a| a.label) }),
            ));
        }
        (Side::Member, MirrorMsg::Rejected { id, path, reason }) => {
            let Some((p, agent)) = m.pending.remove(&id) else { return vec![] };
            m.inflight.remove(&p);
            // Back to the host's version.
            if m.files.contains_key(&p) {
                out.push(MirrorMsg::Fetch { paths: vec![p.clone()] });
            } else {
                write_file(&m.root, &p, &[], true);
            }
            let text = format!(
                "Your change to {path} did not reach the shared working copy: {reason}. Your mirror is back to the host's version."
            );
            if let Some(a) = &agent {
                ctx.coord.notice(a.id, text.clone());
            }
            shared.emit(crate::events::ApiEvent::room_kind(
                &ctx.core.id,
                "mirror_rejected",
                json!({ "path": p, "reason": reason, "agent": agent.map(|a| a.label) }),
            ));
        }
        _ => {}
    }
    out
}

/// A local change seen by a watcher. Returns what to send.
fn local_change(ctx: &mut RoomCtx, path: &str, agent: Option<AgentRef>) -> Option<MirrorMsg> {
    // Agents finish quickly: when none is mid-turn, credit the local agent
    // that holds the file, if one does.
    let agent = agent.or_else(|| {
        let me = ctx.core.me;
        let t = ClaimTarget { path: path.to_string(), symbol: None };
        ctx.core
            .claims
            .all()
            .into_iter()
            .find(|c| c.owner.device == me && polyph_core::claims::targets_conflict(&c.target, &t))
            .map(|c| c.owner)
    });
    let m = ctx.mirror.as_mut()?;
    let data = polyph_plugins::read_under(&m.root, Path::new(path), MAX_FILE as u64);
    let (data, deleted) = match data {
        Ok(d) => (d, false),
        Err(e) if e.kind() == std::io::ErrorKind::FileTooLarge => return None,
        // Gone, or now a link the mirror must not follow.
        Err(_) => (vec![], true),
    };
    let new = (!deleted).then(|| hash(&data));
    match m.side {
        Side::Host => {
            if m.files.get(path).copied() == new {
                return None;
            }
            match new {
                Some(h) => m.files.insert(path.to_string(), h),
                None => m.files.remove(path),
            };
            Some(MirrorMsg::Changed { path: path.to_string(), data, deleted, by: agent, proposal: None })
        }
        Side::Member => {
            let known = m.inflight.get(path).or(m.files.get(path)).copied();
            if known == new {
                return None;
            }
            let id = ObjectId::random().ok()?;
            m.pending.insert(id, (path.to_string(), agent.clone()));
            match new {
                Some(h) => m.inflight.insert(path.to_string(), h),
                None => m.inflight.remove(path),
            };
            Some(MirrorMsg::Propose {
                id,
                agent,
                path: path.to_string(),
                base: known.map(|h| h.to_vec()).unwrap_or_default(),
                data,
                deleted,
            })
        }
    }
}

/// The one local agent working in `root` right now, if exactly one is.
async fn working_agent(shared: &Shared, root: &Path) -> Option<AgentRef> {
    let d = shared.daemon.get()?.upgrade()?;
    let here: Vec<_> = d.agents.map.lock().await.values().filter(|h| h.worktree == root).cloned().collect();
    let mut working = vec![];
    for h in here {
        if matches!(*h.state.lock().await, AgentState::Running | AgentState::WaitingApproval) {
            working.push(h.agent.clone());
        }
    }
    (working.len() == 1).then(|| working.remove(0))
}

fn start_watcher(root: PathBuf, rt: Arc<RoomRuntime>, shared: Arc<Shared>) -> Option<JoinHandle<()>> {
    let (tx, mut rx) = mpsc::unbounded_channel::<PathBuf>();
    let mut w = notify::recommended_watcher(move |r: notify::Result<notify::Event>| {
        if let Ok(ev) = r
            && matches!(ev.kind, EventKind::Create(_) | EventKind::Modify(_) | EventKind::Remove(_))
        {
            for p in ev.paths {
                let _ = tx.send(p);
            }
        }
    })
    .ok()?;
    w.watch(&root, RecursiveMode::Recursive).ok()?;
    Some(tokio::spawn(async move {
        let _keep = w;
        while let Some(first) = rx.recv().await {
            let agent = working_agent(&shared, &root).await;
            let mut changed = vec![first];
            let deadline = tokio::time::Instant::now() + Duration::from_millis(200);
            while let Ok(Some(p)) = tokio::time::timeout_at(deadline, rx.recv()).await {
                changed.push(p);
            }
            let mut rel: Vec<String> = changed
                .iter()
                .filter(|p| !p.is_dir())
                .filter_map(|p| p.strip_prefix(&root).ok().map(|r| r.to_string_lossy().replace('\\', "/")))
                .filter_map(|r| safe_path(&r))
                .collect();
            rel.sort();
            rel.dedup();
            let mut ctx = rt.ctx.lock().await;
            let mut sends = vec![];
            for p in rel {
                if let Some(m) = local_change(&mut ctx, &p, agent.clone()) {
                    sends.push(m);
                }
            }
            for m in sends {
                crate::runtime::send(&mut ctx, &RoomMessage::Mirror(m)).await;
            }
        }
    }))
}

fn dir_param(dir: &Path) -> Result<PathBuf, ApiError> {
    std::fs::create_dir_all(dir)?;
    polyph_git::canonical(dir).map_err(|e| ApiError::BadRequest(format!("{}: {e}", dir.display())))
}

/// Makes this device the host of the room's shared working copy at `dir`.
pub async fn host(daemon: &Arc<Daemon>, room: &str, dir: &Path) -> Result<Value, ApiError> {
    let rt = daemon.find(room).await?;
    let root = dir_param(dir)?;
    let r2 = root.clone();
    let (files, too_large) =
        tokio::task::spawn_blocking(move || scan(&r2)).await.map_err(|e| ApiError::Failed(e.to_string()))?;
    let me = daemon.shared.identity.device_id();
    let count = files.len();
    let mut ctx = rt.ctx.lock().await;
    if !ctx.core.role().can_write() {
        return Err(ApiError::Forbidden("viewers cannot host the working copy".into()));
    }
    let watcher = start_watcher(root.clone(), rt.clone(), daemon.shared.clone());
    let m = Mirror {
        side: Side::Host,
        root: root.clone(),
        host: me,
        files,
        too_large: too_large.clone(),
        pending: HashMap::new(),
        inflight: HashMap::new(),
        watcher,
    };
    let idx = index(&m);
    ctx.mirror = Some(m);
    let fx = ctx.core.change_doc(DocName::Settings, |d| {
        docs::set_setting(d, SETTING, &json!({ "device": me.to_hex() }));
    })?;
    apply(&rt, &mut ctx, &daemon.shared, fx).await;
    crate::runtime::send(&mut ctx, &RoomMessage::Mirror(idx)).await;
    Ok(json!({ "root": root, "files": count, "too_large": too_large }))
}

/// Opens this member's mirror of the room's shared working copy at `dir`.
pub async fn open(daemon: &Arc<Daemon>, room: &str, dir: &Path) -> Result<Value, ApiError> {
    let rt = daemon.find(room).await?;
    let root = dir_param(dir)?;
    let mut ctx = rt.ctx.lock().await;
    let host = docs::setting(ctx.core.doc(&DocName::Settings), SETTING)
        .and_then(|v| v["device"].as_str().and_then(|h| DeviceId::from_hex(h).ok()))
        .ok_or_else(|| {
            ApiError::BadRequest(
                "this room has no shared working copy; its host runs `polyph host --mode hosted`".into(),
            )
        })?;
    if host == daemon.shared.identity.device_id() {
        return Err(ApiError::BadRequest("this device hosts the working copy".into()));
    }
    let watcher = start_watcher(root.clone(), rt.clone(), daemon.shared.clone());
    ctx.mirror = Some(Mirror {
        side: Side::Member,
        root: root.clone(),
        host,
        files: HashMap::new(),
        too_large: vec![],
        pending: HashMap::new(),
        inflight: HashMap::new(),
        watcher,
    });
    crate::runtime::send(&mut ctx, &RoomMessage::Mirror(MirrorMsg::IndexRequest)).await;
    Ok(json!({ "root": root, "host": host.to_hex() }))
}

pub async fn status(rt: &RoomRuntime) -> Value {
    let ctx = rt.ctx.lock().await;
    match &ctx.mirror {
        None => json!({ "mirror": null }),
        Some(m) => json!({
            "side": m.side,
            "root": m.root,
            "host": m.host.to_hex(),
            "files": m.files.len(),
            "too_large": m.too_large,
            "pending": m.pending.len(),
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paths_and_judgement() {
        assert_eq!(safe_path("src/a.rs").as_deref(), Some("src/a.rs"));
        for bad in ["", "/etc/passwd", "../x", "src/../../x", ".git/config", "a/./b"] {
            assert!(safe_path(bad).is_none(), "{bad}");
        }
        let h = hash(b"v1");
        assert!(judge(Some(&h), &h, None).is_ok());
        assert!(judge(None, &[], None).is_ok());
        assert!(judge(Some(&h), &hash(b"v0"), None).unwrap_err().contains("changed on the host first"));
        assert!(judge(Some(&h), &[], None).is_err(), "a new file that already exists");
        assert!(judge(None, &h, None).unwrap_err().contains("no longer exists"));
        assert_eq!(judge(Some(&h), &h, Some("Ann/claude")).unwrap_err(), "Ann/claude holds this file");
    }
}
