//! Tasks on this device: a worktree and branch per agent task, finishing a
//! task (commit and queue), and the merge queue: review, approval, merge with
//! intent-aware conflict resolution, checks, and the target branch moving on.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, LazyLock};
use std::time::Duration;

use polyph_agents::project::ProjectConfig;
use polyph_core::docs::{self, MergeEntry, MergeState};
use polyph_git::{MergeOutcome, Repo, Signature};
use polyph_merge::{Intent, Strategy};
use polyph_proto::crdt::{Board, CardView};
use polyph_proto::ids::ObjectId;
use polyph_proto::messages::{AgentEventKind, AgentRef, ApprovalKind, Decision, DocName};
use polyph_sandbox::Policy;
use polyph_sandbox::proxy::{Allowlist, Decider};
use serde::Serialize;
use serde_json::{Value, json};
use tracing::warn;
use yrs::Any;

use crate::agents::{self, AgentHandle, StartParams};
use crate::daemon::{ApiError, Daemon, Shared};
use crate::runtime::{RoomRuntime, apply};

/// Where task worktrees live, relative to the main working copy.
pub const WORKTREES: &str = ".polyph/worktrees";
/// Longest diff given to a reviewer, in bytes.
const REVIEW_DIFF_MAX: usize = 60_000;
const CHECK_TIMEOUT: Duration = Duration::from_secs(15 * 60);

#[derive(Debug, Clone, Serialize)]
pub struct TaskWorktree {
    /// The main working copy.
    pub repo: PathBuf,
    pub path: PathBuf,
    pub branch: String,
    /// Branch the task started from.
    pub base: String,
}

fn git_err(e: polyph_git::GitError) -> ApiError {
    ApiError::Failed(e.to_string())
}

async fn blocking<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> Result<T, ApiError> {
    tokio::task::spawn_blocking(f).await.map_err(|e| ApiError::Failed(e.to_string()))
}

/// Creates (or reuses) the worktree for a task in the repository at `cwd`.
pub async fn prepare_worktree(cwd: PathBuf, branch: String, base: Option<String>) -> Result<TaskWorktree, ApiError> {
    blocking(move || -> Result<TaskWorktree, ApiError> {
        let repo = Repo::open(&cwd).map_err(git_err)?;
        let main = Repo::open(&repo.main_root().map_err(git_err)?).map_err(git_err)?;
        let base = match base {
            Some(b) => b,
            None => repo.current_branch().ok_or_else(|| {
                ApiError::BadRequest(
                    "the repository is on a detached HEAD; check out the branch tasks start from".into(),
                )
            })?,
        };
        let path = main.root().join(WORKTREES).join(branch.replace('/', "-"));
        if path.exists()
            && let Ok(wt) = main.open_worktree(&path)
            && wt.current_branch().as_deref() == Some(branch.as_str())
        {
            return Ok(TaskWorktree { repo: main.root().to_path_buf(), path, branch, base });
        }
        main.exclude("/.polyph/").map_err(git_err)?;
        std::fs::create_dir_all(main.root().join(WORKTREES))?;
        main.add_worktree(&path, &branch, &base).map_err(git_err)?;
        Ok(TaskWorktree { repo: main.root().to_path_buf(), path, branch, base })
    })
    .await?
}

pub fn card(ctx: &mut crate::runtime::RoomCtx, id: &str) -> Option<CardView> {
    Board::new(ctx.core.doc(&DocName::Board)).card(id)
}

/// Updates a card if it exists: moves it and sets string fields.
pub async fn update_card(
    rt: &RoomRuntime,
    shared: &Shared,
    id: &str,
    column: Option<&str>,
    fields: &[(&str, Option<String>)],
) {
    let now = shared.clock.now();
    let mut ctx = rt.ctx.lock().await;
    if card(&mut ctx, id).is_none() {
        return;
    }
    let fx = ctx.core.change_doc(DocName::Board, |d| {
        let b = Board::new(d);
        if let Some(c) = column {
            let last = b.cards_in(c).last().map(|x| x.id.clone());
            let _ = b.move_card(id, c, last.as_deref(), now);
        }
        for (k, v) in fields {
            let val = match v {
                Some(s) => Any::String(s.as_str().into()),
                None => Any::Null,
            };
            let _ = b.set_field(id, k, val, now);
        }
    });
    if let Ok(fx) = fx {
        apply(rt, &mut ctx, shared, fx).await;
    }
    if let Some(c) = column {
        crate::linear::spawn_card_moved(shared, rt.id, id.to_string(), c.to_string());
    }
}

pub(crate) async fn put_entry(rt: &RoomRuntime, shared: &Shared, e: &MergeEntry) {
    let mut ctx = rt.ctx.lock().await;
    if let Ok(fx) = ctx.core.change_doc(DocName::Board, |d| docs::put_merge_entry(d, e)) {
        apply(rt, &mut ctx, shared, fx).await;
    }
}

