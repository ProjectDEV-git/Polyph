//! Helpers over the shared documents that are not board-specific: chat and
//! threads live in the `Threads` document.

use serde::{Deserialize, Serialize};
use yrs::{Any, Array, Doc, Map, Out, Transact};

pub const CHAT: &str = "chat";

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChatEntry {
    pub id: String,
    /// Member id (hex) of the author, or of the agent's owner.
    pub author: String,
    /// Agent label when an agent wrote it.
    pub agent: Option<String>,
    pub thread: Option<String>,
    pub text: String,
    pub at: i64,
}

pub fn append_chat(doc: &Doc, e: &ChatEntry) {
    let arr = doc.get_or_insert_array(CHAT);
    let json = serde_json::to_string(e).unwrap_or_default();
    let mut txn = doc.transact_mut();
    arr.push_back(&mut txn, json);
}

pub fn chat(doc: &Doc) -> Vec<ChatEntry> {
    let arr = doc.get_or_insert_array(CHAT);
    let txn = doc.transact();
    let mut v: Vec<ChatEntry> = arr
        .iter(&txn)
        .filter_map(|o| match o {
            Out::Any(Any::String(s)) => serde_json::from_str(&s).ok(),
            _ => None,
        })
        .collect();
    v.sort_by(|a, b| a.at.cmp(&b.at).then_with(|| a.id.cmp(&b.id)));
    v
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ThreadInfo {
    pub id: String,
    pub title: String,
    pub card: Option<String>,
    pub created_by: String,
    pub created_at: i64,
}

pub fn create_thread(doc: &Doc, t: &ThreadInfo) {
    let threads = doc.get_or_insert_map(polyph_proto::crdt::threads::THREADS);
    let json = serde_json::to_string(t).unwrap_or_default();
    let mut txn = doc.transact_mut();
    threads.insert(&mut txn, t.id.as_str(), json);
}

pub fn threads(doc: &Doc) -> Vec<ThreadInfo> {
    let threads = doc.get_or_insert_map(polyph_proto::crdt::threads::THREADS);
    let txn = doc.transact();
    let mut v: Vec<ThreadInfo> = threads
        .iter(&txn)
        .filter_map(|(_, o)| match o {
            Out::Any(Any::String(s)) => serde_json::from_str(&s).ok(),
            _ => None,
        })
        .collect();
    v.sort_by_key(|t| t.created_at);
    v
}

/// Room settings stored in the `Settings` document as JSON values.
pub fn set_setting(doc: &Doc, key: &str, value: &serde_json::Value) {
    let m = doc.get_or_insert_map("settings");
    let mut txn = doc.transact_mut();
    m.insert(&mut txn, key, value.to_string());
}

pub fn setting(doc: &Doc, key: &str) -> Option<serde_json::Value> {
    let m = doc.get_or_insert_map("settings");
    let txn = doc.transact();
    match m.get(&txn, key) {
        Some(Out::Any(Any::String(s))) => serde_json::from_str(&s).ok(),
        _ => None,
    }
}
