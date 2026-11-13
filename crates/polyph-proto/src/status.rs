//! The status feed: newline-delimited JSON for the status pill, Nublet,
//! scripts and the D-Bus service. It carries states, names, counts and
//! colors only, never code or chat content.

use serde::{Deserialize, Serialize};

use crate::messages::AgentState;

pub const STATUS_FEED_VERSION: u32 = 1;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct StatusAgent {
    pub id: String,
    pub label: String,
    pub owner: String,
    /// CSS color for the agent's voice, e.g. `oklch(0.72 0.12 250)`.
    pub color: String,
    pub state: AgentState,
    pub task: Option<String>,
    /// Model the router set on this agent, when one is known.
    #[serde(default)]
    pub model: Option<String>,
    pub mine: bool,
    pub cloud: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct StatusApproval {
    pub id: String,
    pub agent: String,
    /// One-line description, e.g. "Run cargo test" or "Connect to crates.io:443".
    pub title: String,
    pub mine: bool,
    pub created_at: i64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct StatusBudget {
    pub scope: String,
    pub used: f64,
    pub limit: f64,
    pub unit: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct StatusSnapshot {
    pub room: Option<String>,
    pub room_name: Option<String>,
    pub connected: bool,
    pub members_online: u32,
    pub agents: Vec<StatusAgent>,
    pub approvals: Vec<StatusApproval>,
    pub budgets: Vec<StatusBudget>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum StatusEventKind {
    Snapshot(StatusSnapshot),
    AgentStarted { agent: StatusAgent },
    AgentState { id: String, state: AgentState, task: Option<String> },
    AgentStopped { id: String },
    ApprovalPending { approval: StatusApproval },
    ApprovalResolved { id: String, approved: bool },
    ClaimsChanged { count: u32 },
    TaskMoved { task: String, title: String, column: String },
    BudgetWarning { budget: StatusBudget, percent: u32 },
    Connection { connected: bool },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct StatusEvent {
    pub v: u32,
    pub seq: u64,
    pub at: i64,
    #[serde(flatten)]
    pub kind: StatusEventKind,
}

impl StatusEvent {
    pub fn new(seq: u64, at: i64, kind: StatusEventKind) -> Self {
        Self { v: STATUS_FEED_VERSION, seq, at, kind }
    }

    pub fn to_json_line(&self) -> String {
        serde_json::to_string(self).unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_shape_is_stable() {
        let e = StatusEvent::new(1, 2, StatusEventKind::AgentStopped { id: "a".into() });
        assert_eq!(e.to_json_line(), r#"{"v":1,"seq":1,"at":2,"type":"agent_stopped","id":"a"}"#);
        let back: StatusEvent = serde_json::from_str(&e.to_json_line()).unwrap();
        assert_eq!(back, e);
    }
}
