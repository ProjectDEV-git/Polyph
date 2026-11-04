//! The planner: an agent splits a goal into a task graph, which Polyph
//! validates and writes to the board as draft cards for people to accept.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use polyph_agents::project::ProjectConfig;
use polyph_core::docs;
use polyph_proto::crdt::{Board, NewCard, card};
use polyph_proto::ids::ObjectId;
use polyph_proto::messages::DocName;
use polyph_proto::plan::{TASK_GRAPH_SCHEMA, TaskGraph};
use serde::Deserialize;
use serde_json::{Value, json};
use yrs::Any;

use crate::daemon::{ApiError, Daemon};
use crate::runtime::{RoomRuntime, apply};
use crate::tasks::{Helper, drop_worktree, temp_worktree};

#[derive(Debug, Deserialize)]
pub struct PlanParams {
    pub room: String,
    pub goal: String,
    /// Repository (or any directory) the planner reads.
    pub cwd: PathBuf,
    /// Planner agent kind; default `[planner] agent` in polyph.toml.
    pub kind: Option<String>,
}

/// The JSON object in an agent's answer: a ```json block, or the outermost braces.
pub fn extract_json(text: &str) -> Option<&str> {
    if let Some(start) = text.find("```json") {
        let rest = &text[start + 7..];
        if let Some(end) = rest.find("```") {
            return Some(rest[..end].trim());
        }
    }
    let (a, b) = (text.find('{')?, text.rfind('}')?);
    (a < b).then(|| &text[a..=b])
}

fn parse(text: &str) -> Result<TaskGraph, String> {
    let json = extract_json(text).ok_or("the answer had no JSON object")?;
    let g: TaskGraph = serde_json::from_str(json).map_err(|e| format!("the JSON does not match the schema: {e}"))?;
    g.validate().map_err(|e| format!("the plan is not valid: {e}"))?;
    Ok(g)
}

fn prompt(goal: &str, team: &str) -> String {
    format!(
        "Plan this goal for a team whose agents work in parallel, each on its own branch:\n\n{goal}\n\n\
The repository is checked out in your working directory. Read what you need; do not edit anything.\n\n\
The team right now:\n{team}\n\n\
Split the goal into tasks that different agents can do at the same time while touching as few of the same files \
as possible. For each task give: a short id (t1, t2, ...), a title, a description, acceptance criteria (what done \
means), the files it will most likely change (predicted_files), the ids it depends on (depends_on), a size \
(s, m, l or xl), and optionally suggested_assignee (an agent label or a person from the team).\n\n\
Answer with only a JSON object matching this schema:\n{TASK_GRAPH_SCHEMA}"
    )
}

/// Runs the planner and writes its plan as draft cards.
pub async fn create(daemon: Arc<Daemon>, p: PlanParams) -> Result<Value, ApiError> {
    let rt = daemon.find(&p.room).await?;
    if p.goal.trim().is_empty() {
        return Err(ApiError::BadRequest("the goal is empty".into()));
    }
    let cwd = p.cwd.canonicalize().map_err(|e| ApiError::BadRequest(format!("{}: {e}", p.cwd.display())))?;
    let project = ProjectConfig::load(&cwd).unwrap_or_default();
    let kind = p.kind.or(project.planner.agent.clone()).ok_or_else(|| {
        ApiError::BadRequest("no planner agent: pass one or set [planner] agent in polyph.toml".into())
    })?;
    let goal = polyph_core::redact::redact(p.goal.trim()).into_owned();
    let team = {
        let ctx = rt.ctx.lock().await;
        crate::coord::team_state(&ctx, daemon.shared.clock.now()).text
    };
    // In a git repository the planner reads a scratch checkout, never yours.
    let repo = polyph_git::Repo::open(&cwd).ok().and_then(|r| r.main_root().ok());
    let plan_id = ObjectId::random().map_err(|e| ApiError::Failed(e.to_string()))?.to_hex();
    let scratch = match &repo {
        Some(root) => Some((root.clone(), temp_worktree(root, &format!("plan-{}", &plan_id[..8]), "HEAD").await?)),
        None => None,
    };
    let dir = scratch.as_ref().map(|(_, w)| w.clone()).unwrap_or_else(|| cwd.clone());
    let result = async {
        let helper = Helper::start(&daemon, &rt, &kind, &dir).await?;
        let mut answer = helper.turn(&daemon, &rt, &prompt(&goal, &team)).await;
        let mut graph = answer.as_ref().map_err(|e| e.to_string()).and_then(|a| parse(a));
        // One retry, telling the planner what was wrong.
        if let Err(why) = &graph
            && answer.is_ok()
        {
            answer = helper
                .turn(&daemon, &rt, &format!("{why}. Answer again with only the JSON object, matching the schema."))
                .await;
            graph = answer.as_ref().map_err(|e| e.to_string()).and_then(|a| parse(a));
        }
        helper.stop(&daemon, &rt).await;
        graph.map_err(|why| ApiError::Failed(format!("the planner's answer could not be used: {why}")))
    }
    .await;
    if let Some((root, w)) = &scratch {
        drop_worktree(root, w).await;
    }
    let graph = result?;
    write_cards(&daemon, &rt, &plan_id, &goal, &graph).await
}

