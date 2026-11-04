//! Claims, layer 3: a file watcher on each agent's worktree catches edits
//! made outside the agent protocol (an agent running `sed -i` or a code
//! generator in its terminal). A changed file nobody holds is claimed for the
//! agent; a file someone else holds produces a warning for the agent and the
//! room. Edits while the agent is not working are a person's, and are left
//! alone.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use notify::{EventKind, RecursiveMode, Watcher};
use polyph_core::claims::targets_conflict;
use polyph_proto::crdt::{Activity, ActivityEntry};
use polyph_proto::ids::ObjectId;
use polyph_proto::messages::{AgentState, ClaimTarget, DocName};
use serde_json::json;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tracing::debug;

use crate::agents::AgentHandle;
use crate::daemon::Shared;
use crate::runtime::{RoomRuntime, apply};

/// Changes are gathered for this long before they are looked at.
const SETTLE: Duration = Duration::from_millis(300);
/// The same overlap is reported at most once in this many seconds.
const REWARN_SECS: i64 = 120;
const SKIP_DIRS: &[&str] = &[".git", ".polyph", "target", "node_modules", ".venv", "__pycache__", ".next", "dist"];

/// Whether a relative path is worth watching or mirroring.
pub fn relevant(rel: &str) -> bool {
    !rel.is_empty()
        && !rel.split('/').any(|seg| SKIP_DIRS.contains(&seg))
        && !rel.ends_with('~')
        && !rel.ends_with(".swp")
}

/// Starts watching `h`'s worktree. The task ends when the handle is dropped.
pub fn spawn(h: Arc<AgentHandle>, rt: Arc<RoomRuntime>, shared: Arc<Shared>) -> Option<JoinHandle<()>> {
    let (tx, rx) = mpsc::unbounded_channel::<PathBuf>();
    let mut watcher = notify::recommended_watcher(move |r: notify::Result<notify::Event>| {
        if let Ok(ev) = r
            && matches!(ev.kind, EventKind::Create(_) | EventKind::Modify(_) | EventKind::Remove(_))
        {
            for p in ev.paths {
                let _ = tx.send(p);
            }
        }
    })
    .ok()?;
    watcher.watch(&h.worktree, RecursiveMode::Recursive).ok()?;
    // The handle is weak so the watcher never keeps a stopped agent alive.
    let weak = Arc::downgrade(&h);
    let root = h.worktree.clone();
    drop(h);
    Some(tokio::spawn(async move {
        let _keep = watcher;
        run(rx, weak, root, rt, shared).await;
    }))
}

async fn run(
    mut rx: mpsc::UnboundedReceiver<PathBuf>,
    h: std::sync::Weak<AgentHandle>,
    root: PathBuf,
    rt: Arc<RoomRuntime>,
    shared: Arc<Shared>,
) {
    let mut warned: HashMap<String, i64> = HashMap::new();
    while let Some(first) = rx.recv().await {
        let Some(h) = h.upgrade() else { return };
        // Only edits made while the agent works are its own; others are a
        // person's. Decided when the change happens, not after settling.
        let working = matches!(*h.state.lock().await, AgentState::Running | AgentState::WaitingApproval);
        let mut changed: HashSet<PathBuf> = HashSet::from([first]);
        let deadline = tokio::time::Instant::now() + SETTLE;
        while let Ok(Some(p)) = tokio::time::timeout_at(deadline, rx.recv()).await {
            changed.insert(p);
        }
        if !working {
            continue;
        }
        if another_agent_running_here(&shared, &h).await {
            debug!(agent = %h.agent.label, "watcher: several agents running in one directory; not attributing");
            continue;
        }
        let rel = relative_paths(&root, changed).await;
        for path in rel {
            check(&h, &rt, &shared, &path, &mut warned).await;
        }
    }
}