pub(crate) async fn set_state(
    rt: &RoomRuntime,
    shared: &Shared,
    e: &mut MergeEntry,
    state: MergeState,
    detail: Option<String>,
) {
    e.state = state;
    if detail.is_some() {
        e.detail = detail.map(|d| polyph_core::redact::redact(&d).into_owned());
    }
    e.updated_at = shared.clock.now();
    put_entry(rt, shared, e).await;
    shared.emit(crate::events::ApiEvent::room_kind(&rt.id, "merge_queue", json!({ "entry": e })));
    if matches!(state, MergeState::Merged | MergeState::Failed | MergeState::ChangesRequested) {
        let kind = match state {
            MergeState::Merged => "merged",
            MergeState::Failed => "merge_failed",
            _ => "changes_requested",
        };
        crate::feed::log(
            rt,
            shared,
            kind,
            e.agent.as_deref(),
            json!({ "branch": e.branch, "into": e.into, "card": e.card, "resolved": e.resolved, "commit": e.merge_commit }),
        )
        .await;
    }
}

/// Commits the agent's work and queues it for merging.
pub async fn finish(daemon: Arc<Daemon>, h: Arc<AgentHandle>, message: Option<String>) -> Result<Value, ApiError> {
    let wt = h.task_wt.clone().ok_or_else(|| {
        ApiError::BadRequest("this agent has no task worktree; start it with a task in a git repository".into())
    })?;
    let rt = daemon.find(&h.room.to_hex()).await?;
    let card = match h.task {
        Some(t) => card(&mut *rt.ctx.lock().await, &t.to_hex()),
        None => None,
    };
    let msg = message
        .filter(|m| !m.trim().is_empty())
        .or_else(|| card.as_ref().map(|c| c.title.clone()))
        .unwrap_or_else(|| format!("Work by {}", h.agent.label));
    let msg = polyph_core::redact::redact(&msg).into_owned();
    let summary = msg.clone();
    let sig = Signature::agent(&h.agent.label, &h.agent.id.short());
    let (path, branch, base) = (wt.path.clone(), wt.branch.clone(), wt.base.clone());
    let project = ProjectConfig::load(&wt.repo).unwrap_or_default();
    let into = project.merge.into.clone().unwrap_or(base);
    let (head, ahead) = {
        let (into, branch, repo) = (into.clone(), branch.clone(), wt.repo.clone());
        blocking(move || -> Result<(String, usize), ApiError> {
            let r = Repo::open(&repo).and_then(|m| m.open_worktree(&path)).map_err(git_err)?;
            r.commit_all(&msg, &sig).map_err(git_err)?;
            let head = r.rev_parse(&branch).map_err(git_err)?;
            let ahead = r.log_subjects(&into, &head).map_err(git_err)?.len();
            Ok((head, ahead))
        })
        .await??
    };
    if ahead == 0 {
        return Err(ApiError::BadRequest(format!("{branch} has nothing that is not already in {into}")));
    }
    let now = daemon.shared.clock.now();
    let mut entry = MergeEntry {
        id: ObjectId::random().map_err(|e| ApiError::Failed(e.to_string()))?.to_hex(),
        card: h.task.map(|t| t.to_hex()),
        branch: branch.clone(),
        into,
        head,
        device: daemon.shared.identity.device_id().to_hex(),
        member: h.agent.member.to_hex(),
        agent: Some(h.agent.label.clone()),
        state: MergeState::Queued,
        detail: None,
        resolved: vec![],
        merge_commit: None,
        pr: None,
        pr_number: None,
        pr_repo: None,
        queued_at: now,
        updated_at: now,
    };
    put_entry(&rt, &daemon.shared, &entry).await;
    if let Some(c) = &entry.card {
        // The commit message is the agent's summary: keep it for late joiners.
        update_card(&rt, &daemon.shared, c, Some("review"), &[(polyph_proto::crdt::card::SUMMARY, Some(summary))])
            .await;
    }
    // The work is committed: its files are free for others, and any overlap
    // from here on is the merge queue's to settle.
    {
        let mut ctx = rt.ctx.lock().await;
        let mine: Vec<ObjectId> =
            ctx.core.claims.all().into_iter().filter(|c| c.owner.id == h.agent.id).map(|c| c.id).collect();
        let fx = ctx.core.release_claims(&mine);
        apply(&rt, &mut ctx, &daemon.shared, fx).await;
    }
    let out = json!({ "entry": entry.id, "branch": branch, "into": entry.into, "commits": ahead });
    let agent = h.agent.clone();
    let repo = wt.repo.clone();
    let via_github = project.merge.via.as_deref() == Some("github");
    tokio::spawn(async move {
        let lock = repo_lock(&repo);
        let _g = lock.lock().await;
        let r = if via_github {
            crate::github::open_pr(&daemon, &rt, &mut entry, &repo, &agent).await
        } else {
            process(&daemon, &rt, &mut entry, &repo, &agent).await
        };
        if let Err(e) = r {
            set_state(&rt, &daemon.shared, &mut entry, MergeState::Failed, Some(e.to_string())).await;
        }
    });
    Ok(out)
}

