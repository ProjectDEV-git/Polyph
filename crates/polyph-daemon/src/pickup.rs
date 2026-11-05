//! `next_task`: an agent takes the next ready task, and no two agents ever
//! get the same one.
//!
//! The board is a CRDT, so two agents reading it at once would both see the
//! same card free. Taking a card therefore goes through the room's claim
//! arbiter (the acting host, which serializes every claim): the agent claims
//! the card's own target, `.polyph/tasks/<card id>`, and only the agent the
//! arbiter grants it to assigns the card. A denied agent moves on to the next
//! candidate. The lease lasts long enough for the assignment to reach
//! everyone, after which the card's assignee keeps others away.
//!
//! Candidates are cards in `ready` that are not drafts, have no assignee,
//! whose dependencies are done and whose predicted files nobody else holds.
//! Cards in the caller's code areas (see `areas`) come first, then board order.

use polyph_mcp::ToolOutput;
use polyph_proto::crdt::{Board, CardView};
use polyph_proto::messages::{ClaimTarget, DocName};
use serde_json::json;

use crate::areas::Areas;
use crate::coord::Caller;
use crate::daemon::Shared;
use crate::runtime::RoomRuntime;

/// Where cards' leases live in the claim space.
pub const TASK_PREFIX: &str = ".polyph/tasks/";

/// Where a card's lease lives in the claim space.
pub fn task_target(card: &str) -> ClaimTarget {
    ClaimTarget { path: format!("{TASK_PREFIX}{card}"), symbol: None }
}

/// Candidate cards, best first.
pub fn candidates(cards: &[CardView], held_by_others: &[String], areas: &Areas, who: &str) -> Vec<CardView> {
    let done = |id: &String| cards.iter().find(|c| &c.id == id).is_none_or(|c| c.column == "done");
    let overlaps = |f: &String| {
        held_by_others
            .iter()
            .any(|h| h == f || (h.ends_with('/') && f.starts_with(h.as_str())) || f.starts_with(&format!("{h}/")))
    };
    let mut v: Vec<(bool, usize, CardView)> = cards
        .iter()
        .filter(|c| c.column == "ready" && !c.draft && c.assignee.is_none())
        .filter(|c| c.depends_on.iter().all(done))
        .filter(|c| !c.files.iter().any(overlaps))
        .enumerate()
        .map(|(i, c)| {
            let mine = c.files.iter().any(|f| areas.owner_of(f).is_some_and(|a| a.owned_by(who)))
                || c.suggested_assignee.as_deref().is_some_and(|s| crate::areas::same_owner(s, who));
            (!mine, i, c.clone())
        })
        .collect();
    v.sort_by(|a, b| {
        a.0.cmp(&b.0).then(a.2.order.partial_cmp(&b.2.order).unwrap_or(std::cmp::Ordering::Equal)).then(a.1.cmp(&b.1))
    });
    v.into_iter().map(|(_, _, c)| c).collect()
}

/// Tries candidates in turn until the arbiter grants one.
const MAX_TRIES: usize = 8;

