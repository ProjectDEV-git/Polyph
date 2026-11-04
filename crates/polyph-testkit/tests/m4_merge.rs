//! Milestone 4: tasks in their own worktrees, claims between them, review,
//! approval, and a conflict merged by keeping both tasks' intent.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::Path;
use std::process::Command;

use polyph_agents::registry::AgentConfig;
use polyph_testkit::harness::{AgentBins, Harness};
use serde_json::{Value, json};

fn bins() -> AgentBins {
    AgentBins {
        fake_agent: env!("CARGO_BIN_EXE_polyph-fake-agent").into(),
        sandbox_helper: env!("CARGO_BIN_EXE_polyph-test-sandbox-exec").into(),
        mcp: env!("CARGO_BIN_EXE_polyph-test-mcp").into(),
    }
}

/// A fake agent kind whose script is `script`.
fn scripted(dir: &Path, name: &str, script: Value) -> AgentConfig {
    let p = dir.join(format!("{name}.json"));
    std::fs::write(&p, script.to_string()).unwrap();
    AgentConfig {
        command: env!("CARGO_BIN_EXE_polyph-fake-agent").into(),
        env: [("POLYPH_FAKE_SCRIPT".to_string(), p.to_string_lossy().into_owned())].into(),
        ..Default::default()
    }
}

fn git(dir: &Path, args: &[&str]) -> String {
    let o = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(["-c", "user.name=Test", "-c", "user.email=test@example.invalid"])
        .args(args)
        .output()
        .unwrap();
    assert!(o.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&o.stderr));
    String::from_utf8_lossy(&o.stdout).into_owned()
}

const LIB: &str = "pub fn charge(n: i64) -> i64 {\n    n\n}\n";

fn with(f: &str) -> String {
    format!("{LIB}\n{f}")
}

const REFUND: &str = "pub fn refund(n: i64) -> i64 {\n    -n\n}\n";
const INVOICE: &str = "pub fn invoice(n: i64) -> String {\n    format!(\"due: {n}\")\n}\n";

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

async fn entry(h: &Harness, i: usize, rid: &str, branch: &str) -> Option<Value> {
    let q = h.nodes[i].call("merge.queue", json!({ "room": rid })).await.unwrap();
    q.as_array().unwrap().iter().rev().find(|e| e["branch"] == branch).cloned()
}

/// Approves the next merge approval as `who`, once it shows up.
async fn approve_merge(h: &Harness, who: usize, rid: &str, branch: &str) {
    let id = h
        .until_value("merge approval", || async {
            let l = h.nodes[who].call("approvals.list", json!({ "room": rid })).await.unwrap();
            l.as_array()
                .unwrap()
                .iter()
                .find(|a| a["kind"]["t"] == "merge" && a["kind"]["c"]["branch"] == branch)
                .map(|a| a["id"].as_str().unwrap().to_string())
        })
        .await
        .unwrap();
    h.nodes[who].call("approve", json!({ "room": rid, "id": id })).await.unwrap();
}