/// One merge at a time per repository, in queue order (the lock is fair).
fn repo_lock(repo: &Path) -> Arc<tokio::sync::Mutex<()>> {
    static LOCKS: LazyLock<parking_lot::Mutex<HashMap<PathBuf, Arc<tokio::sync::Mutex<()>>>>> =
        LazyLock::new(Default::default);
    LOCKS.lock().entry(repo.to_path_buf()).or_default().clone()
}

pub(crate) fn intent_of(c: Option<&CardView>, label: &str) -> Intent {
    match c {
        Some(c) => Intent {
            label: label.to_string(),
            title: c.title.clone(),
            description: c.description.clone(),
            acceptance: c.acceptance.clone(),
        },
        None => Intent { label: label.to_string(), ..Default::default() },
    }
}

async fn process(
    daemon: &Arc<Daemon>,
    rt: &Arc<RoomRuntime>,
    e: &mut MergeEntry,
    repo_root: &Path,
    agent: &AgentRef,
) -> Result<(), ApiError> {
    let shared = daemon.shared.clone();
    // The target branch's own polyph.toml, which people control.
    let project = ProjectConfig::load(repo_root).unwrap_or_default();
    let card = match &e.card {
        Some(c) => card(&mut *rt.ctx.lock().await, c),
        None => None,
    };
    let theirs_intent = intent_of(card.as_ref(), &e.branch);

    // 1. Review.
    if let Some(kind) = project.merge.reviewer.clone() {
        set_state(rt, &shared, e, MergeState::Reviewing, None).await;
        let (approve, notes) = review(daemon, rt, repo_root, e, &kind, &theirs_intent).await?;
        if !approve {
            set_state(rt, &shared, e, MergeState::ChangesRequested, Some(notes.clone())).await;
            if let Some(c) = &e.card {
                update_card(rt, &shared, c, Some("in_progress"), &[]).await;
            }
            // The task's agent hears about it at its next turn.
            let mut ctx = rt.ctx.lock().await;
            let now = shared.clock.now();
            if let Ok((_, fx)) = ctx.core.send_inbox(
                agent.clone(),
                None,
                None,
                None,
                &format!("The reviewer asked for changes before {} can merge:\n{notes}", e.branch),
                0,
                now,
            ) {
                apply(rt, &mut ctx, &shared, fx).await;
            }
            return Ok(());
        }
        e.detail = Some(notes);
    }

    // 2. A person approves the merge.
    if project.merge.approval.unwrap_or(true) {
        set_state(rt, &shared, e, MergeState::AwaitingApproval, None).await;
        let kind = ApprovalKind::Merge { branch: e.branch.clone(), into: e.into.clone() };
        let task = e.card.as_deref().and_then(|c| ObjectId::from_hex(c).ok());
        if let Decision::Deny { note } = agents::request_approval(rt, &shared, agent, kind, task).await {
            let why = note.map(|n| format!(": {n}")).unwrap_or_default();
            set_state(rt, &shared, e, MergeState::Failed, Some(format!("the merge was not approved{why}"))).await;
            return Ok(());
        }
    }

    // 3. Merge, resolve, check, move the branch. Retried if the branch moves.
    let who = Signature { name: "Polyph merge queue".into(), email: "merge-queue@polyph.invalid".into() };
    let message = format!(
        "Merge {} into {}{}\n\nQueued by {}.",
        e.branch,
        e.into,
        card.as_ref().map(|c| format!(": {}", c.title)).unwrap_or_default(),
        e.agent.clone().unwrap_or_default()
    );
    for _attempt in 0..3 {
        set_state(rt, &shared, e, MergeState::Merging, None).await;
        let (into, head, root) = (e.into.clone(), e.head.clone(), repo_root.to_path_buf());
        let (msg, who2) = (message.clone(), who.clone());
        let (old, outcome, how) = blocking(move || -> Result<_, ApiError> {
            let r = Repo::open(&root).map_err(git_err)?;
            let old = r.rev_parse(&into).map_err(git_err)?;
            let mut how: Vec<(String, Strategy)> = vec![];
            let out = r
                .merge(&old, &head, &msg, &who2, &mut |c| {
                    let (text, s) = polyph_merge::resolve(c)?;
                    how.push((c.path.clone(), s));
                    Some(text)
                })
                .map_err(git_err)?;
            Ok((old, out, how))
        })
        .await??;
        let mut resolved: Vec<(String, String)> =
            how.iter().map(|(p, s)| (p.clone(), format!("{s:?}").to_lowercase())).collect();
        let commit = match outcome {
            MergeOutcome::UpToDate => {
                e.merge_commit = Some(old);
                set_state(rt, &shared, e, MergeState::Merged, Some(format!("already in {}", e.into))).await;
                if let Some(c) = &e.card {
                    update_card(rt, &shared, c, Some("done"), &[]).await;
                }
                return Ok(());
            }
            MergeOutcome::Merged { commit, .. } => commit,
            MergeOutcome::Conflicts(files) => {
                let Some(kind) = project.merge.resolver.clone() else {
                    set_state(rt, &shared, e, MergeState::Failed, Some(format!(
                        "conflicts Polyph could not merge by itself in {}; set [merge] resolver in polyph.toml to have an agent resolve them, or merge by hand",
                        files.join(", ")
                    )))
                    .await;
                    return Ok(());
                };
                set_state(
                    rt,
                    &shared,
                    e,
                    MergeState::Resolving,
                    Some(format!("asking {kind} to resolve {}", files.join(", "))),
                )
                .await;
                let ours_intent = ours_intent(rt, repo_root, &e.into, &files).await;
                let answers =
                    resolve_with_agent(daemon, rt, repo_root, e, &old, &files, &kind, &ours_intent, &theirs_intent)
                        .await?;
                let (head, root, msg, who2) = (e.head.clone(), repo_root.to_path_buf(), message.clone(), who.clone());
                let old2 = old.clone();
                let answers2 = answers.clone();
                let out = blocking(move || -> Result<_, ApiError> {
                    let r = Repo::open(&root).map_err(git_err)?;
                    r.merge(&old2, &head, &msg, &who2, &mut |c| match answers2.get(&c.path) {
                        Some(t) => Some(t.clone()),
                        None => polyph_merge::resolve(c).map(|x| x.0),
                    })
                    .map_err(git_err)
                })
                .await??;
                match out {
                    MergeOutcome::Merged { commit, .. } => {
                        for f in answers.keys() {
                            resolved.retain(|(p, _)| p != f);
                            resolved.push((f.clone(), format!("agent:{kind}")));
                        }
                        commit
                    }
                    other => {
                        set_state(
                            rt,
                            &shared,
                            e,
                            MergeState::Failed,
                            Some(format!("the resolver did not settle the conflicts: {other:?}")),
                        )
                        .await;
                        return Ok(());
                    }
                }
            }
        };
        e.resolved = resolved;
        // Checks on the merged result, in the sandbox.
        if project.merge.run_checks.unwrap_or(true) && !project.checks.is_empty() {
            set_state(rt, &shared, e, MergeState::Checking, None).await;
            if let Err(why) = run_checks(&shared, &project, repo_root, &commit, &e.id).await {
                set_state(rt, &shared, e, MergeState::Failed, Some(why)).await;
                if let Some(c) = &e.card {
                    update_card(rt, &shared, c, Some("in_progress"), &[]).await;
                }
                return Ok(());
            }
        }
        let (root, into, c2, o2) = (repo_root.to_path_buf(), e.into.clone(), commit.clone(), old.clone());
        let moved = blocking(move || Repo::open(&root).and_then(|r| r.advance_branch(&into, &c2, &o2))).await?;
        match moved {
            Ok(()) => {
                e.merge_commit = Some(commit);
                let summary = if e.resolved.is_empty() {
                    None
                } else {
                    Some(format!(
                        "merged; conflicts resolved in {}",
                        e.resolved.iter().map(|(p, s)| format!("{p} ({s})")).collect::<Vec<_>>().join(", ")
                    ))
                };
                set_state(rt, &shared, e, MergeState::Merged, summary).await;
                if let Some(c) = &e.card {
                    update_card(rt, &shared, c, Some("done"), &[]).await;
                    crate::plan::promote_unblocked(rt, &shared).await;
                }
                if let Some(remote) = project.merge.push.clone() {
                    push(rt, &shared, agent, repo_root, &remote, &e.into).await;
                }
                return Ok(());
            }
            Err(polyph_git::GitError::Invalid(m)) if m.contains("moved") => continue,
            Err(err) => {
                set_state(rt, &shared, e, MergeState::Failed, Some(err.to_string())).await;
                return Ok(());
            }
        }
    }
    set_state(rt, &shared, e, MergeState::Failed, Some(format!("{} kept moving; queue it again", e.into))).await;
    Ok(())
}

