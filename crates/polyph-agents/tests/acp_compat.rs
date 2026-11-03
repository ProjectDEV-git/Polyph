//! Wire compatibility with the official ACP schema crate: messages Polyph
//! sends must parse as the official types, and messages agents send (built
//! from the official types' JSON shape) must parse as Polyph's.
#![allow(clippy::unwrap_used)]

use agent_client_protocol_schema::v1 as acp;
use polyph_agents::wire::*;
use serde_json::{Value, json};

fn roundtrip<Ours: serde::Serialize, Theirs: serde::de::DeserializeOwned>(v: &Ours) -> Theirs {
    let j = serde_json::to_value(v).unwrap();
    serde_json::from_value(j.clone()).unwrap_or_else(|e| panic!("official type rejected {j}: {e}"))
}

#[test]
fn client_requests_parse_as_official_types() {
    let init = InitializeRequest {
        protocol_version: PROTOCOL_VERSION,
        client_capabilities: ClientCapabilities {
            fs: FsCapabilities { read_text_file: true, write_text_file: true },
            terminal: true,
        },
        client_info: Some(Implementation { name: "polyph".into(), version: "0.1.0".into() }),
    };
    let _: acp::InitializeRequest = roundtrip(&init);
    let new = NewSessionRequest {
        cwd: "/work".into(),
        mcp_servers: vec![McpServerStdio {
            name: "polyph".into(),
            command: "polyph".into(),
            args: vec!["mcp".into(), "serve".into()],
            env: vec![EnvVariable { name: "A".into(), value: "b".into() }],
        }],
    };
    let _: acp::NewSessionRequest = roundtrip(&new);
    let prompt = PromptRequest { session_id: "s1".into(), prompt: vec![ContentBlock::Text { text: "hi".into() }] };
    let _: acp::PromptRequest = roundtrip(&prompt);
    let resp = RequestPermissionResponse { outcome: PermissionOutcome::Selected { option_id: "allow".into() } };
    let _: acp::RequestPermissionResponse = roundtrip(&resp);
    let cancelled = RequestPermissionResponse { outcome: PermissionOutcome::Cancelled };
    let _: acp::RequestPermissionResponse = roundtrip(&cancelled);
}

#[test]
fn agent_messages_parse_as_ours() {
    let updates: Vec<Value> = vec![
        json!({ "sessionId": "s1", "update": { "sessionUpdate": "agent_message_chunk", "content": { "type": "text", "text": "Hello" } } }),
        json!({ "sessionId": "s1", "update": { "sessionUpdate": "agent_thought_chunk", "content": { "type": "text", "text": "Thinking" } } }),
        json!({ "sessionId": "s1", "update": { "sessionUpdate": "tool_call", "toolCallId": "t1", "title": "Read src/a.rs", "kind": "read", "status": "pending", "locations": [{ "path": "/w/src/a.rs", "line": 4 }], "rawInput": { "file_path": "/w/src/a.rs" } } }),
        json!({ "sessionId": "s1", "update": { "sessionUpdate": "tool_call_update", "toolCallId": "t1", "status": "completed", "content": [{ "type": "diff", "path": "/w/src/a.rs", "oldText": "a", "newText": "b" }, { "type": "terminal", "terminalId": "x" }, { "type": "content", "content": { "type": "text", "text": "ok" } }] } }),
        json!({ "sessionId": "s1", "update": { "sessionUpdate": "plan", "entries": [{ "content": "step", "priority": "high", "status": "pending" }] } }),
        json!({ "sessionId": "s1", "update": { "sessionUpdate": "usage_update", "used": 100, "size": 200000 } }),
    ];
    for u in updates {
        // Valid for the official schema...
        let _: acp::SessionNotification =
            serde_json::from_value(u.clone()).unwrap_or_else(|e| panic!("official rejected fixture {u}: {e}"));
        // ...and for ours.
        let _: SessionNotification =
            serde_json::from_value(u.clone()).unwrap_or_else(|e| panic!("polyph rejected {u}: {e}"));
    }
    let perm = json!({
        "sessionId": "s1",
        "toolCall": { "toolCallId": "t2", "title": "Run cargo test", "kind": "execute", "rawInput": { "command": "cargo test" } },
        "options": [{ "optionId": "a", "name": "Allow", "kind": "allow_once" }, { "optionId": "r", "name": "Reject", "kind": "reject_once" }]
    });
    let _: acp::RequestPermissionRequest = serde_json::from_value(perm.clone()).unwrap();
    let ours: RequestPermissionRequest = serde_json::from_value(perm).unwrap();
    assert_eq!(ours.options.len(), 2);
    let pr = json!({ "stopReason": "end_turn" });
    let _: acp::PromptResponse = serde_json::from_value(pr.clone()).unwrap();
    let _: PromptResponse = serde_json::from_value(pr).unwrap();
    let write = json!({ "sessionId": "s1", "path": "/w/a.rs", "content": "x" });
    let _: acp::WriteTextFileRequest = serde_json::from_value(write.clone()).unwrap();
    let _: WriteTextFileRequest = serde_json::from_value(write).unwrap();
    let term = json!({ "sessionId": "s1", "command": "cargo", "args": ["test"], "cwd": "/w", "outputByteLimit": 1000 });
    let _: acp::CreateTerminalRequest = serde_json::from_value(term.clone()).unwrap();
    let _: CreateTerminalRequest = serde_json::from_value(term).unwrap();
}
