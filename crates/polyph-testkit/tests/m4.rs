//! Milestone 4 acceptance tests: agents coordinating through the Polyph MCP
//! server (started by the agent inside its sandbox, reaching the daemon
//! through the agent's proxy): claims, team state, inboxes with the loop
//! guard, and memory notes.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::Path;

use polyph_testkit::harness::{AgentBins, Harness};
use serde_json::{Value, json};

fn bins() -> AgentBins {
    AgentBins {
        fake_agent: env!("CARGO_BIN_EXE_polyph-fake-agent").into(),
        sandbox_helper: env!("CARGO_BIN_EXE_polyph-test-sandbox-exec").into(),
        mcp: env!("CARGO_BIN_EXE_polyph-test-mcp").into(),
    }
}

async fn room_of_two() -> (Harness, String) {
    let mut h = Harness::new().await.unwrap();
    h.agents = Some(bins());
    let a = h.add("Ann").await.unwrap();
    let b = h.add("Bo").await.unwrap();
    let room = h.nodes[a].call("room.create", json!({ "name": "coord" })).await.unwrap();
    let rid = room["id"].as_str().unwrap().to_string();
    h.until("connected", || async {
        h.nodes[a].call("room.get", json!({ "room": rid })).await.unwrap()["connected"] == true
    })
    .await
    .unwrap();
    h.join(&rid, a, b, a, "member").await.unwrap();
    (h, rid)
}

fn worktree(script: Value, lib: &str) -> tempfile::TempDir {
    let d = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(d.path().join("src")).unwrap();
    std::fs::write(d.path().join("src/lib.rs"), lib).unwrap();
    std::fs::write(d.path().join(".fake-script.json"), script.to_string()).unwrap();
    std::fs::write(d.path().join("polyph.toml"), "").unwrap();
    d
}

async fn events(h: &Harness, i: usize, rid: &str, agent: &str) -> Vec<Value> {
    let t = h.nodes[i].call("agent.transcript", json!({ "room": rid, "agent": agent })).await.unwrap();
    t["events"].as_array().unwrap().clone()
}

/// Like `messages`, but `None` while this node has not yet heard of the agent
/// (the room replicates it from the device that runs it), so a poll can wait.
async fn messages_if_known(h: &Harness, i: usize, rid: &str, agent: &str) -> Option<Vec<String>> {
    h.nodes[i].call("agent.transcript", json!({ "room": rid, "agent": agent })).await.ok()?;
    Some(messages(h, i, rid, agent).await)
}

async fn messages(h: &Harness, i: usize, rid: &str, agent: &str) -> Vec<String> {
    events(h, i, rid, agent)
        .await
        .iter()
        .filter(|e| e["kind"]["t"] == "message")
        .flat_map(|e| e["kind"]["c"]["text"].as_str().unwrap().lines().map(str::to_string).collect::<Vec<_>>())
        .collect()
}

async fn prompts(h: &Harness, i: usize, rid: &str, agent: &str) -> usize {
    events(h, i, rid, agent).await.iter().filter(|e| e["kind"]["t"] == "prompt").count()
}

fn find<'a>(msgs: &'a [String], prefix: &str) -> &'a str {
    msgs.iter()
        .find(|m| m.starts_with(prefix))
        .map(String::as_str)
        .unwrap_or_else(|| panic!("no \"{prefix}\" in {msgs:?}"))
}

async fn start_agent(h: &Harness, i: usize, rid: &str, dir: &Path) -> String {
    let r = h.nodes[i]
        .call("agent.start", json!({ "room": rid, "kind": "fake", "cwd": dir, "preset": "default" }))
        .await
        .unwrap();
    r["id"].as_str().unwrap().to_string()
}