async fn relative_paths(root: &Path, changed: HashSet<PathBuf>) -> Vec<String> {
    let mut rel: Vec<String> = changed
        .into_iter()
        .filter(|p| !p.is_dir())
        .filter_map(|p| p.strip_prefix(root).ok().map(|r| r.to_string_lossy().replace('\\', "/")))
        .filter(|r| relevant(r))
        .collect();
    rel.sort();
    rel.dedup();
    let root = root.to_path_buf();
    let rel2 = rel.clone();
    let ignored = tokio::task::spawn_blocking(move || {
        polyph_git::Repo::open(&root).and_then(|r| r.ignored(&rel2)).unwrap_or_default()
    })
    .await
    .unwrap_or_default();
    rel.retain(|r| !ignored.contains(r));
    rel
}

async fn another_agent_running_here(shared: &Shared, h: &AgentHandle) -> bool {
    let Some(d) = shared.daemon.get().and_then(|w| w.upgrade()) else { return false };
    let others: Vec<Arc<AgentHandle>> = d
        .agents
        .map
        .lock()
        .await
        .values()
        .filter(|o| o.agent.id != h.agent.id && o.worktree == h.worktree)
        .cloned()
        .collect();
    for o in others {
        if matches!(*o.state.lock().await, AgentState::Running | AgentState::WaitingApproval) {
            return true;
        }
    }
    false
}

async fn check(h: &AgentHandle, rt: &RoomRuntime, shared: &Shared, path: &str, warned: &mut HashMap<String, i64>) {
    // Written through the file tools: claims were settled then.
    let via_protocol =
        h.acp_writes.lock().get(&h.worktree.join(path)).is_some_and(|t| t.elapsed() < Duration::from_secs(30));
    if via_protocol {
        return;
    }
    let target = ClaimTarget { path: path.to_string(), symbol: None };
    let now = shared.clock.now();
    let mut ctx = rt.ctx.lock().await;
    let all = ctx.core.claims.all();
    // Already covered by one of this agent's claims (file, directory or symbol).
    if all.iter().any(|c| c.owner.id == h.agent.id && c.expires_at > now && targets_conflict(&c.target, &target)) {
        return;
    }
    match ctx.core.claims.holder(&target, Some(&h.agent), now).cloned() {
        None => {
            if !h.project.auto_claim() {
                return;
            }
            // Claim it now; the arbiter's answer arrives as an event.
            if let Ok((_, fx)) = ctx.core.request_claims(
                &h.agent,
                &[target],
                "edited outside Polyph's file tools",
                h.task,
                polyph_core::room::DEFAULT_LEASE,
                now,
            ) {
                apply(rt, &mut ctx, shared, fx).await;
            }
        }
        Some(holder) => {
            if warned.get(path).is_some_and(|t| now - t < REWARN_SECS) {
                return;
            }
            warned.insert(path.to_string(), now);
            ctx.coord.notice(
                h.agent.id,
                format!(
                    "You changed {path} outside Polyph's file tools, but {} holds {} ({}). Undo that change, or ask them with ask_agent before going on.",
                    holder.owner.label, holder.target, holder.reason
                ),
            );
            let entry = ActivityEntry {
                id: ObjectId::random().map(|o| o.to_hex()).unwrap_or_default(),
                at: now,
                kind: "overlap".into(),
                actor: h.agent.member.to_hex(),
                agent: Some(h.agent.label.clone()),
                summary_key: "activity-overlap".into(),
                args: json!({ "path": path, "agent": h.agent.label, "holder": holder.owner.label, "claim": holder.target.to_string() }),
                checkpoint: None,
            };
            if let Ok(fx) = ctx.core.change_doc(DocName::Activity, |d| {
                let _ = Activity::new(d).append(&entry);
            }) {
                apply(rt, &mut ctx, shared, fx).await;
            }
            shared.emit(crate::events::ApiEvent::room_kind(
                &rt.id,
                "claim_overlap",
                json!({ "path": path, "agent": h.agent.label, "holder": holder.owner.label }),
            ));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::relevant;

    #[test]
    fn skips_build_output_and_editor_files() {
        assert!(relevant("src/a.rs"));
        assert!(!relevant("target/debug/x"));
        assert!(!relevant("web/node_modules/a/index.js"));
        assert!(!relevant(".git/index"));
        assert!(!relevant("src/.a.rs.swp"));
    }
}