async fn push(rt: &RoomRuntime, shared: &Shared, agent: &AgentRef, root: &Path, remote: &str, branch: &str) {
    let kind = ApprovalKind::Push { remote: remote.to_string(), branch: branch.to_string() };
    if let Decision::Approve { .. } = agents::request_approval(rt, shared, agent, kind, None).await {
        let (root, remote2, branch2) = (root.to_path_buf(), remote.to_string(), branch.to_string());
        let home = shared.paths.home.clone();
        let r =
            blocking(move || crate::folders::open_for_network(&home, &root).and_then(|r| r.push(&remote2, &branch2)))
                .await;
        let ok = matches!(r, Ok(Ok(())));
        shared.emit(crate::events::ApiEvent::room_kind(
            &rt.id,
            "pushed",
            json!({ "remote": remote, "branch": branch, "ok": ok, "error": r.ok().and_then(|x| x.err()).map(|e| e.to_string()) }),
        ));
    }
}

/// The intent of the task most recently merged into `into` that touched `files`.
async fn ours_intent(rt: &RoomRuntime, root: &Path, into: &str, files: &[String]) -> Intent {
    let entries = {
        let mut ctx = rt.ctx.lock().await;
        docs::merge_entries(ctx.core.doc(&DocName::Board))
    };
    for m in entries.iter().rev().filter(|m| m.state == MergeState::Merged && m.into == into) {
        let Some(commit) = m.merge_commit.clone() else { continue };
        let root = root.to_path_buf();
        let touched = blocking(move || Repo::open(&root).and_then(|r| r.merged_files(&commit)).unwrap_or_default())
            .await
            .unwrap_or_default();
        if touched.iter().any(|f| files.contains(f)) {
            let c = match &m.card {
                Some(id) => card(&mut *rt.ctx.lock().await, id),
                None => None,
            };
            return intent_of(c.as_ref(), &m.branch);
        }
    }
    Intent { label: into.to_string(), title: format!("the current {into}"), ..Default::default() }
}

