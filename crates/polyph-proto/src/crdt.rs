//! Shared document layouts. The same layout is used by `yjs` in TypeScript,
//! so field names here are part of the protocol.
//!
//! Board document:
//! - `columns`: Y.Array of column ids (strings), in display order
//! - `column_titles`: Y.Map column id -> title
//! - `cards`: Y.Map card id -> Y.Map with the `card::*` fields
//!
//! Moving a card writes `column` and `order` on that card. Each field is
//! last-writer-wins inside a Y.Map, so concurrent moves converge.
//!
//! Threads document: `threads` Y.Map id -> Y.Map, `posts` Y.Map thread id -> Y.Array of Y.Map.
//! Activity document: `events` Y.Array of Y.Map (append-only).
//! Whiteboard document: `elements` Y.Map element id -> JSON string of the
//! Excalidraw element, `order` Y.Array of ids, `meta` Y.Map (name, frames).

use serde::{Deserialize, Serialize};
use yrs::types::ToJson;
use yrs::updates::decoder::Decode;
use yrs::updates::encoder::Encode;
use yrs::{Any, Array, ArrayRef, Doc, Map, MapPrelim, MapRef, Out, ReadTxn, StateVector, Transact, Update};

use crate::error::{ProtoError, Result};

pub mod board {
    pub const COLUMNS: &str = "columns";
    pub const COLUMN_TITLES: &str = "column_titles";
    pub const CARDS: &str = "cards";
    pub const DEFAULT_COLUMNS: [(&str, &str); 6] = [
        ("backlog", "Backlog"),
        ("ready", "Ready"),
        ("claimed", "Claimed"),
        ("in_progress", "In progress"),
        ("review", "Review"),
        ("done", "Done"),
    ];
}

pub mod card {
    pub const ID: &str = "id";
    pub const TITLE: &str = "title";
    pub const DESCRIPTION: &str = "description";
    pub const COLUMN: &str = "column";
    /// Fractional index; cards sort by (order, id).
    pub const ORDER: &str = "order";
    pub const ASSIGNEE: &str = "assignee";
    pub const CREATED_BY: &str = "created_by";
    pub const CREATED_AT: &str = "created_at";
    pub const UPDATED_AT: &str = "updated_at";
    /// JSON array string of acceptance criteria.
    pub const ACCEPTANCE: &str = "acceptance";
    /// JSON array string of predicted files.
    pub const FILES: &str = "files";
    /// JSON array string of card ids this card depends on.
    pub const DEPENDS_ON: &str = "depends_on";
    pub const BRANCH: &str = "branch";
    pub const PR: &str = "pr";
    pub const THREAD: &str = "thread";
    pub const SIZE: &str = "size";
    pub const KIND: &str = "kind";
    pub const AUTO_PICKUP: &str = "auto_pickup";
    pub const DRAFT: &str = "draft";
    /// Brief left by an agent that handed the task over, for the next one.
    pub const HANDOFF: &str = "handoff";
    /// Id of the plan that created the card.
    pub const PLAN: &str = "plan";
    /// Agent label or person the planner suggested.
    pub const SUGGESTED_ASSIGNEE: &str = "suggested_assignee";
}

pub mod activity {
    pub const EVENTS: &str = "events";
}

pub mod threads {
    pub const THREADS: &str = "threads";
    pub const POSTS: &str = "posts";
}

pub mod whiteboard {
    pub const ELEMENTS: &str = "elements";
    pub const ORDER: &str = "order";
    pub const META: &str = "meta";
}

