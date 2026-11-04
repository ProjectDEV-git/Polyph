//! The tools Polyph gives agents. Descriptions are written for the model.

use serde_json::{Value, json};

pub struct ToolDef {
    pub name: &'static str,
    pub description: &'static str,
    pub schema: fn() -> Value,
}

impl std::fmt::Debug for ToolDef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.name)
    }
}

pub const CLAIM: &str = "claim_files";
pub const RELEASE: &str = "release_files";
pub const CLAIMS: &str = "list_claims";
pub const TEAM: &str = "team_state";
pub const ASK: &str = "ask_agent";
pub const REPLY: &str = "reply";
pub const INBOX: &str = "read_inbox";
pub const REMEMBER: &str = "remember";
pub const RECALL: &str = "recall";
pub const FORGET: &str = "forget";

fn no_args() -> Value {
    json!({ "type": "object", "properties": {}, "additionalProperties": false })
}

pub static TOOLS: &[ToolDef] = &[
    ToolDef {
        name: CLAIM,
        description: "Claim files before editing them so other agents in the room do not edit them at the same time. \
Each target is a repository-relative file (`src/auth.rs`), a directory ending in `/` (`src/billing/`), or a symbol \
(`src/auth.rs#login`). Claims are leases: re-claiming renews them. If a target is held by another agent, the result \
names the holder; use ask_agent to coordinate instead of working around it. Edits to unclaimed files may be claimed \
for you automatically, but claiming up front avoids surprises.",
        schema: || {
            json!({
                "type": "object",
                "properties": {
                    "paths": { "type": "array", "items": { "type": "string" }, "minItems": 1, "maxItems": 100 },
                    "reason": { "type": "string", "description": "What you are about to do, in a few words." }
                },
                "required": ["paths", "reason"],
                "additionalProperties": false
            })
        },
    },
    ToolDef {
        name: RELEASE,
        description: "Release claims you hold, so others can edit those files. Without `paths`, releases all of yours. \
Release files as soon as you are done with them.",
        schema: || {
            json!({
                "type": "object",
                "properties": { "paths": { "type": "array", "items": { "type": "string" } } },
                "additionalProperties": false
            })
        },
    },
    ToolDef {
        name: CLAIMS,
        description: "List every live claim in the room: target, holder, reason and seconds left.",
        schema: no_args,
    },
    ToolDef {
        name: TEAM,
        description: "A short digest of the room: who is online, every agent with its state and what it holds, and \
whether anything is waiting for approval. Call this before starting work and whenever you are blocked.",
        schema: no_args,
    },
    ToolDef {
        name: ASK,
        description: "Send a message to another agent in the room (by label, such as `Bo/claude`, or by the owner's \
name when they run one agent). Use it to ask for a file someone holds, to hand over context, or to agree who does \
what. With `wait_seconds`, waits that long for the reply; otherwise the reply arrives in your inbox.",
        schema: || {
            json!({
                "type": "object",
                "properties": {
                    "to": { "type": "string" },
                    "message": { "type": "string" },
                    "wait_seconds": { "type": "integer", "minimum": 0, "maximum": 300 }
                },
                "required": ["to", "message"],
                "additionalProperties": false
            })
        },
    },
    ToolDef {
        name: REPLY,
        description: "Reply to a message in your inbox, by its id.",
        schema: || {
            json!({
                "type": "object",
                "properties": { "message_id": { "type": "string" }, "text": { "type": "string" } },
                "required": ["message_id", "text"],
                "additionalProperties": false
            })
        },
    },
    ToolDef {
        name: INBOX,
        description: "Read the messages sent to you by people and other agents since you last checked.",
        schema: no_args,
    },
    ToolDef {
        name: REMEMBER,
        description: "Pin a note to a file, a directory (ending in `/`), or a line of a file, for every agent and \
person in the room: a decision, a pitfall, an invariant. Notes follow their line when the file changes. Keep them \
short and factual.",
        schema: || {
            json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string" },
                    "line": { "type": "integer", "minimum": 1 },
                    "note": { "type": "string" }
                },
                "required": ["path", "note"],
                "additionalProperties": false
            })
        },
    },
    ToolDef {
        name: RECALL,
        description: "Read the notes pinned to a file or directory (and to the directories containing it), with \
their current line. Without `path`, every note in the room. Read notes before editing a file.",
        schema: || {
            json!({
                "type": "object",
                "properties": { "path": { "type": "string" } },
                "additionalProperties": false
            })
        },
    },
    ToolDef {
        name: FORGET,
        description: "Delete a note that is wrong or no longer true, by its id.",
        schema: || {
            json!({
                "type": "object",
                "properties": { "id": { "type": "string" } },
                "required": ["id"],
                "additionalProperties": false
            })
        },
    },
];

pub fn find(name: &str) -> Option<&'static ToolDef> {
    TOOLS.iter().find(|t| t.name == name)
}

/// The `tools/list` result.
pub fn list() -> Value {
    let tools: Vec<Value> = TOOLS
        .iter()
        .map(|t| json!({ "name": t.name, "description": t.description, "inputSchema": (t.schema)() }))
        .collect();
    json!({ "tools": tools })
}