/// A Polyph-run agent (planner, reviewer, resolver) in a scratch directory.
pub(crate) struct Helper {
    pub h: Arc<AgentHandle>,
}

impl Helper {
    pub async fn start(
        daemon: &Arc<Daemon>,
        rt: &Arc<RoomRuntime>,
        kind: &str,
        cwd: &Path,
    ) -> Result<Helper, ApiError> {
        let started = agents::start(
            daemon.shared.clone(),
            &daemon.agents,
            rt.clone(),
            StartParams {
                room: rt.id.to_hex(),
                kind: kind.to_string(),
                cwd: cwd.to_path_buf(),
                preset: Some("default".into()),
                task: None,
                prompt: None,
                worktree: Some(false),
                helper: true,
            },
        )
        .await?;
        let h = daemon.agent(started["id"].as_str().unwrap_or_default()).await?;
        Ok(Helper { h })
    }

    /// One turn; returns what the agent said in it.
    pub async fn turn(&self, daemon: &Arc<Daemon>, rt: &Arc<RoomRuntime>, prompt: &str) -> Result<String, ApiError> {
        let seen = rt.ctx.lock().await.transcripts.get(&self.h.agent.id).map(|l| l.len()).unwrap_or(0);
        agents::prompt(&daemon.shared, rt, &self.h, prompt).await?;
        let ctx = rt.ctx.lock().await;
        Ok(ctx
            .transcripts
            .get(&self.h.agent.id)
            .map(|log| {
                log.iter()
                    .skip(seen)
                    .filter_map(|ev| match &ev.kind {
                        AgentEventKind::Message { text } => Some(text.clone()),
                        _ => None,
                    })
                    .collect::<String>()
            })
            .unwrap_or_default())
    }

    pub async fn stop(self, daemon: &Arc<Daemon>, rt: &Arc<RoomRuntime>) {
        let _ = agents::stop(&daemon.shared, &daemon.agents, rt, self.h.agent.id).await;
    }
}

/// Runs a helper agent in `cwd` for one turn and returns what it said.
async fn helper_turn(
    daemon: &Arc<Daemon>,
    rt: &Arc<RoomRuntime>,
    kind: &str,
    cwd: &Path,
    prompt: &str,
) -> Result<String, ApiError> {
    let helper = Helper::start(daemon, rt, kind, cwd).await?;
    let r = helper.turn(daemon, rt, prompt).await;
    helper.stop(daemon, rt).await;
    r
}

pub(crate) async fn temp_worktree(root: &Path, name: &str, rev: &str) -> Result<PathBuf, ApiError> {
    let (root, rev) = (root.to_path_buf(), rev.to_string());
    let path = root.join(WORKTREES).join(name);
    let p2 = path.clone();
    blocking(move || -> Result<(), ApiError> {
        let r = Repo::open(&root).map_err(git_err)?;
        r.exclude("/.polyph/").map_err(git_err)?;
        if p2.exists() {
            let _ = r.remove_worktree(&p2);
            let _ = std::fs::remove_dir_all(&p2);
        }
        std::fs::create_dir_all(root.join(WORKTREES))?;
        r.add_detached(&p2, &rev).map_err(git_err)?;
        Ok(())
    })
    .await??;
    Ok(path)
}

pub(crate) async fn drop_worktree(root: &Path, path: &Path) {
    let (root, path) = (root.to_path_buf(), path.to_path_buf());
    let _ = blocking(move || {
        if let Ok(r) = Repo::open(&root) {
            let _ = r.remove_worktree(&path);
        }
        let _ = std::fs::remove_dir_all(&path);
    })
    .await;
}

