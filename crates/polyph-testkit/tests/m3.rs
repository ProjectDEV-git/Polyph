//! Milestone 3 acceptance tests: agents through real daemons, the fake ACP
//! agent, the sandbox, approvals and budgets.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::{Path, PathBuf};

use polyph_testkit::harness::{AgentBins, Harness};
use serde_json::{Value, json};

fn bins() -> AgentBins {
    AgentBins {
        fake_agent: env!("CARGO_BIN_EXE_polyph-fake-agent").into(),
        sandbox_helper: env!("CARGO_BIN_EXE_polyph-test-sandbox-exec").into(),
    }
}

/// A two-member room (Ann owner, Bo member) with agents enabled.
async fn room_of_two() -> (Harness, String) {
    let mut h = Harness::new().await.unwrap();
    h.agents = Some(bins());
    let a = h.add("Ann").await.unwrap();
    let b = h.add("Bo").await.unwrap();
    let room = h.nodes[a].call("room.create", json!({ "name": "agents" })).await.unwrap();
    let rid = room["id"].as_str().unwrap().to_string();
    h.until("connected", || async {
        h.nodes[a].call("room.get", json!({ "room": rid })).await.unwrap()["connected"] == true
    })
    .await
    .unwrap();
    h.join(&rid, a, b, a, "member").await.unwrap();
    (h, rid)
}

fn worktree(script: Value, toml: &str) -> tempfile::TempDir {
    let d = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(d.path().join("src")).unwrap();
    std::fs::write(d.path().join("src/lib.rs"), "pub fn existing() {}\n").unwrap();
    std::fs::write(d.path().join(".fake-script.json"), script.to_string()).unwrap();
    std::fs::write(d.path().join("polyph.toml"), toml).unwrap();
    d
}

async fn messages(h: &Harness, i: usize, rid: &str, agent: &str) -> Vec<String> {
    let t = h.nodes[i].call("agent.transcript", json!({ "room": rid, "agent": agent })).await.unwrap();
    t["events"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|e| e["kind"]["t"] == "message")
        .flat_map(|e| e["kind"]["c"]["text"].as_str().unwrap().lines().map(str::to_string).collect::<Vec<_>>())
        .collect()
}