/// Card as read from the board document.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct CardView {
    pub id: String,
    pub title: String,
    pub description: String,
    pub column: String,
    pub order: f64,
    pub assignee: Option<String>,
    pub created_by: String,
    pub created_at: i64,
    pub acceptance: Vec<String>,
    pub files: Vec<String>,
    pub depends_on: Vec<String>,
    pub branch: Option<String>,
    pub pr: Option<String>,
    pub thread: Option<String>,
    pub size: Option<String>,
    pub kind: Option<String>,
    pub draft: bool,
    #[serde(default)]
    pub handoff: Option<String>,
    #[serde(default)]
    pub plan: Option<String>,
    #[serde(default)]
    pub suggested_assignee: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct NewCard {
    pub id: String,
    pub title: String,
    pub description: String,
    pub column: String,
    pub created_by: String,
    pub created_at: i64,
    pub acceptance: Vec<String>,
    pub files: Vec<String>,
    pub depends_on: Vec<String>,
    pub size: Option<String>,
    pub kind: Option<String>,
    pub draft: bool,
}

pub fn encode_full(doc: &Doc) -> Vec<u8> {
    doc.transact().encode_state_as_update_v1(&StateVector::default())
}

pub fn state_vector(doc: &Doc) -> Vec<u8> {
    doc.transact().state_vector().encode_v1()
}

/// Update containing everything the peer with `sv` is missing.
pub fn diff_since(doc: &Doc, sv: &[u8]) -> Result<Vec<u8>> {
    let sv = StateVector::decode_v1(sv).map_err(|_| ProtoError::Decode("state vector".into()))?;
    Ok(doc.transact().encode_state_as_update_v1(&sv))
}

pub fn apply_update(doc: &Doc, update: &[u8]) -> Result<()> {
    let u = Update::decode_v1(update).map_err(|_| ProtoError::Decode("crdt update".into()))?;
    doc.transact_mut().apply_update(u).map_err(|_| ProtoError::Decode("crdt update could not be applied".into()))
}

fn any_str(v: Option<Out>) -> Option<String> {
    match v {
        Some(Out::Any(Any::String(s))) => Some(s.to_string()),
        _ => None,
    }
}

fn any_f64(v: Option<Out>) -> Option<f64> {
    match v {
        Some(Out::Any(Any::Number(yrs::any::Number::Float(n)))) => Some(n),
        Some(Out::Any(Any::Number(yrs::any::Number::Int(n)))) => Some(n as f64),
        _ => None,
    }
}

fn any_bool(v: Option<Out>) -> bool {
    matches!(v, Some(Out::Any(Any::Bool(true))))
}

fn json_list(v: Option<Out>) -> Vec<String> {
    any_str(v).and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default()
}

/// Typed access to a board document.
#[derive(Debug)]
pub struct Board<'a> {
    pub doc: &'a Doc,
}

impl<'a> Board<'a> {
    pub fn new(doc: &'a Doc) -> Self {
        Self { doc }
    }

    fn cards(&self) -> MapRef {
        self.doc.get_or_insert_map(board::CARDS)
    }

    /// Creates default columns if the board has none.
    pub fn init_columns(&self, columns: &[(&str, &str)]) {
        let cols: ArrayRef = self.doc.get_or_insert_array(board::COLUMNS);
        let titles = self.doc.get_or_insert_map(board::COLUMN_TITLES);
        let mut txn = self.doc.transact_mut();
        if cols.len(&txn) > 0 {
            return;
        }
        for (id, title) in columns {
            cols.push_back(&mut txn, *id);
            titles.insert(&mut txn, *id, *title);
        }
    }

    pub fn columns(&self) -> Vec<(String, String)> {
        let cols: ArrayRef = self.doc.get_or_insert_array(board::COLUMNS);
        let titles = self.doc.get_or_insert_map(board::COLUMN_TITLES);
        let txn = self.doc.transact();
        cols.iter(&txn)
            .filter_map(|v| match v {
                Out::Any(Any::String(s)) => Some(s.to_string()),
                _ => None,
            })
            .map(|id| {
                let t = any_str(titles.get(&txn, &id)).unwrap_or_else(|| id.clone());
                (id, t)
            })
            .collect()
    }