/// Asks the reviewer agent; returns (approved, its notes).
async fn review(
    daemon: &Arc<Daemon>,
    rt: &Arc<RoomRuntime>,
    root: &Path,
    e: &MergeEntry,
    kind: &str,
    intent: &Intent,
) -> Result<(bool, String), ApiError> {
    let (r2, into, head) = (root.to_path_buf(), e.into.clone(), e.head.clone());
    let diff = blocking(move || Repo::open(&r2).and_then(|r| r.diff(&into, &head))).await?.map_err(git_err)?;
    let mut diff = polyph_core::redact::redact(&diff).into_owned();
    if diff.len() > REVIEW_DIFF_MAX {
        let mut cut = REVIEW_DIFF_MAX;
        while !diff.is_char_boundary(cut) {
            cut -= 1;
        }
        diff.truncate(cut);
        diff.push_str("\n[diff cut off; read the files for the rest]\n");
    }
    let mut task = format!("Task: {}", if intent.title.is_empty() { &e.branch } else { &intent.title });
    if !intent.description.is_empty() {
        task.push_str(&format!("\n{}", intent.description));
    }
    for a in &intent.acceptance {
        task.push_str(&format!("\n- done when: {a}"));
    }
    let prompt = format!(
        "Review this change before it merges into {}. The working directory has the branch checked out.\n\n{task}\n\n\
Check that it does what the task asks, that nothing obviously breaks, and that it adds no secrets. \
Do not edit files. End your answer with exactly one line: `VERDICT: APPROVE` or `VERDICT: CHANGES`, \
and when asking for changes, list them above that line.\n\n```diff\n{diff}```",
        e.into
    );
    let wt = temp_worktree(root, &format!("review-{}", &e.id[..8]), &e.head).await?;
    let said = helper_turn(daemon, rt, kind, &wt, &prompt).await;
    drop_worktree(root, &wt).await;
    let said = said?;
    let verdict = said.lines().rev().find_map(|l| {
        let l = l.trim().trim_matches('`').trim().to_ascii_uppercase();
        l.strip_prefix("VERDICT:").map(|v| v.trim().to_string())
    });
    let notes: String = said
        .lines()
        .filter(|l| !l.trim().trim_matches('`').to_ascii_uppercase().starts_with("VERDICT:"))
        .collect::<Vec<_>>()
        .join("\n");
    match verdict.as_deref() {
        Some("APPROVE") => Ok((true, notes.trim().to_string())),
        Some(_) => Ok((false, notes.trim().to_string())),
        None => Ok((false, format!("the reviewer gave no verdict. It said: {}", notes.trim()))),
    }
}

/// Writes the conflicted files with markers into a worktree at `ours` and
/// lets the resolver agent edit them. Returns the resolved contents.
#[allow(clippy::too_many_arguments)]
async fn resolve_with_agent(
    daemon: &Arc<Daemon>,
    rt: &Arc<RoomRuntime>,
    root: &Path,
    e: &MergeEntry,
    ours: &str,
    files: &[String],
    kind: &str,
    ours_intent: &Intent,
    theirs_intent: &Intent,
) -> Result<HashMap<String, String>, ApiError> {
    let wt = temp_worktree(root, &format!("resolve-{}", &e.id[..8]), ours).await?;
    let (r2, o2, h2, f2, wt2) = (root.to_path_buf(), ours.to_string(), e.head.clone(), files.to_vec(), wt.clone());
    let (ol, tl) = (ours_intent.label.clone(), theirs_intent.label.clone());
    blocking(move || -> Result<(), ApiError> {
        let r = Repo::open(&r2).map_err(git_err)?;
        let base = r.merge_base(&o2, &h2).map_err(git_err)?;
        for f in &f2 {
            let c = polyph_git::Conflict {
                path: f.clone(),
                base: base.as_deref().and_then(|b| r.show(b, f)),
                ours: r.show(&o2, f),
                theirs: r.show(&h2, f),
            };
            let p = wt2.join(f);
            if let Some(d) = p.parent() {
                std::fs::create_dir_all(d)?;
            }
            std::fs::write(&p, polyph_merge::with_markers(&c, &ol, &tl))?;
        }
        Ok(())
    })
    .await??;
    let prompt = polyph_merge::resolver_prompt(files, ours_intent, theirs_intent);
    let r = helper_turn(daemon, rt, kind, &wt, &prompt).await;
    let mut out = HashMap::new();
    let mut bad = vec![];
    for f in files {
        match polyph_plugins::read_text_under(&wt, Path::new(f)) {
            Ok(t) if !polyph_merge::has_markers(&t) => {
                out.insert(f.clone(), t);
            }
            _ => bad.push(f.clone()),
        }
    }
    drop_worktree(root, &wt).await;
    r?;
    if !bad.is_empty() {
        return Err(ApiError::Failed(format!("the resolver left conflict markers in {}", bad.join(", "))));
    }
    Ok(out)
}