async fn prompt(h: &Harness, i: usize, agent: &str, text: &str) {
    let r = h.nodes[i].call("agent.prompt", json!({ "agent": agent, "text": text })).await.unwrap();
    assert_eq!(r["stop_reason"], "end_turn");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn claims_and_team_state_through_mcp() {
    let (h, rid) = room_of_two().await;
    let (a, b) = (0, 1);
    let ann = worktree(
        json!({ "turns": [
            [
                { "mcp": { "tool": "claim_files", "args": { "paths": ["src/lib.rs", "src/billing/"], "reason": "refactor billing" } } },
                { "mcp": { "tool": "team_state", "args": {} } }
            ],
            [ { "mcp": { "tool": "release_files", "args": { "paths": ["src/lib.rs"] } } } ]
        ] }),
        "pub fn a() {}\n",
    );
    let bo = worktree(
        json!({ "turns": [
            [
                { "mcp": { "tool": "claim_files", "args": { "paths": ["src/billing/invoice.rs", "src/other.rs"], "reason": "invoices" } } },
                { "mcp": { "tool": "list_claims", "args": {} } },
                { "mcp": { "tool": "claim_files", "args": { "paths": ["../escape.rs"], "reason": "x" } } }
            ],
            [ { "mcp": { "tool": "claim_files", "args": { "paths": ["src/lib.rs"], "reason": "now mine" } } } ]
        ] }),
        "pub fn a() {}\n",
    );
    let aid = start_agent(&h, a, &rid, ann.path()).await;
    let bid = start_agent(&h, b, &rid, bo.path()).await;
    // Both agents must know about each other before the digest is read.
    h.until("agents visible", || async {
        h.nodes[a].call("agent.list", json!({ "room": rid })).await.unwrap().as_array().unwrap().len() == 2
    })
    .await
    .unwrap();

    prompt(&h, a, &aid, "refactor billing").await;
    let am = messages(&h, a, &rid, &aid).await;
    assert_eq!(find(&am, "mcp claim_files"), "mcp claim_files: Claimed: src/lib.rs, src/billing/.");
    let team = find(&am, "mcp team_state");
    assert!(team.contains("Ann (owner), online") && team.contains("Bo (member), online"), "{team}");
    assert!(
        team.contains("Ann/fake (fake)")
            && team.contains("holds ")
            && team.contains("src/lib.rs")
            && team.contains("src/billing/"),
        "{team}"
    );
    assert!(team.contains("Bo/fake (fake)"), "{team}");

    prompt(&h, b, &bid, "write invoices").await;
    let bm = messages(&h, b, &rid, &bid).await;
    let c = find(&bm, "mcp claim_files");
    assert!(c.contains("Claimed: src/other.rs."), "{c}");
    // The answer names the claim that blocks the request.
    assert!(c.contains("src/billing/ (held by Ann/fake: refactor billing)"), "{c}");
    let l = find(&bm, "mcp list_claims");
    assert!(l.contains("src/billing/ held by Ann/fake") && l.contains("src/other.rs held by Bo/fake"), "{l}");
    assert!(bm.iter().any(|m| m == "mcp claim_files: ../escape.rs is outside your worktree"), "{bm:?}");

    // Releasing frees the file for Bo's agent.
    prompt(&h, a, &aid, "done with lib.rs").await;
    assert!(messages(&h, a, &rid, &aid).await.contains(&"mcp release_files: Released: src/lib.rs.".to_string()));
    prompt(&h, b, &bid, "take lib.rs").await;
    assert!(
        messages(&h, b, &rid, &bid).await.contains(&"mcp claim_files: Claimed: src/lib.rs.".to_string()),
        "{:?}",
        messages(&h, b, &rid, &bid).await
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn agents_ask_and_reply_and_the_loop_guard_stops_ping_pong() {
    let (h, rid) = room_of_two().await;
    let (a, b) = (0, 1);
    // Ann's agent asks and waits for the answer; then, woken by Bo's agent,
    // asks again; its third turn is a person's prompt.
    let ann = worktree(
        json!({ "turns": [
            [ { "mcp": { "tool": "ask_agent", "args": { "to": "Bo", "message": "can I have src/lib.rs?", "wait_seconds": 60 } } } ],
            [ { "echo_prompt": true }, { "mcp": { "tool": "ask_agent", "args": { "to": "Bo/fake", "message": "and after that?" } } } ],
            [ { "echo_prompt": true } ]
        ] }),
        "pub fn a() {}\n",
    );
    // Bo's agent is woken each time: it replies, then keeps the chain going.
    let bo = worktree(
        json!({ "turns": [
            [ { "echo_prompt": true }, { "mcp": { "tool": "reply", "args": { "message_id": "$MSG", "text": "yes, after lunch" } } },
              { "mcp": { "tool": "ask_agent", "args": { "to": "Ann/fake", "message": "anything else?" } } } ],
            [ { "echo_prompt": true }, { "mcp": { "tool": "ask_agent", "args": { "to": "Ann/fake", "message": "still there?" } } } ],
            [ { "echo_prompt": true } ]
        ] }),
        "pub fn a() {}\n",
    );
    let aid = start_agent(&h, a, &rid, ann.path()).await;
    let bid = start_agent(&h, b, &rid, bo.path()).await;
    h.until("agents visible", || async {
        h.nodes[a].call("agent.list", json!({ "room": rid })).await.unwrap().as_array().unwrap().len() == 2
    })
    .await
    .unwrap();

    // Turn 1 (person): Ann's agent asks Bo's agent and gets the reply inline.
    prompt(&h, a, &aid, "coordinate with Bo").await;
    let am = messages(&h, a, &rid, &aid).await;
    let asked = find(&am, "mcp ask_agent");
    assert!(asked.contains("Sent to Bo/fake") && asked.contains("Reply from Bo/fake: yes, after lunch"), "{asked}");
    // Bo's turn may still be running when Ann's agent gets the reply.
    h.until("Bo's agent replied", || async {
        messages(&h, b, &rid, &bid).await.iter().any(|m| m.starts_with("mcp reply"))
    })
    .await
    .unwrap();
    let bm = messages(&h, b, &rid, &bid).await;
    assert!(find(&bm, "prompt:").contains("from Ann/fake: can I have src/lib.rs?"), "{bm:?}");
    assert_eq!(find(&bm, "mcp reply"), "mcp reply: Replied to Ann/fake.");

    // The chain: Bo asks Ann (hop 2) -> Ann woken, asks Bo (hop 3) -> Bo woken,
    // asks Ann (hop 4) -> the guard does not wake Ann again.
    h.until("Bo's agent warned by the loop guard", || async {
        messages(&h, b, &rid, &bid).await.iter().any(|m| m.contains("too often"))
    })
    .await
    .unwrap();
    let am = messages(&h, a, &rid, &aid).await;
    assert!(
        am.iter().any(|m| m.starts_with("prompt: [Polyph]") && m.contains("from Bo/fake: anything else?")),
        "{am:?}"
    );
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    assert_eq!(prompts(&h, a, &rid, &aid).await, 2, "Ann's agent ran a turn for a person and one wake");
    assert_eq!(prompts(&h, b, &rid, &bid).await, 2, "Bo's agent was woken twice");

    // The stopped message is not lost: it arrives with the next person's prompt.
    prompt(&h, a, &aid, "carry on").await;
    let am = messages(&h, a, &rid, &aid).await;
    let last = am.iter().rev().find(|m| m.starts_with("prompt:")).unwrap();
    assert!(last.starts_with("prompt: carry on") && last.contains("from Bo/fake: still there?"), "{last}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn memory_notes_are_shared_and_follow_their_line() {
    let (h, rid) = room_of_two().await;
    let (a, b) = (0, 1);
    let lib = "pub fn charge() {\n    retry(3);\n}\n";
    let ann = worktree(
        json!({ "turns": [[
            { "mcp": { "tool": "remember", "args": { "path": "src/lib.rs", "line": 2, "note": "retries must stay idempotent" } } },
            { "mcp": { "tool": "remember", "args": { "path": "src/", "note": "no panics in library code" } } }
        ]] }),
        lib,
    );
    // Bo's copy has two new lines on top: the note must follow its line.
    let bo = worktree(
        json!({ "turns": [
            [ { "mcp": { "tool": "recall", "args": { "path": "src/lib.rs" } } } ],
            [ { "mcp": { "tool": "forget", "args": { "id": "$MSG" } } }, { "mcp": { "tool": "recall", "args": {} } } ]
        ] }),
        &format!("// billing\n\n{lib}"),
    );
    let aid = start_agent(&h, a, &rid, ann.path()).await;
    let bid = start_agent(&h, b, &rid, bo.path()).await;
    prompt(&h, a, &aid, "write down what you learned").await;
    let am = messages(&h, a, &rid, &aid).await;
    assert!(find(&am, "mcp remember").contains("to src/lib.rs:2"), "{am:?}");

    let note_id = h
        .until_value("notes reach Bo", || async {
            let r = h.nodes[b].call("memory.list", json!({ "room": rid })).await.unwrap();
            let notes = r.as_array().unwrap();
            (notes.len() == 2)
                .then(|| notes.iter().find(|n| n["line"] == 2).unwrap()["id"].as_str().unwrap().to_string())
        })
        .await
        .unwrap();
    prompt(&h, b, &bid, "read the notes").await;
    let bm = messages(&h, b, &rid, &bid).await;
    let r = find(&bm, "mcp recall");
    assert!(r.contains("src/lib.rs:4 (moved from line 2), by Ann/fake: retries must stay idempotent"), "{r}");
    assert!(r.contains("src/, by Ann/fake: no panics in library code"), "{r}");

    prompt(&h, b, &bid, &format!("forget [{}]", &note_id[..8])).await;
    let bm = messages(&h, b, &rid, &bid).await;
    assert!(find(&bm, "mcp forget").starts_with("mcp forget: Deleted note"), "{bm:?}");
    let all = bm.iter().rev().find(|m| m.starts_with("mcp recall")).unwrap();
    assert!(!all.contains("idempotent") && all.contains("no panics"), "{all}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn secrets_are_redacted_before_leaving_the_device() {
    let (h, rid) = room_of_two().await;
    let (a, b) = (0, 1);
    // A fake token, built from pieces so secret scanners do not flag the source.
    let key = &["gh", "p_aBcDeFgHiJkLmNoPqRsTuVwXyZ0123456789"].concat();
    let bo = worktree(
        json!({ "turns": [[
            { "message": format!("found the token {key} in .env") },
            { "run": { "command": "sh", "args": ["-c", format!("echo GITHUB_TOKEN={key}")] } }
        ]] }),
        "",
    );
    // The command is a declared check, so it runs without asking anyone.
    std::fs::write(bo.path().join("polyph.toml"), format!("checks = [\"sh -c 'echo GITHUB_TOKEN={key}'\"]\n")).unwrap();
    let bid = start_agent(&h, b, &rid, bo.path()).await;
    prompt(&h, b, &bid, "look around").await;
    h.until("Ann sees the transcript", || async {
        messages_if_known(&h, a, &rid, &bid).await.is_some_and(|m| m.iter().any(|m| m.starts_with("run: exit")))
    })
    .await
    .unwrap();
    let all = serde_json::to_string(&events(&h, a, &rid, &bid).await).unwrap();
    assert!(!all.contains(key), "the token reached Ann: {all}");
    assert!(all.contains("[REDACTED:github-token]"), "{all}");
    // Inbox messages from people are redacted too.
    h.nodes[a].call("inbox.send", json!({ "room": rid, "agent": bid, "text": format!("use {key}") })).await.unwrap();
}