    pub fn add_card(&self, c: &NewCard) -> Result<()> {
        let cards = self.cards();
        let order = self.next_order(&c.column);
        let mut txn = self.doc.transact_mut();
        let enc = |v: &Vec<String>| serde_json::to_string(v).unwrap_or_else(|_| "[]".into());
        let mut fields: Vec<(&str, Any)> = vec![
            (card::ID, Any::from(c.id.as_str())),
            (card::TITLE, Any::from(c.title.as_str())),
            (card::DESCRIPTION, Any::from(c.description.as_str())),
            (card::COLUMN, Any::from(c.column.as_str())),
            (card::ORDER, Any::from(order)),
            (card::CREATED_BY, Any::from(c.created_by.as_str())),
            (card::CREATED_AT, Any::from(c.created_at as f64)),
            (card::UPDATED_AT, Any::from(c.created_at as f64)),
            (card::ACCEPTANCE, Any::from(enc(&c.acceptance))),
            (card::FILES, Any::from(enc(&c.files))),
            (card::DEPENDS_ON, Any::from(enc(&c.depends_on))),
            (card::DRAFT, Any::from(c.draft)),
        ];
        if let Some(s) = &c.size {
            fields.push((card::SIZE, Any::from(s.as_str())));
        }
        if let Some(k) = &c.kind {
            fields.push((card::KIND, Any::from(k.as_str())));
        }
        let prelim: MapPrelim = fields.into_iter().map(|(k, v)| (k.to_string(), yrs::In::Any(v))).collect();
        cards.insert(&mut txn, c.id.as_str(), prelim);
        Ok(())
    }

    fn next_order(&self, column: &str) -> f64 {
        self.cards_in(column).last().map(|c| c.order + 1.0).unwrap_or(1.0)
    }

    fn card_map(&self, id: &str) -> Option<MapRef> {
        let cards = self.cards();
        let txn = self.doc.transact();
        match cards.get(&txn, id) {
            Some(Out::YMap(m)) => Some(m),
            _ => None,
        }
    }

    pub fn set_field(&self, id: &str, field: &str, value: Any, now: i64) -> Result<()> {
        let m = self.card_map(id).ok_or(ProtoError::Malformed("no such card"))?;
        let mut txn = self.doc.transact_mut();
        m.insert(&mut txn, field, value);
        m.insert(&mut txn, card::UPDATED_AT, now as f64);
        Ok(())
    }

    /// Moves a card to `column`, placed after `after` (or at the top if None).
    pub fn move_card(&self, id: &str, column: &str, after: Option<&str>, now: i64) -> Result<()> {
        let siblings: Vec<CardView> = self.cards_in(column).into_iter().filter(|c| c.id != id).collect();
        let order = match after {
            None => siblings.first().map(|c| c.order - 1.0).unwrap_or(1.0),
            Some(a) => {
                let pos = siblings.iter().position(|c| c.id == a);
                match pos {
                    Some(p) => {
                        let lo = siblings[p].order;
                        match siblings.get(p + 1) {
                            Some(n) => (lo + n.order) / 2.0,
                            None => lo + 1.0,
                        }
                    }
                    None => siblings.last().map(|c| c.order + 1.0).unwrap_or(1.0),
                }
            }
        };
        let m = self.card_map(id).ok_or(ProtoError::Malformed("no such card"))?;
        let mut txn = self.doc.transact_mut();
        m.insert(&mut txn, card::COLUMN, column);
        m.insert(&mut txn, card::ORDER, order);
        m.insert(&mut txn, card::UPDATED_AT, now as f64);
        Ok(())
    }

    pub fn remove_card(&self, id: &str) {
        let cards = self.cards();
        let mut txn = self.doc.transact_mut();
        cards.remove(&mut txn, id);
    }

    pub fn card(&self, id: &str) -> Option<CardView> {
        let m = self.card_map(id)?;
        let txn = self.doc.transact();
        Some(read_card(&m, &txn))
    }

