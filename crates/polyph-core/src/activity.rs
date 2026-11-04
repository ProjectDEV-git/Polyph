//! The activity feed and board replay.
//!
//! Every local change to the board is diffed and written to the Activity
//! document by its author (`card_created`, `card_moved`, `card_updated`,
//! `card_removed`), alongside events the daemon records (merges, overlaps,
//! agents, approvals). Every [`CHECKPOINT_EVERY`] card events, an entry also
//! carries the whole board as JSON, so the board at any moment is the latest
//! checkpoint before it plus the card events after it ([`replay`]).

use std::collections::BTreeMap;

use polyph_proto::crdt::{Activity, ActivityEntry, Board, CardView};
use serde::Serialize;
use serde_json::json;
use yrs::Doc;

pub const CHECKPOINT_EVERY: usize = 50;
const CARD_KINDS: [&str; 4] = ["card_created", "card_moved", "card_updated", "card_removed"];

pub fn is_card_event(kind: &str) -> bool {
    CARD_KINDS.contains(&kind)
}

/// Fields that count as an edit (order alone is a move within a column).
fn edited(a: &CardView, b: &CardView) -> Vec<&'static str> {
    let mut v = vec![];
    let mut f = |name, changed: bool| {
        if changed {
            v.push(name);
        }
    };
    f("title", a.title != b.title);
    f("description", a.description != b.description);
    f("assignee", a.assignee != b.assignee);
    f("acceptance", a.acceptance != b.acceptance);
    f("files", a.files != b.files);
    f("depends_on", a.depends_on != b.depends_on);
    f("branch", a.branch != b.branch);
    f("pr", a.pr != b.pr);
    f("draft", a.draft != b.draft);
    f("size", a.size != b.size);
    v
}

/// The activity entries for a board change, without ids or checkpoints.
pub fn diff(before: &[CardView], after: &[CardView]) -> Vec<(String, serde_json::Value)> {
    let b: BTreeMap<&str, &CardView> = before.iter().map(|c| (c.id.as_str(), c)).collect();
    let a: BTreeMap<&str, &CardView> = after.iter().map(|c| (c.id.as_str(), c)).collect();
    let mut out = vec![];
    for (id, c) in &a {
        match b.get(id) {
            None => out.push((
                "card_created".to_string(),
                json!({ "card": id, "title": c.title, "column": c.column, "draft": c.draft }),
            )),
            Some(old) => {
                if old.column != c.column {
                    out.push((
                        "card_moved".to_string(),
                        json!({ "card": id, "title": c.title, "from": old.column, "to": c.column }),
                    ));
                }
                let fields = edited(old, c);
                if !fields.is_empty() {
                    let mut args = json!({ "card": id, "title": c.title, "fields": fields });
                    if fields.contains(&"assignee") {
                        args["assignee"] = json!(c.assignee);
                    }
                    if fields.contains(&"pr") {
                        args["pr"] = json!(c.pr);
                    }
                    out.push(("card_updated".to_string(), args));
                }
            }
        }
    }
    for (id, c) in &b {
        if !a.contains_key(id) {
            out.push(("card_removed".to_string(), json!({ "card": id, "title": c.title })));
        }
    }
    out
}

/// Appends entries for a board change; adds a checkpoint when one is due.
pub fn record_board_change(
    activity: &Doc,
    board_after: &[CardView],
    changes: Vec<(String, serde_json::Value)>,
    actor: &str,
    agent: Option<&str>,
    now: i64,
    new_id: &mut dyn FnMut() -> String,
) {
    if changes.is_empty() {
        return;
    }
    let log = Activity::new(activity);
    let since_checkpoint = {
        let all = log.entries();
        let last = all.iter().rposition(|e| e.checkpoint.is_some());
        all.iter().skip(last.map(|i| i + 1).unwrap_or(0)).filter(|e| is_card_event(&e.kind)).count()
    };
    let n = changes.len();
    for (i, (kind, args)) in changes.into_iter().enumerate() {
        let checkpoint = (i + 1 == n && since_checkpoint + n >= CHECKPOINT_EVERY)
            .then(|| serde_json::to_string(board_after).unwrap_or_default());
        let e = ActivityEntry {
            id: new_id(),
            at: now,
            summary_key: format!("activity-{}", kind.replace('_', "-")),
            kind,
            actor: actor.to_string(),
            agent: agent.map(str::to_string),
            args,
            checkpoint,
        };
        let _ = log.append(&e);
    }
}

/// Cards as they stood at a moment, per column.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct BoardAt {
    pub at: i64,
    /// Column id to (card id, title), in no particular order within a column.
    pub columns: BTreeMap<String, Vec<(String, String)>>,
    /// Entries applied since the checkpoint used.
    pub applied: usize,
}