/// Runs the project's checks on `commit` in a sandboxed worktree.
async fn run_checks(
    shared: &Arc<Shared>,
    project: &ProjectConfig,
    root: &Path,
    commit: &str,
    id: &str,
) -> Result<(), String> {
    let wt = temp_worktree(root, &format!("check-{}", &id[..8]), commit).await.map_err(|e| e.to_string())?;
    let r = run_checks_in(shared, project, &wt).await;
    drop_worktree(root, &wt).await;
    r
}

async fn run_checks_in(shared: &Arc<Shared>, project: &ProjectConfig, wt: &Path) -> Result<(), String> {
    let mech = agents::mechanism(shared, project).map_err(|e| e.to_string())?;
    let tmp = wt.join(".polyph-check-tmp");
    std::fs::create_dir_all(&tmp).map_err(|e| e.to_string())?;
    // Allowlisted hosts only; nobody is asked about others during checks.
    let mut allow = Allowlist::default_for_agents();
    allow.hosts.extend(project.sandbox.allow_hosts.iter().cloned());
    let deny: Decider = Arc::new(|_, _| Box::pin(async { false }));
    let run_dir = std::env::temp_dir()
        .join(format!("polyph-check-{}", ObjectId::random().map(|o| o.short()).unwrap_or_default()));
    let proxy = polyph_sandbox::proxy::start(allow, deny, Some(run_dir.clone())).await.map_err(|e| e.to_string())?;
    // Checks read the repository; they commit nothing.
    let git = {
        let wt = wt.to_path_buf();
        tokio::task::spawn_blocking(move || polyph_git::sandbox_git(&wt, false)).await.unwrap_or_default()
    };
    let policy = Policy {
        worktree: wt.to_path_buf(),
        writable: vec![],
        tmp: Some(tmp.clone()),
        hidden: Policy::default_hidden(&shared.paths.home),
        read_only: git.read_only,
        git_writable: vec![],
        proxy_tcp: Some(proxy.tcp),
        proxy_unix: proxy.unix.clone(),
        helper: shared.opts.sandbox_helper.clone(),
        container_image: project.sandbox.image.clone(),
    };
    let mut result = Ok(());
    for check in &project.checks {
        let (prog, args) = if cfg!(windows) {
            ("cmd".to_string(), vec!["/C".to_string(), check.clone()])
        } else {
            ("sh".to_string(), vec!["-c".to_string(), check.clone()])
        };
        let w = match polyph_sandbox::wrap(mech, &policy, &prog, &args) {
            Ok(w) => w,
            Err(e) => {
                result = Err(e.to_string());
                break;
            }
        };
        let mut cmd = tokio::process::Command::new(&w.program);
        cmd.args(&w.args).current_dir(&w.cwd).envs(w.env.iter().map(|(k, v)| (k, v))).kill_on_drop(true);
        cmd.stdin(std::process::Stdio::null());
        match tokio::time::timeout(CHECK_TIMEOUT, cmd.output()).await {
            Ok(Ok(o)) if o.status.success() => {}
            Ok(Ok(o)) => {
                let mut tail = String::from_utf8_lossy(&o.stderr).into_owned();
                tail.push_str(&String::from_utf8_lossy(&o.stdout));
                let tail: String =
                    tail.lines().rev().take(15).collect::<Vec<_>>().into_iter().rev().collect::<Vec<_>>().join("\n");
                result = Err(format!("check `{check}` failed on the merged code:\n{tail}"));
                break;
            }
            Ok(Err(e)) => {
                result = Err(format!("check `{check}` could not run: {e}"));
                break;
            }
            Err(_) => {
                result = Err(format!("check `{check}` took longer than {} minutes", CHECK_TIMEOUT.as_secs() / 60));
                break;
            }
        }
    }
    drop(proxy);
    let _ = std::fs::remove_dir_all(&run_dir);
    let _ = std::fs::remove_dir_all(&tmp);
    if let Err(e) = &result {
        warn!(error = %e, "merge checks failed");
    }
    result
}

/// The merge queue of a room, oldest first.
pub async fn queue(rt: &RoomRuntime) -> Vec<MergeEntry> {
    let mut ctx = rt.ctx.lock().await;
    docs::merge_entries(ctx.core.doc(&DocName::Board))
}