    pub fn all_cards(&self) -> Vec<CardView> {
        let cards = self.cards();
        let txn = self.doc.transact();
        let mut out: Vec<CardView> = cards
            .iter(&txn)
            .filter_map(|(_, v)| match v {
                Out::YMap(m) => Some(read_card(&m, &txn)),
                _ => None,
            })
            .collect();
        out.sort_by(|a, b| a.order.total_cmp(&b.order).then_with(|| a.id.cmp(&b.id)));
        out
    }

    pub fn cards_in(&self, column: &str) -> Vec<CardView> {
        self.all_cards().into_iter().filter(|c| c.column == column).collect()
    }

    pub fn to_json(&self) -> serde_json::Value {
        let txn = self.doc.transact();
        let mut s = String::new();
        self.doc.get_or_insert_map(board::CARDS).to_json(&txn).to_json(&mut s);
        serde_json::from_str(&s).unwrap_or(serde_json::Value::Null)
    }
}

fn read_card<T: ReadTxn>(m: &MapRef, txn: &T) -> CardView {
    CardView {
        id: any_str(m.get(txn, card::ID)).unwrap_or_default(),
        title: any_str(m.get(txn, card::TITLE)).unwrap_or_default(),
        description: any_str(m.get(txn, card::DESCRIPTION)).unwrap_or_default(),
        column: any_str(m.get(txn, card::COLUMN)).unwrap_or_default(),
        order: any_f64(m.get(txn, card::ORDER)).unwrap_or(0.0),
        assignee: any_str(m.get(txn, card::ASSIGNEE)),
        created_by: any_str(m.get(txn, card::CREATED_BY)).unwrap_or_default(),
        created_at: any_f64(m.get(txn, card::CREATED_AT)).unwrap_or(0.0) as i64,
        acceptance: json_list(m.get(txn, card::ACCEPTANCE)),
        files: json_list(m.get(txn, card::FILES)),
        depends_on: json_list(m.get(txn, card::DEPENDS_ON)),
        branch: any_str(m.get(txn, card::BRANCH)),
        pr: any_str(m.get(txn, card::PR)),
        thread: any_str(m.get(txn, card::THREAD)),
        size: any_str(m.get(txn, card::SIZE)),
        kind: any_str(m.get(txn, card::KIND)),
        draft: any_bool(m.get(txn, card::DRAFT)),
        handoff: any_str(m.get(txn, card::HANDOFF)),
        plan: any_str(m.get(txn, card::PLAN)),
        suggested_assignee: any_str(m.get(txn, card::SUGGESTED_ASSIGNEE)),
    }
}

/// Activity entry stored in the activity document.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ActivityEntry {
    pub id: String,
    pub at: i64,
    /// e.g. "claim", "approval", "commit", "merge", "review", "overlap", "join", "leave", "card_moved", "agent".
    pub kind: String,
    pub actor: String,
    pub agent: Option<String>,
    /// Structured, translatable summary key plus arguments as JSON.
    pub summary_key: String,
    pub args: serde_json::Value,
    /// Every so many card events: all cards as a JSON array of `CardView`,
    /// where board replay starts (see `polyph_core::activity`).
    pub checkpoint: Option<String>,
}

#[derive(Debug)]
pub struct Activity<'a> {
    pub doc: &'a Doc,
}

impl<'a> Activity<'a> {
    pub fn new(doc: &'a Doc) -> Self {
        Self { doc }
    }

    pub fn append(&self, e: &ActivityEntry) -> Result<()> {
        let arr = self.doc.get_or_insert_array(activity::EVENTS);
        let json = serde_json::to_string(e).map_err(|err| ProtoError::Encode(err.to_string()))?;
        let mut txn = self.doc.transact_mut();
        arr.push_back(&mut txn, json);
        Ok(())
    }