/// Rebuilds the board at `at` from the activity entries (sorted by time).
pub fn replay(entries: &[ActivityEntry], at: i64) -> BoardAt {
    let upto: Vec<&ActivityEntry> = entries.iter().filter(|e| e.at <= at).collect();
    let start = upto.iter().rposition(|e| e.checkpoint.is_some());
    // card id -> (column, title)
    let mut cards: BTreeMap<String, (String, String)> = BTreeMap::new();
    if let Some(i) = start
        && let Some(cp) = &upto[i].checkpoint
        && let Ok(list) = serde_json::from_str::<Vec<CardView>>(cp)
    {
        for c in list {
            cards.insert(c.id, (c.column, c.title));
        }
    }
    let mut applied = 0;
    for e in upto.iter().skip(start.map(|i| i + 1).unwrap_or(0)) {
        let id = e.args["card"].as_str().unwrap_or_default().to_string();
        let title = e.args["title"].as_str().unwrap_or_default().to_string();
        match e.kind.as_str() {
            "card_created" => {
                cards.insert(id, (e.args["column"].as_str().unwrap_or_default().to_string(), title));
            }
            "card_moved" => {
                let to = e.args["to"].as_str().unwrap_or_default().to_string();
                cards.entry(id).and_modify(|c| c.0 = to.clone()).or_insert((to, title));
            }
            "card_updated" => {
                if let Some(c) = cards.get_mut(&id) {
                    c.1 = title;
                }
            }
            "card_removed" => {
                cards.remove(&id);
            }
            _ => continue,
        }
        applied += 1;
    }
    let mut columns: BTreeMap<String, Vec<(String, String)>> = BTreeMap::new();
    for (id, (col, title)) in cards {
        columns.entry(col).or_default().push((id, title));
    }
    BoardAt { at, columns, applied }
}

/// Snapshot of the board's cards, for diffing around a change.
pub fn cards(board: &Doc) -> Vec<CardView> {
    Board::new(board).all_cards()
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;
    use polyph_proto::crdt::NewCard;

    fn ids() -> impl FnMut() -> String {
        let mut n = 0;
        move || {
            n += 1;
            format!("e{n:04}")
        }
    }

    #[test]
    fn diff_and_replay() {
        let board = Doc::new();
        let act = Doc::new();
        let mut id = ids();
        let mut t = 0;
        let mut change = |f: &dyn Fn(&Board<'_>)| {
            t += 10;
            let before = cards(&board);
            f(&Board::new(&board));
            let after = cards(&board);
            record_board_change(&act, &after, diff(&before, &after), "ann", None, t, &mut id);
        };
        change(&|b| {
            b.add_card(&NewCard {
                id: "c1".into(),
                title: "Login".into(),
                column: "backlog".into(),
                ..Default::default()
            })
            .unwrap()
        });
        change(&|b| b.move_card("c1", "in_progress", None, 1).unwrap());
        change(&|b| b.set_field("c1", "title", yrs::Any::String("Log in".into()), 1).unwrap());
        change(&|b| {
            b.add_card(&NewCard { id: "c2".into(), title: "Docs".into(), column: "ready".into(), ..Default::default() })
                .unwrap()
        });
        change(&|b| b.remove_card("c2"));
        let entries = Activity::new(&act).entries();
        let kinds: Vec<&str> = entries.iter().map(|e| e.kind.as_str()).collect();
        assert_eq!(kinds, ["card_created", "card_moved", "card_updated", "card_created", "card_removed"]);
        assert_eq!(entries[2].args["fields"], json!(["title"]));
        // Board at t=20: Login in progress; at t=40: plus Docs in ready; at 50: Docs gone.
        assert_eq!(replay(&entries, 20).columns["in_progress"], vec![("c1".to_string(), "Login".to_string())]);
        assert_eq!(replay(&entries, 40).columns["ready"], vec![("c2".to_string(), "Docs".to_string())]);
        assert!(!replay(&entries, 50).columns.contains_key("ready"));
        assert_eq!(replay(&entries, 50).columns["in_progress"][0].1, "Log in");
        assert!(replay(&entries, 5).columns.is_empty());
    }

    #[test]
    fn checkpoints_bound_replay_work() {
        let board = Doc::new();
        let act = Doc::new();
        let mut id = ids();
        for i in 0..(CHECKPOINT_EVERY + 5) {
            let before = cards(&board);
            Board::new(&board)
                .add_card(&NewCard {
                    id: format!("c{i:03}"),
                    title: format!("T{i}"),
                    column: "backlog".into(),
                    ..Default::default()
                })
                .unwrap();
            let after = cards(&board);
            record_board_change(&act, &after, diff(&before, &after), "ann", None, i as i64, &mut id);
        }
        let entries = Activity::new(&act).entries();
        assert_eq!(entries.iter().filter(|e| e.checkpoint.is_some()).count(), 1);
        let r = replay(&entries, 1000);
        assert_eq!(r.columns["backlog"].len(), CHECKPOINT_EVERY + 5);
        assert_eq!(r.applied, 5, "replay starts at the checkpoint");
    }
}
