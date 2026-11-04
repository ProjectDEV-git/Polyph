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
    // Stable: messages in the same second keep the document's order, which
    // every member shares.
    v.sort_by_key(|e| e.at);
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

/// Settings whose key starts with `prefix`, as (key, value).
pub fn settings_with_prefix(doc: &Doc, prefix: &str) -> Vec<(String, serde_json::Value)> {
    let m = doc.get_or_insert_map("settings");
    let txn = doc.transact();
    let mut v: Vec<(String, serde_json::Value)> = m
        .iter(&txn)
        .filter(|(k, _)| k.starts_with(prefix))
        .filter_map(|(k, o)| match o {
            Out::Any(Any::String(s)) => serde_json::from_str(&s).ok().map(|x| (k.to_string(), x)),
            _ => None,
        })
        .collect();
    v.sort_by(|a, b| a.0.cmp(&b.0));
    v
}

pub const USAGE: &str = "usage";

/// One agent turn's usage, shared through the Activity document so every
/// member computes the same ledger. `usage` is None when the agent reported nothing.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct UsageRecord {
    pub id: String,
    pub member: String,
    pub agent: String,
    pub task: Option<String>,
    pub usage: Option<polyph_proto::messages::Usage>,
    pub at: i64,
}

pub fn append_usage(doc: &Doc, r: &UsageRecord) {
    let arr = doc.get_or_insert_array(USAGE);
    let json = serde_json::to_string(r).unwrap_or_default();
    let mut txn = doc.transact_mut();
    arr.push_back(&mut txn, json);
}

pub fn usage_records(doc: &Doc) -> Vec<UsageRecord> {
    let arr = doc.get_or_insert_array(USAGE);
    let txn = doc.transact();
    arr.iter(&txn)
        .filter_map(|o| match o {
            Out::Any(Any::String(s)) => serde_json::from_str(&s).ok(),
            _ => None,
        })
        .collect()
}

/// Board document: `merge_queue` Y.Map entry id -> JSON [`MergeEntry`].
pub const MERGE_QUEUE: &str = "merge_queue";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MergeState {
    Queued,
    Reviewing,
    ChangesRequested,
    AwaitingApproval,
    Merging,
    Resolving,
    Checking,
    /// Pushed and waiting on a pull request (`[merge] via = "github"`).
    PullRequest,
    Merged,
    Failed,
}

impl MergeState {
    pub fn finished(self) -> bool {
        matches!(self, MergeState::Merged | MergeState::Failed | MergeState::ChangesRequested)
    }
}

/// One finished task waiting to merge. Only the device holding the task's
/// worktree (`device`) works on an entry; everyone else watches.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MergeEntry {
    pub id: String,
    /// Board card id, if the task has one.
    pub card: Option<String>,
    pub branch: String,
    pub into: String,
    /// The commit that was queued.
    pub head: String,
    pub device: String,
    pub member: String,
    pub agent: Option<String>,
    pub state: MergeState,
    /// Why it failed, review comments, or how conflicts were resolved.
    pub detail: Option<String>,
    /// Files whose conflicts Polyph resolved, and how.
    #[serde(default)]
    pub resolved: Vec<(String, String)>,
    pub merge_commit: Option<String>,
    /// The pull request, when the task merges through GitHub.
    #[serde(default)]
    pub pr: Option<String>,
    #[serde(default)]
    pub pr_number: Option<u64>,
    /// `owner/name` the pull request is in.
    #[serde(default)]
    pub pr_repo: Option<String>,
    pub queued_at: i64,
    pub updated_at: i64,
}

pub fn put_merge_entry(doc: &Doc, e: &MergeEntry) {
    let m = doc.get_or_insert_map(MERGE_QUEUE);
    let json = serde_json::to_string(e).unwrap_or_default();
    let mut txn = doc.transact_mut();
    m.insert(&mut txn, e.id.as_str(), json);
}

/// Entries in queue order.
pub fn merge_entries(doc: &Doc) -> Vec<MergeEntry> {
    let m = doc.get_or_insert_map(MERGE_QUEUE);
    let txn = doc.transact();
    let mut v: Vec<MergeEntry> = m
        .iter(&txn)
        .filter_map(|(_, o)| match o {
            Out::Any(Any::String(s)) => serde_json::from_str(&s).ok(),
            _ => None,
        })
        .collect();
    v.sort_by(|a, b| a.queued_at.cmp(&b.queued_at).then_with(|| a.id.cmp(&b.id)));
    v
}