/// What one agent leaves the next: the task, the work so far and open ends.
async fn handoff_brief(rt: &RoomRuntime, h: &AgentHandle, commits: &[String], note: Option<&str>) -> String {
    let mut ctx = rt.ctx.lock().await;
    let mut s = format!("You are taking over this task from {}.", h.agent.label);
    if let Some(t) = h.task
        && let Some(c) = card(&mut ctx, &t.to_hex())
    {
        s.push_str(&format!("\nTask: {}", c.title));
        if !c.description.trim().is_empty() {
            s.push_str(&format!("\n{}", c.description.trim()));
        }
        for a in &c.acceptance {
            s.push_str(&format!("\n- done when: {a}"));
        }
    }
    if let Some(wt) = &h.task_wt {
        s.push_str(&format!("\nBranch {} (from {}); their work is committed there.", wt.branch, wt.base));
    }
    if !commits.is_empty() {
        s.push_str("\nCommits so far:");
        for c in commits.iter().rev().take(20).rev() {
            s.push_str(&format!("\n- {c}"));
        }
    }
    if let Some(log) = ctx.transcripts.get(&h.agent.id) {
        let edited: Vec<String> = {
            let mut v: Vec<String> = vec![];
            for e in log {
                if let AgentEventKind::Tool(t) = &e.kind
                    && t.kind == polyph_proto::messages::ToolKind::Edit
                {
                    for p in &t.paths {
                        if !v.contains(p) {
                            v.push(p.clone());
                        }
                    }
                }
            }
            v
        };
        if !edited.is_empty() {
            s.push_str(&format!("\nFiles they edited: {}", edited.join(", ")));
        }
        let said: Vec<String> = log
            .iter()
            .filter_map(|e| match &e.kind {
                AgentEventKind::Message { text } => Some(text.trim().to_string()),
                _ => None,
            })
            .filter(|t| !t.is_empty())
            .collect();
        if !said.is_empty() {
            s.push_str("\nThe last things they said:");
            for t in said.iter().rev().take(6).rev() {
                let t: String = t.chars().take(400).collect();
                s.push_str(&format!("\n> {}", t.replace('\n', "\n> ")));
            }
        }
    }
    let held: Vec<String> =
        ctx.core.claims.all().into_iter().filter(|c| c.owner.id == h.agent.id).map(|c| c.target.to_string()).collect();
    if !held.is_empty() {
        s.push_str(&format!("\nClaims they held (claim them again before editing): {}", held.join(", ")));
    }
    if let Some(n) = note.filter(|n| !n.trim().is_empty()) {
        s.push_str(&format!("\nTheir note: {}", n.trim()));
    }
    polyph_core::redact::redact(&s).into_owned()
}

/// Hands a task over: commits work in progress, leaves a brief on the card,
/// stops the agent and, with `to_kind`, starts its successor right away.
pub async fn handoff(
    daemon: Arc<Daemon>,
    h: Arc<AgentHandle>,
    to_kind: Option<String>,
    note: Option<String>,
) -> Result<Value, ApiError> {
    let rt = daemon.find(&h.room.to_hex()).await?;
    let mut commits = vec![];
    if let Some(wt) = h.task_wt.clone() {
        let sig = Signature::agent(&h.agent.label, &h.agent.id.short());
        let msg = format!("Work in progress, handed over by {}", h.agent.label);
        commits = blocking(move || -> Result<Vec<String>, ApiError> {
            let r = Repo::open(&wt.repo).and_then(|m| m.open_worktree(&wt.path)).map_err(git_err)?;
            r.commit_all(&msg, &sig).map_err(git_err)?;
            r.log_subjects(&wt.base, &wt.branch).map_err(git_err)
        })
        .await??;
    }
    let brief = handoff_brief(&rt, &h, &commits, note.as_deref()).await;
    let held: Vec<polyph_proto::messages::ClaimTarget> = {
        let ctx = rt.ctx.lock().await;
        ctx.core.claims.all().into_iter().filter(|c| c.owner.id == h.agent.id).map(|c| c.target).collect()
    };
    if let Some(t) = h.task {
        update_card(
            &rt,
            &daemon.shared,
            &t.to_hex(),
            None,
            &[(polyph_proto::crdt::card::HANDOFF, Some(brief.clone()))],
        )
        .await;
    }
    agents::stop(&daemon.shared, &daemon.agents, &rt, h.agent.id).await?;
    let Some(kind) = to_kind else {
        return Ok(json!({ "brief": brief, "next": null }));
    };
    let cwd = h.task_wt.as_ref().map(|w| w.repo.clone()).unwrap_or_else(|| h.worktree.clone());
    let started = agents::start(
        daemon.shared.clone(),
        &daemon.agents,
        rt.clone(),
        StartParams {
            room: rt.id.to_hex(),
            kind,
            cwd,
            preset: Some(format!("{:?}", h.preset).to_lowercase().replace("askall", "ask-all")),
            task: h.task.map(|t| t.to_hex()),
            prompt: None,
            worktree: None,
            helper: false,
        },
    )
    .await?;
    // The successor holds what its predecessor held.
    if !held.is_empty() {
        let next = daemon.agent(started["id"].as_str().unwrap_or_default()).await?;
        let mut ctx = rt.ctx.lock().await;
        let now = daemon.shared.clock.now();
        // Nobody waits for the answer: the grant simply stands.
        if let Ok((_, fx)) =
            ctx.core.request_claims(&next.agent, &held, "taken over", next.task, polyph_core::room::DEFAULT_LEASE, now)
        {
            apply(&rt, &mut ctx, &daemon.shared, fx).await;
        }
    }
    Ok(json!({ "brief": brief, "next": started }))
}