    pub fn entries(&self) -> Vec<ActivityEntry> {
        let arr = self.doc.get_or_insert_array(activity::EVENTS);
        let txn = self.doc.transact();
        let mut v: Vec<ActivityEntry> = arr
            .iter(&txn)
            .filter_map(|o| match o {
                Out::Any(Any::String(s)) => serde_json::from_str(&s).ok(),
                _ => None,
            })
            .collect();
        // Stable: equal times keep the document's order, which every peer shares.
        v.sort_by_key(|e| e.at);
        v
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sync(a: &Doc, b: &Doc) {
        let ua = diff_since(a, &state_vector(b)).unwrap();
        let ub = diff_since(b, &state_vector(a)).unwrap();
        apply_update(b, &ua).unwrap();
        apply_update(a, &ub).unwrap();
    }

    fn new_card(id: &str) -> NewCard {
        NewCard { id: id.into(), title: format!("Card {id}"), column: "backlog".into(), ..Default::default() }
    }

    #[test]
    fn concurrent_moves_converge() {
        let docs: Vec<Doc> = (1..=3).map(Doc::with_client_id).collect();
        Board::new(&docs[0]).init_columns(&board::DEFAULT_COLUMNS);
        Board::new(&docs[0]).add_card(&new_card("c1")).unwrap();
        Board::new(&docs[0]).add_card(&new_card("c2")).unwrap();
        sync(&docs[0], &docs[1]);
        sync(&docs[0], &docs[2]);

        Board::new(&docs[0]).move_card("c1", "ready", None, 10).unwrap();
        Board::new(&docs[1]).move_card("c1", "review", None, 11).unwrap();
        Board::new(&docs[2]).move_card("c1", "done", None, 12).unwrap();
        Board::new(&docs[2]).move_card("c2", "in_progress", None, 12).unwrap();

        sync(&docs[0], &docs[1]);
        sync(&docs[1], &docs[2]);
        sync(&docs[0], &docs[2]);
        sync(&docs[0], &docs[1]);

        let views: Vec<Vec<CardView>> = docs.iter().map(|d| Board::new(d).all_cards()).collect();
        assert_eq!(views[0], views[1]);
        assert_eq!(views[1], views[2]);
        assert_eq!(Board::new(&docs[0]).card("c2").unwrap().column, "in_progress");
        assert_eq!(Board::new(&docs[0]).columns().len(), 6);
    }

    #[test]
    fn ordering_within_column() {
        let d = Doc::new();
        let b = Board::new(&d);
        for id in ["a", "b", "c"] {
            b.add_card(&new_card(id)).unwrap();
        }
        b.move_card("c", "backlog", Some("a"), 1).unwrap();
        let ids: Vec<String> = b.cards_in("backlog").into_iter().map(|c| c.id).collect();
        assert_eq!(ids, vec!["a", "c", "b"]);
        b.move_card("b", "backlog", None, 2).unwrap();
        let ids: Vec<String> = b.cards_in("backlog").into_iter().map(|c| c.id).collect();
        assert_eq!(ids, vec!["b", "a", "c"]);
    }

    #[test]
    fn activity_appends_merge() {
        let a = Doc::with_client_id(1);
        let b = Doc::with_client_id(2);
        let mk = |id: &str, at| ActivityEntry {
            id: id.into(),
            at,
            kind: "join".into(),
            actor: "m".into(),
            agent: None,
            summary_key: "activity-join".into(),
            args: serde_json::json!({}),
            checkpoint: None,
        };
        Activity::new(&a).append(&mk("1", 5)).unwrap();
        Activity::new(&b).append(&mk("2", 3)).unwrap();
        sync(&a, &b);
        let ids: Vec<String> = Activity::new(&a).entries().into_iter().map(|e| e.id).collect();
        assert_eq!(ids, vec!["2", "1"]);
        assert_eq!(Activity::new(&b).entries().len(), 2);
    }
}