async fn wait_state(h: &Harness, i: usize, rid: &str, branch: &str, state: &str) -> Value {
    let mut last = Value::Null;
    for _ in 0..400 {
        if let Some(e) = entry(h, i, rid, branch).await {
            if e["state"] == state {
                return e;
            }
            if e["state"] == "failed" {
                panic!("merge failed: {e}");
            }
            last = e;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    panic!("{branch} never reached {state}; last {last}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn two_tasks_merge_keeping_both_intents() {
    let scripts = tempfile::tempdir().unwrap();
    let mut h = Harness::new().await.unwrap();
    h.agents = Some(bins());
    h.extra_agents.insert(
        "fake-a".into(),
        scripted(
            scripts.path(),
            "a",
            json!({ "turns": [[
            { "write": { "path": "src/lib.rs", "content": with(REFUND) } }
        ]] }),
        ),
    );
    h.extra_agents.insert(
        "fake-b".into(),
        scripted(
            scripts.path(),
            "b",
            json!({ "turns": [
            [ { "write": { "path": "src/lib.rs", "content": with(INVOICE) } } ],
            [ { "write": { "path": "src/lib.rs", "content": with(INVOICE) } } ]
        ] }),
        ),
    );
    h.extra_agents.insert(
        "fake-reviewer".into(),
        scripted(
            scripts.path(),
            "r",
            json!({ "turns": [[ { "message": "Does what the card asks.\nVERDICT: APPROVE" } ]] }),
        ),
    );
    let a = h.add("Ann").await.unwrap();
    let b = h.add("Bo").await.unwrap();
    let room = h.nodes[a].call("room.create", json!({ "name": "payments" })).await.unwrap();
    let rid = room["id"].as_str().unwrap().to_string();
    h.until("connected", || async {
        h.nodes[a].call("room.get", json!({ "room": rid })).await.unwrap()["connected"] == true
    })
    .await
    .unwrap();
    h.join(&rid, a, b, a, "member").await.unwrap();

    // The repository both tasks work on, on main.
    let repo = tempfile::tempdir().unwrap();
    let r = repo.path();
    git(r, &["init", "-q", "-b", "main"]);
    std::fs::create_dir_all(r.join("src")).unwrap();
    std::fs::write(r.join("src/lib.rs"), LIB).unwrap();
    std::fs::write(
        r.join("polyph.toml"),
        "checks = [\"sh -c 'grep -q charge src/lib.rs'\"]\n\n[merge]\nreviewer = \"fake-reviewer\"\n",
    )
    .unwrap();
    git(r, &["add", "-A"]);
    git(r, &["commit", "-q", "-m", "initial"]);

    let card = |title: &'static str| {
        let (h, rid) = (&h, rid.clone());
        async move {
            let c = h.nodes[a]
                .call(
                    "board.add",
                    json!({ "room": rid, "title": title, "column": "ready", "acceptance": [format!("{title} works")] }),
                )
                .await
                .unwrap();
            c["id"].as_str().unwrap().to_string()
        }
    };
    let refunds = card("Add refunds").await;
    let invoices = card("Add invoices").await;
    h.until("cards reach Bo", || async {
        h.nodes[b].call("board.get", json!({ "room": rid })).await.unwrap()["cards"].as_array().unwrap().len() == 2
    })
    .await
    .unwrap();

    // Each task gets its own worktree and branch.
    let sa = h.nodes[a]
        .call("agent.start", json!({ "room": rid, "kind": "fake-a", "cwd": r, "task": refunds }))
        .await
        .unwrap();
    let sb = h.nodes[b]
        .call("agent.start", json!({ "room": rid, "kind": "fake-b", "cwd": r, "task": invoices }))
        .await
        .unwrap();
    let (aid, bid) = (sa["id"].as_str().unwrap().to_string(), sb["id"].as_str().unwrap().to_string());
    let (abr, bbr) = (sa["branch"].as_str().unwrap().to_string(), sb["branch"].as_str().unwrap().to_string());
    assert!(abr.starts_with("polyph/add-refunds-") && bbr.starts_with("polyph/add-invoices-"), "{abr} {bbr}");
    assert_ne!(sa["worktree"], sb["worktree"]);

    // Ann's agent edits lib.rs; Bo's agent is stopped by the claim.
    h.nodes[a].call("agent.prompt", json!({ "agent": aid, "text": "add refunds" })).await.unwrap();
    h.until("claim reaches Bo", || async {
        h.nodes[b].call("claims.list", json!({ "room": rid })).await.unwrap().as_array().unwrap().len() == 1
    })
    .await
    .unwrap();
    h.nodes[b].call("agent.prompt", json!({ "agent": bid, "text": "add invoices" })).await.unwrap();
    let bm = messages(&h, b, &rid, &bid).await;
    assert!(bm.iter().any(|m| m.starts_with("write: denied") && m.contains("Ann/fake-a")), "{bm:?}");

    // Ann's task: commit, review, approval, merge.
    h.nodes[a].call("task.finish", json!({ "agent": aid })).await.unwrap();
    approve_merge(&h, a, &rid, &abr).await;
    let e = wait_state(&h, a, &rid, &abr, "merged").await;
    assert_eq!(e["detail"], "Does what the card asks.", "the reviewer's note stays on the entry");
    assert_eq!(std::fs::read_to_string(r.join("src/lib.rs")).unwrap(), with(REFUND));
    let board = h.nodes[a].call("board.get", json!({ "room": rid })).await.unwrap();
    let col = |id: &str| board["cards"].as_array().unwrap().iter().find(|c| c["id"] == id).unwrap()["column"].clone();
    assert_eq!(col(&refunds), "done");

    // Finishing released the claim: Bo's agent can write now, on the old base.
    h.until("claim released on Bo's side", || async {
        h.nodes[b].call("claims.list", json!({ "room": rid })).await.unwrap().as_array().unwrap().is_empty()
    })
    .await
    .unwrap();
    h.nodes[b].call("agent.prompt", json!({ "agent": bid, "text": "try again" })).await.unwrap();
    assert!(messages(&h, b, &rid, &bid).await.contains(&"write: ok".to_string()));

    // Both tasks appended a function after charge(): git conflicts, Polyph
    // keeps both, the checks pass, and main moves on.
    h.nodes[b].call("task.finish", json!({ "agent": bid, "message": "Add invoices" })).await.unwrap();
    approve_merge(&h, b, &rid, &bbr).await;
    let e = wait_state(&h, b, &rid, &bbr, "merged").await;
    assert_eq!(e["resolved"], json!([["src/lib.rs", "symbols"]]), "{e}");
    let merged = std::fs::read_to_string(r.join("src/lib.rs")).unwrap();
    assert_eq!(merged, format!("{LIB}\n{REFUND}\n{INVOICE}"));
    let log = git(r, &["log", "--format=%s", "main"]);
    assert!(log.starts_with(&format!("Merge {bbr} into main: Add invoices")), "{log}");

    // Everyone sees the queue.
    h.until("Ann sees both merged", || async {
        let q = h.nodes[a].call("merge.queue", json!({ "room": rid })).await.unwrap();
        q.as_array().unwrap().iter().filter(|e| e["state"] == "merged").count() == 2
    })
    .await
    .unwrap();
}

/// Ann and Bo in a room, a repo on main with `toml`, and the given agent kinds.
async fn setup(kinds: Vec<(&str, Value)>, toml: &str) -> (Harness, String, tempfile::TempDir, tempfile::TempDir) {
    let scripts = tempfile::tempdir().unwrap();
    let mut h = Harness::new().await.unwrap();
    h.agents = Some(bins());
    for (k, s) in kinds {
        h.extra_agents.insert(k.into(), scripted(scripts.path(), k, s));
    }
    let a = h.add("Ann").await.unwrap();
    let b = h.add("Bo").await.unwrap();
    let room = h.nodes[a].call("room.create", json!({ "name": "payments" })).await.unwrap();
    let rid = room["id"].as_str().unwrap().to_string();
    h.until("connected", || async {
        h.nodes[a].call("room.get", json!({ "room": rid })).await.unwrap()["connected"] == true
    })
    .await
    .unwrap();
    h.join(&rid, a, b, a, "member").await.unwrap();
    let repo = tempfile::tempdir().unwrap();
    let r = repo.path();
    git(r, &["init", "-q", "-b", "main"]);
    std::fs::create_dir_all(r.join("src")).unwrap();
    std::fs::write(r.join("src/lib.rs"), LIB).unwrap();
    std::fs::write(r.join("polyph.toml"), toml).unwrap();
    git(r, &["add", "-A"]);
    git(r, &["commit", "-q", "-m", "initial"]);
    (h, rid, repo, scripts)
}

async fn add_card(h: &Harness, rid: &str, title: &str, description: &str) -> String {
    let c = h.nodes[0]
        .call("board.add", json!({ "room": rid, "title": title, "description": description, "column": "ready" }))
        .await
        .unwrap();
    c["id"].as_str().unwrap().to_string()
}

async fn start_task(h: &Harness, i: usize, rid: &str, kind: &str, repo: &Path, card: &str) -> (String, String) {
    let s =
        h.nodes[i].call("agent.start", json!({ "room": rid, "kind": kind, "cwd": repo, "task": card })).await.unwrap();
    (s["id"].as_str().unwrap().to_string(), s["branch"].as_str().unwrap().to_string())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn the_resolver_agent_settles_a_real_disagreement_with_both_intents() {
    let double = LIB.replace("    n\n", "    n * 2\n");
    let fee = LIB.replace("    n\n", "    n + 1\n");
    let both = LIB.replace("    n\n", "    n * 2 + 1\n");
    let (h, rid, repo, _s) = setup(
        vec![
            ("fake-a", json!({ "turns": [[ { "write": { "path": "src/lib.rs", "content": double } } ]] })),
            ("fake-b", json!({ "turns": [[ { "write": { "path": "src/lib.rs", "content": fee } } ]] })),
            (
                "fake-resolver",
                json!({ "turns": [[
                { "echo_prompt": true },
                { "read": "src/lib.rs" },
                { "write": { "path": "src/lib.rs", "content": both } },
                { "message": "Doubled first, then added the fee." }
            ]] }),
            ),
        ],
        "[merge]\napproval = false\nresolver = \"fake-resolver\"\n",
    )
    .await;
    let r = repo.path();
    let c1 = add_card(&h, &rid, "Double charges", "Premium accounts pay twice.").await;
    let c2 = add_card(&h, &rid, "Add a fee", "Every charge carries a fee of 1.").await;
    h.until("cards reach Bo", || async {
        h.nodes[1].call("board.get", json!({ "room": rid })).await.unwrap()["cards"].as_array().unwrap().len() == 2
    })
    .await
    .unwrap();
    let (aid, abr) = start_task(&h, 0, &rid, "fake-a", r, &c1).await;
    let (bid, bbr) = start_task(&h, 1, &rid, "fake-b", r, &c2).await;
    // Ann's agent finishes first; then Bo's, which changed the same line.
    h.nodes[0].call("agent.prompt", json!({ "agent": aid, "text": "double" })).await.unwrap();
    h.nodes[0].call("task.finish", json!({ "agent": aid })).await.unwrap();
    wait_state(&h, 0, &rid, &abr, "merged").await;
    h.until("claim released", || async {
        h.nodes[1].call("claims.list", json!({ "room": rid })).await.unwrap().as_array().unwrap().is_empty()
    })
    .await
    .unwrap();
    h.nodes[1].call("agent.prompt", json!({ "agent": bid, "text": "fee" })).await.unwrap();
    h.nodes[1].call("task.finish", json!({ "agent": bid })).await.unwrap();
    let e = wait_state(&h, 1, &rid, &bbr, "merged").await;
    assert_eq!(e["resolved"], json!([["src/lib.rs", "agent:fake-resolver"]]), "{e}");
    assert_eq!(std::fs::read_to_string(r.join("src/lib.rs")).unwrap(), LIB.replace("    n\n", "    n * 2 + 1\n"));

    // The resolver saw the conflict and both tasks' intent.
    let agents = h.nodes[1].call("agent.list", json!({ "room": rid })).await.unwrap();
    let res = agents.as_array().unwrap().iter().find(|a| a["kind"] == "fake-resolver").unwrap()["id"]
        .as_str()
        .unwrap()
        .to_string();
    let said = messages(&h, 1, &rid, &res).await;
    let p = said.iter().find(|m| m.starts_with("prompt:")).unwrap();
    assert!(p.contains("Double charges") && p.contains("Premium accounts pay twice."), "{p}");
    assert!(p.contains("Add a fee") && p.contains("Every charge carries a fee of 1."), "{p}");
    assert!(said.contains(&"read: ok".to_string()));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_reviewer_can_send_a_task_back() {
    let (h, rid, repo, _s) = setup(
        vec![
            (
                "fake-a",
                json!({ "turns": [
                [ { "write": { "path": "src/lib.rs", "content": with(REFUND) } } ],
                [ { "echo_prompt": true } ]
            ] }),
            ),
            (
                "fake-strict",
                json!({ "turns": [[ { "message": "refund() must reject negative amounts.\nVERDICT: CHANGES" } ]] }),
            ),
        ],
        "[merge]\nreviewer = \"fake-strict\"\n",
    )
    .await;
    let r = repo.path();
    let c = add_card(&h, &rid, "Add refunds", "").await;
    let (aid, abr) = start_task(&h, 0, &rid, "fake-a", r, &c).await;
    h.nodes[0].call("agent.prompt", json!({ "agent": aid, "text": "refunds" })).await.unwrap();
    h.nodes[0].call("task.finish", json!({ "agent": aid })).await.unwrap();
    let e = wait_state(&h, 0, &rid, &abr, "changes_requested").await;
    assert_eq!(e["detail"], "refund() must reject negative amounts.");
    assert_eq!(std::fs::read_to_string(r.join("src/lib.rs")).unwrap(), LIB, "main did not move");
    // The card went back, and the agent hears the review at its next turn.
    let board = h.nodes[0].call("board.get", json!({ "room": rid })).await.unwrap();
    assert_eq!(board["cards"][0]["column"], "in_progress");
    h.nodes[0].call("agent.prompt", json!({ "agent": aid, "text": "continue" })).await.unwrap();
    let said = messages(&h, 0, &rid, &aid).await;
    let p = said.iter().rev().find(|m| m.starts_with("prompt:")).unwrap();
    assert!(p.contains("The reviewer asked for changes") && p.contains("must reject negative amounts"), "{p}");
}