async fn start_agent(h: &Harness, i: usize, rid: &str, dir: &Path, preset: &str) -> String {
    let r = h.nodes[i]
        .call("agent.start", json!({ "room": rid, "kind": "fake", "cwd": dir, "preset": preset }))
        .await
        .unwrap();
    r["id"].as_str().unwrap().to_string()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn scripted_task_completes_and_teammates_see_it() {
    let (h, rid) = room_of_two().await;
    let (a, b) = (0, 1);
    let script = json!({ "turns": [[
        { "message": "Adding the login module" },
        { "read": "src/lib.rs" },
        { "write": { "path": "src/login.rs", "content": "pub fn login() -> bool {\n    true\n}\n" } },
        { "run": { "command": "sh", "args": ["-c", "echo 3 passed, 0 failed"] } },
        { "usage": { "input": 100, "output": 20 } }
    ]] });
    let dir = worktree(script, "checks = [\"sh -c 'echo 3 passed, 0 failed'\"]\n");
    let id = start_agent(&h, b, &rid, dir.path(), "default").await;
    let r = h.nodes[b].call("agent.prompt", json!({ "agent": id, "text": "add a login function" })).await.unwrap();
    assert_eq!(r["stop_reason"], "end_turn");
    assert_eq!(
        std::fs::read_to_string(dir.path().join("src/login.rs")).unwrap(),
        "pub fn login() -> bool {\n    true\n}\n"
    );

    // Ann sees the transcript, with structured summaries.
    h.until("Ann sees the transcript", || async {
        messages(&h, a, &rid, &id).await.iter().any(|m| m == "run: exit 0")
    })
    .await
    .unwrap();
    let msgs = messages(&h, a, &rid, &id).await;
    assert!(msgs.contains(&"read: ok".to_string()) && msgs.contains(&"write: ok".to_string()), "{msgs:?}");
    let t = h.nodes[a].call("agent.transcript", json!({ "room": rid, "agent": id })).await.unwrap();
    let keys: Vec<&str> = t["summary"].as_array().unwrap().iter().map(|l| l["key"].as_str().unwrap()).collect();
    assert_eq!(keys, vec!["tool-read", "tool-edit", "tool-run-tests"], "{}", t["summary"]);
    let edit = &t["summary"][1]["args"];
    assert_eq!((edit["added"].as_u64(), edit["removed"].as_u64()), (Some(3), Some(0)));
    assert_eq!(t["summary"][2]["args"]["passed"], 3);

    // The edit auto-claimed the new file through the acting host.
    let claims = h.nodes[a].call("claims.list", json!({ "room": rid })).await.unwrap();
    assert!(
        claims
            .as_array()
            .unwrap()
            .iter()
            .any(|c| c["target"]["path"] == "src/login.rs" && c["owner"]["label"] == "Bo/fake"),
        "{claims}"
    );

    // Usage is shared.
    h.until("usage reaches Ann", || async {
        let u = h.nodes[a].call("usage.get", json!({ "room": rid })).await.unwrap();
        u["ledger"]["room"]["input_tokens"] == 100 && u["ledger"]["room"]["output_tokens"] == 20
    })
    .await
    .unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn approval_goes_to_the_owner_first_then_anyone() {
    let (mut h, rid) = room_of_two().await;
    let (a, b) = (0, 1);
    let script = json!({ "turns": [[ { "permission": { "kind": "execute", "title": "Run cargo publish", "command": "cargo publish" } } ]] });
    let dir = worktree(script, "");
    let id = start_agent(&h, b, &rid, dir.path(), "default").await;
    let bd = h.nodes[b].daemon.clone();
    let id2 = id.clone();
    let prompt = tokio::spawn(async move { bd.call("agent.prompt", json!({ "agent": id2, "text": "publish" })).await });
    let ev = h.nodes[a].wait_event("approval on Ann's device", |e| e.kind() == "approval_pending").await.unwrap();
    let approval = ev.event["approval"]["id"].as_str().unwrap().to_string();
    assert_eq!(ev.event["approval"]["kind"]["c"]["command"], "cargo publish");
    // Ann is not the agent's owner: she must wait for escalation.
    assert!(h.nodes[a].call("approve", json!({ "room": rid, "id": approval })).await.is_err());
    h.clock.advance(121);
    h.nodes[a].call("approve", json!({ "room": rid, "id": approval })).await.unwrap();
    let r = prompt.await.unwrap().map_err(|e| e.to_string()).unwrap();
    assert_eq!(r["stop_reason"], "end_turn");
    assert!(messages(&h, b, &rid, &id).await.contains(&"permission: allowed".to_string()));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn owner_can_deny_their_own_agent_immediately() {
    let (mut h, rid) = room_of_two().await;
    let b = 1;
    let script = json!({ "turns": [[ { "run": { "command": "sh", "args": ["-c", "echo not a check command"] } } ]] });
    let dir = worktree(script, "");
    let id = start_agent(&h, b, &rid, dir.path(), "default").await;
    let bd = h.nodes[b].daemon.clone();
    let id2 = id.clone();
    let prompt = tokio::spawn(async move { bd.call("agent.prompt", json!({ "agent": id2, "text": "go" })).await });
    let ev = h.nodes[b].wait_event("approval", |e| e.kind() == "approval_pending").await.unwrap();
    let approval = ev.event["approval"]["id"].as_str().unwrap().to_string();
    h.nodes[b].call("deny", json!({ "room": rid, "id": approval, "note": "not now" })).await.unwrap();
    prompt.await.unwrap().map_err(|e| e.to_string()).unwrap();
    assert!(messages(&h, b, &rid, &id).await.contains(&"run: denied".to_string()));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn budget_stops_further_prompts() {
    let (h, rid) = room_of_two().await;
    let (a, b) = (0, 1);
    h.nodes[a]
        .call(
            "budget.set",
            json!({ "room": rid, "budgets": [{ "scope": "room", "limit": { "unit": "tokens", "amount": 100 } }] }),
        )
        .await
        .unwrap();
    // Members cannot change budgets.
    assert!(h.nodes[b].call("budget.set", json!({ "room": rid, "budgets": [] })).await.is_err());
    let script = json!({ "turns": [[ { "usage": { "input": 120, "output": 30 } } ], [ { "message": "second" } ]] });
    let dir = worktree(script, "");
    h.until("budget reached Bo", || async {
        let u = h.nodes[b].call("usage.get", json!({ "room": rid })).await.unwrap();
        !u["budgets"].as_array().unwrap().is_empty()
    })
    .await
    .unwrap();
    let id = start_agent(&h, b, &rid, dir.path(), "auto").await;
    h.nodes[b].call("agent.prompt", json!({ "agent": id, "text": "one" })).await.unwrap();
    let second = h.nodes[b].call("agent.prompt", json!({ "agent": id, "text": "two" })).await;
    assert!(second.unwrap_err().to_string().contains("budget"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn edits_to_files_claimed_by_others_are_refused() {
    let (h, rid) = room_of_two().await;
    let (a, b) = (0, 1);
    let ann_dir =
        worktree(json!({ "turns": [[ { "write": { "path": "src/shared.rs", "content": "// Ann\n" } } ]] }), "");
    let bo_dir = worktree(json!({ "turns": [[ { "write": { "path": "src/shared.rs", "content": "// Bo\n" } } ]] }), "");
    let ann = start_agent(&h, a, &rid, ann_dir.path(), "default").await;
    h.nodes[a].call("agent.prompt", json!({ "agent": ann, "text": "edit" })).await.unwrap();
    h.until("Bo knows the claim", || async {
        let c = h.nodes[b].call("claims.list", json!({ "room": rid })).await.unwrap();
        !c.as_array().unwrap().is_empty()
    })
    .await
    .unwrap();
    let bo = start_agent(&h, b, &rid, bo_dir.path(), "default").await;
    h.nodes[b].call("agent.prompt", json!({ "agent": bo, "text": "edit" })).await.unwrap();
    let msgs = messages(&h, b, &rid, &bo).await;
    let denied = msgs.iter().find(|m| m.starts_with("write: denied")).unwrap_or_else(|| panic!("{msgs:?}"));
    assert!(denied.contains("Ann/fake"), "{denied}");
    assert!(!bo_dir.path().join("src/shared.rs").exists());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn sandbox_blocks_writes_outside_the_worktree() {
    let helper = vec![bins().sandbox_helper];
    if polyph_sandbox::best(&helper, false) == polyph_sandbox::Mechanism::None {
        eprintln!("skipped: no sandbox mechanism works on this machine (see `polyph sandbox test`)");
        return;
    }
    let (h, rid) = room_of_two().await;
    let b = 1;
    let outside = tempfile::tempdir().unwrap();
    let escape: PathBuf = outside.path().join("escape.txt");
    let script = json!({ "turns": [[ { "raw_write": escape }, { "raw_write": "src/inside.txt" } ]] });
    let dir = worktree(script, "");
    let id = start_agent(&h, b, &rid, dir.path(), "auto").await;
    h.nodes[b].call("agent.prompt", json!({ "agent": id, "text": "try" })).await.unwrap();
    let msgs = messages(&h, b, &rid, &id).await;
    assert_eq!(msgs, vec!["raw_write: blocked", "raw_write: ok"]);
    assert!(!escape.exists());
    assert!(dir.path().join("src/inside.txt").exists());
}