pub async fn next_task(h: &Caller, rt: &RoomRuntime, shared: &Shared) -> Result<ToolOutput, ToolOutput> {
    // One open task at a time.
    if let Some(mine) = crate::coord::assigned_card(rt, &h.agent.label).await {
        let mut ctx = rt.ctx.lock().await;
        let c = crate::tasks::card(&mut ctx, &mine);
        let title = c.map(|c| c.title).unwrap_or_default();
        return Ok(ToolOutput::ok(
            format!(
                "You already have a task: [{}] {title}. Finish it with finish_task first.",
                &mine[..8.min(mine.len())]
            ),
            json!({ "id": mine, "already": true }),
        ));
    }
    let areas = crate::areas::load(&h.worktree);
    let list = {
        let mut ctx = rt.ctx.lock().await;
        let now = shared.clock.now();
        let cards = Board::new(ctx.core.doc(&DocName::Board)).all_cards();
        let held: Vec<String> = ctx
            .core
            .claims
            .all()
            .into_iter()
            .filter(|c| c.owner.id != h.agent.id && c.expires_at > now)
            .map(|c| c.target.path)
            .collect();
        candidates(&cards, &held, &areas, &h.agent.label)
    };
    if list.is_empty() {
        return Ok(ToolOutput::ok(
            "No task is ready: nothing in the ready column is free, unblocked and clear of others' files. Ask a person, or call recent_work and team_state to see what is going on.",
            json!({ "id": null }),
        ));
    }
    for card in list.iter().take(MAX_TRIES) {
        let target = task_target(&card.id);
        let (granted, _) =
            crate::coord::request_claims(h, rt, shared, &[target], &format!("task: {}", card.title)).await?;
        if granted.is_empty() {
            // Another agent took it a moment ago.
            continue;
        }
        // Granted: it is ours. Re-check the card in case it moved meanwhile.
        let current = {
            let mut ctx = rt.ctx.lock().await;
            crate::tasks::card(&mut ctx, &card.id)
        };
        let Some(current) = current.filter(|c| c.assignee.is_none() && c.column == "ready") else { continue };
        crate::tasks::update_card(
            rt,
            shared,
            &current.id,
            Some("claimed"),
            &[("assignee", Some(h.agent.label.clone()))],
        )
        .await;
        crate::feed::log(
            rt,
            shared,
            "task_taken",
            Some(&h.agent.label),
            json!({ "card": current.id, "title": current.title }),
        )
        .await;
        let mut text = format!("Your task: [{}] {}", &current.id[..8.min(current.id.len())], current.title);
        if !current.description.trim().is_empty() {
            text.push_str(&format!("\n{}", current.description.trim()));
        }
        for a in &current.acceptance {
            text.push_str(&format!("\n- done when: {a}"));
        }
        if !current.files.is_empty() {
            text.push_str(&format!("\nPredicted files: {}. Claim them before editing.", current.files.join(", ")));
        }
        if let Some(a) = current.files.iter().find_map(|f| areas.owner_of(f)) {
            text.push_str(&format!("\nIt is in {}.", a.describe()));
        }
        text.push_str("\nCall get_context for the brief and the notes on these files; call finish_task when done.");
        return Ok(ToolOutput::ok(text, json!({ "id": current.id, "title": current.title, "files": current.files })));
    }
    Ok(ToolOutput::ok("Other agents took the ready tasks first. Try again in a moment.", json!({ "id": null })))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn card(id: &str, column: &str, files: &[&str], deps: &[&str], order: f64) -> CardView {
        CardView {
            id: id.into(),
            title: id.into(),
            column: column.into(),
            order,
            files: files.iter().map(|s| s.to_string()).collect(),
            depends_on: deps.iter().map(|s| s.to_string()).collect(),
            ..Default::default()
        }
    }

    #[test]
    fn picks_free_unblocked_cards_in_my_areas_first() {
        let cards = vec![
            card("a", "ready", &["src/ui/a.ts"], &[], 1.0),
            card("b", "ready", &["src/billing/b.rs"], &[], 2.0),
            card("c", "ready", &[], &["x"], 0.5),
            card("x", "in_progress", &[], &[], 0.0),
            card("d", "ready", &["src/held.rs"], &[], 0.1),
            card("e", "backlog", &[], &[], 0.0),
            card("f", "ready", &["src/ui/f.ts"], &["done1"], 3.0),
            card("done1", "done", &[], &[], 0.0),
        ];
        let areas = crate::areas::parse("src/billing/ @ann\n", None);
        let held = vec!["src/held.rs".to_string()];
        let ids: Vec<String> = candidates(&cards, &held, &areas, "Ann/codex").into_iter().map(|c| c.id).collect();
        assert_eq!(ids, vec!["b", "a", "f"], "my area first, then board order; blocked, held and unready left out");
        let ids: Vec<String> =
            candidates(&cards, &["src/ui/".into()], &areas, "Bo/codex").into_iter().map(|c| c.id).collect();
        assert_eq!(ids, vec!["d", "b"], "a held directory covers its files; board order for Bo");
    }
}
