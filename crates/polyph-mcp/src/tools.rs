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
pub const TASKS: &str = "list_tasks";
pub const CREATE_TASK: &str = "create_task";
pub const MOVE_TASK: &str = "move_task";
pub const FINISH: &str = "finish_task";
pub const WHITEBOARDS: &str = "list_whiteboards";
pub const READ_WB: &str = "read_whiteboard";
pub const DRAW: &str = "draw_diagram";
pub const SKILLS: &str = "list_skills";
pub const READ_SKILL: &str = "read_skill";

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
    ToolDef {
        name: TASKS,
        description: "List the room's task cards: id, title, column, assignee, dependencies and predicted files. \
Your own task is marked. Without `column`, every column except done.",
        schema: || {
            json!({
                "type": "object",
                "properties": { "column": { "enum": ["backlog", "ready", "claimed", "in_progress", "review", "done"] } },
                "additionalProperties": false
            })
        },
    },
    ToolDef {
        name: CREATE_TASK,
        description: "Add a task card to the backlog, for work you found that is outside your own task (a bug, a \
follow-up). A person or the planner decides who does it.",
        schema: || {
            json!({
                "type": "object",
                "properties": {
                    "title": { "type": "string" },
                    "description": { "type": "string" },
                    "acceptance": { "type": "array", "items": { "type": "string" } },
                    "files": { "type": "array", "items": { "type": "string" } }
                },
                "required": ["title"],
                "additionalProperties": false
            })
        },
    },
    ToolDef {
        name: MOVE_TASK,
        description: "Move a task card that is yours or unassigned to another column (backlog, ready, claimed, \
in_progress). Use finish_task, not this, when your work is done; only merging moves a card to done.",
        schema: || {
            json!({
                "type": "object",
                "properties": {
                    "id": { "type": "string" },
                    "column": { "enum": ["backlog", "ready", "claimed", "in_progress"] }
                },
                "required": ["id", "column"],
                "additionalProperties": false
            })
        },
    },
    ToolDef {
        name: FINISH,
        description: "Your task is done: Polyph commits your work on the task branch, releases your claims and \
queues the branch for review and merging. Call it once, at the end, with a one-line summary for the commit.",
        schema: || {
            json!({
                "type": "object",
                "properties": { "summary": { "type": "string" } },
                "required": ["summary"],
                "additionalProperties": false
            })
        },
    },
    ToolDef { name: WHITEBOARDS, description: "List the room's whiteboards (id and name).", schema: no_args },
    ToolDef {
        name: READ_WB,
        description: "Read a whiteboard as text: every shape with its label and position, and which labelled \
shapes the arrows connect. Use it to follow designs people sketched.",
        schema: || {
            json!({
                "type": "object",
                "properties": { "board": { "type": "string", "description": "Whiteboard id or name." } },
                "required": ["board"],
                "additionalProperties": false
            })
        },
    },
    ToolDef {
        name: DRAW,
        description: "Draw a diagram of labelled boxes and arrows on a whiteboard, for people to read: an \
architecture, a data flow, a plan. Polyph lays it out. Drawing again with the same `diagram` id replaces that \
diagram. If no whiteboard has the given name, one is created.",
        schema: || {
            json!({
                "type": "object",
                "properties": {
                    "board": { "type": "string", "description": "Whiteboard id or name." },
                    "diagram": { "type": "string", "description": "A short id for this diagram, e.g. \"auth-flow\"." },
                    "nodes": { "type": "array", "maxItems": 60, "items": {
                        "type": "object",
                        "properties": { "id": { "type": "string" }, "label": { "type": "string" } },
                        "required": ["id", "label"]
                    } },
                    "edges": { "type": "array", "maxItems": 120, "items": {
                        "type": "object",
                        "properties": { "from": { "type": "string" }, "to": { "type": "string" }, "label": { "type": "string" } },
                        "required": ["from", "to"]
                    } }
                },
                "required": ["board", "diagram", "nodes"],
                "additionalProperties": false
            })
        },
    },
    ToolDef {
        name: SKILLS,
        description: "List the team's skills: written instructions for how this team does a kind of task (releases, \
reviews, migrations, house style). Project skills come with the repository; personal ones are your owner's. Read the \
ones that match your task with read_skill before you start, and follow them.",
        schema: no_args,
    },
    ToolDef {
        name: READ_SKILL,
        description: "Read one skill by name (from list_skills). Follow its steps unless your owner says otherwise.",
        schema: || {
            json!({
                "type": "object",
                "properties": { "name": { "type": "string" } },
                "required": ["name"],
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