async fn write_cards(
    daemon: &Arc<Daemon>,
    rt: &RoomRuntime,
    plan_id: &str,
    goal: &str,
    g: &TaskGraph,
) -> Result<Value, ApiError> {
    let now = daemon.shared.clock.now();
    let ids: HashMap<String, String> = g
        .tasks
        .iter()
        .map(|t| Ok((t.id.clone(), ObjectId::random()?.to_hex())))
        .collect::<Result<_, polyph_proto::ProtoError>>()
        .map_err(|e| ApiError::Failed(e.to_string()))?;
    let overlaps: Vec<Value> = g
        .predicted_overlaps()
        .into_iter()
        .map(|(a, b, files)| json!({ "a": ids.get(&a), "b": ids.get(&b), "files": files }))
        .collect();
    let order = g.topo_order().map_err(|e| ApiError::Failed(e.to_string()))?;
    let mut ctx = rt.ctx.lock().await;
    let me = ctx.core.my_member.to_hex();
    let mut created = vec![];
    let fx = ctx.core.change_doc(DocName::Board, |d| {
        let b = Board::new(d);
        for t in &order {
            let id = ids[&t.id].clone();
            let red = |s: &str| polyph_core::redact::redact(s).into_owned();
            let c = NewCard {
                id: id.clone(),
                title: red(&t.title),
                description: red(&t.description),
                column: "backlog".into(),
                created_by: me.clone(),
                created_at: now,
                acceptance: t.acceptance.iter().map(|a| red(a)).collect(),
                files: t.predicted_files.clone(),
                depends_on: t.depends_on.iter().filter_map(|x| ids.get(x).cloned()).collect(),
                size: Some(format!("{:?}", t.size).to_lowercase()),
                kind: Some("task".into()),
                draft: true,
            };
            if b.add_card(&c).is_ok() {
                let _ = b.set_field(&id, card::PLAN, Any::String(plan_id.into()), now);
                if let Some(s) = &t.suggested_assignee {
                    let _ = b.set_field(&id, card::SUGGESTED_ASSIGNEE, Any::String(s.as_str().into()), now);
                }
                created.push(json!({ "id": id, "planner_id": t.id, "title": c.title, "depends_on": c.depends_on }));
            }
        }
        docs::set_setting(
            d,
            &format!("plan:{plan_id}"),
            &json!({ "goal": goal, "cards": created.iter().map(|c| c["id"].clone()).collect::<Vec<_>>(), "overlaps": overlaps, "at": now }),
        );
    })?;
    apply(rt, &mut ctx, &daemon.shared, fx).await;
    Ok(json!({ "plan": plan_id, "goal": goal, "cards": created, "overlaps": overlaps }))
}

/// Accepts a plan's draft cards: tasks without dependencies become Ready.
pub async fn accept(daemon: &Arc<Daemon>, room: &str, plan: &str) -> Result<Value, ApiError> {
    let rt = daemon.find(room).await?;
    let now = daemon.shared.clock.now();
    let mut ctx = rt.ctx.lock().await;
    let want = plan.to_lowercase();
    let cards: Vec<_> = Board::new(ctx.core.doc(&DocName::Board))
        .all_cards()
        .into_iter()
        .filter(|c| c.draft && c.plan.as_deref().is_some_and(|p| p.starts_with(&want)))
        .collect();
    if cards.is_empty() {
        return Err(ApiError::NotFound("no draft cards from that plan".into()));
    }
    let mut ready = 0;
    let fx = ctx.core.change_doc(DocName::Board, |d| {
        let b = Board::new(d);
        for c in &cards {
            let _ = b.set_field(&c.id, card::DRAFT, Any::Bool(false), now);
            if c.depends_on.is_empty() {
                let last = b.cards_in("ready").last().map(|x| x.id.clone());
                let _ = b.move_card(&c.id, "ready", last.as_deref(), now);
                ready += 1;
            }
        }
    })?;
    apply(&rt, &mut ctx, &daemon.shared, fx).await;
    Ok(json!({ "accepted": cards.len(), "ready": ready }))
}

/// Moves cards whose dependencies are all done from Backlog to Ready.
pub async fn promote_unblocked(rt: &RoomRuntime, shared: &crate::daemon::Shared) {
    let now = shared.clock.now();
    let mut ctx = rt.ctx.lock().await;
    let all = Board::new(ctx.core.doc(&DocName::Board)).all_cards();
    let done: Vec<&str> = all.iter().filter(|c| c.column == "done").map(|c| c.id.as_str()).collect();
    let free: Vec<String> = all
        .iter()
        .filter(|c| c.column == "backlog" && !c.draft && !c.depends_on.is_empty())
        .filter(|c| c.depends_on.iter().all(|d| done.contains(&d.as_str())))
        .map(|c| c.id.clone())
        .collect();
    if free.is_empty() {
        return;
    }
    if let Ok(fx) = ctx.core.change_doc(DocName::Board, |d| {
        let b = Board::new(d);
        for id in &free {
            let last = b.cards_in("ready").last().map(|x| x.id.clone());
            let _ = b.move_card(id, "ready", last.as_deref(), now);
        }
    }) {
        apply(rt, &mut ctx, shared, fx).await;
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    #[test]
    fn finds_the_json() {
        assert_eq!(extract_json("Here:\n```json\n{\"a\":1}\n```\nDone."), Some("{\"a\":1}"));
        assert_eq!(extract_json("plan: {\"a\": {\"b\": 2}} ok"), Some("{\"a\": {\"b\": 2}}"));
        assert_eq!(extract_json("no plan"), None);
        assert!(parse("{\"goal\": \"x\", \"tasks\": []}").unwrap_err().contains("no tasks"));
    }
}
