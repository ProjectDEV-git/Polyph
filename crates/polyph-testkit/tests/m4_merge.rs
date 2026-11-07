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
/// Agents' scripts live outside `/tmp`: bubblewrap gives agents a private,
/// empty `/tmp`, so a script there would be invisible to them.
fn scripts_dir() -> tempfile::TempDir {
    tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).unwrap()
}

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
    let scripts = scripts_dir();
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
    // Byte-exact merge results: no CRLF conversion on Windows checkouts.
    git(r, &["config", "core.autocrlf", "false"]);
    std::fs::create_dir_all(r.join("src")).unwrap();
    std::fs::write(r.join("src/lib.rs"), LIB).unwrap();
    std::fs::write(
        r.join("polyph.toml"),
        "checks = [\"sh -c 'grep -q charge src/lib.rs'\"]\n\n[merge]\nreviewer = \"fake-reviewer\"\n",
    )
    .unwrap();
    git(r, &["add", "-A"]);
    git(r, &["commit", "-q", "-m", "initial"]);
    h.trust_project(r).unwrap();

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
    setup_with(kinds, toml, true).await
}

/// As [`setup`]; `trust` says whether the nodes trust the repo's polyph.toml.
async fn setup_with(
    kinds: Vec<(&str, Value)>,
    toml: &str,
    trust: bool,
) -> (Harness, String, tempfile::TempDir, tempfile::TempDir) {
    let scripts = scripts_dir();
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
    // Byte-exact merge results: no CRLF conversion on Windows checkouts.
    git(r, &["config", "core.autocrlf", "false"]);
    std::fs::create_dir_all(r.join("src")).unwrap();
    std::fs::write(r.join("src/lib.rs"), LIB).unwrap();
    std::fs::write(r.join("polyph.toml"), toml).unwrap();
    git(r, &["add", "-A"]);
    git(r, &["commit", "-q", "-m", "initial"]);
    if trust {
        h.trust_project(r).unwrap();
    }
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
    // Bo must have seen the claim before its release means anything.
    h.until("claim reaches Bo", || async {
        h.nodes[1].call("claims.list", json!({ "room": rid })).await.unwrap().as_array().unwrap().len() == 1
    })
    .await
    .unwrap();
    h.nodes[0].call("task.finish", json!({ "agent": aid })).await.unwrap();
    wait_state(&h, 0, &rid, &abr, "merged").await;
    h.until("claim released", || async {
        h.nodes[1].call("claims.list", json!({ "room": rid })).await.unwrap().as_array().unwrap().is_empty()
    })
    .await
    .unwrap();
    h.nodes[1].call("agent.prompt", json!({ "agent": bid, "text": "fee" })).await.unwrap();
    let bm = messages(&h, 1, &rid, &bid).await;
    if !bm.contains(&"write: ok".to_string()) {
        let ca = h.nodes[0].call("claims.list", json!({ "room": rid })).await.unwrap();
        let cb = h.nodes[1].call("claims.list", json!({ "room": rid })).await.unwrap();
        panic!("{bm:?}\nAnn: {ca}\nBo: {cb}");
    }
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
    // The card went back (moved just after the review state is announced),
    // and the agent hears the review at its next turn.
    h.until("the card is back in progress", || async {
        let board = h.nodes[0].call("board.get", json!({ "room": rid })).await.unwrap();
        board["cards"][0]["column"] == "in_progress"
    })
    .await
    .unwrap();
    h.nodes[0].call("agent.prompt", json!({ "agent": aid, "text": "continue" })).await.unwrap();
    let said = messages(&h, 0, &rid, &aid).await;
    let p = said.iter().rev().find(|m| m.starts_with("prompt:")).unwrap();
    assert!(p.contains("The reviewer asked for changes") && p.contains("must reject negative amounts"), "{p}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn an_untrusted_project_file_cannot_skip_approval_or_checks() {
    // A polyph.toml nobody on this device trusted: `approval = false` and
    // `run_checks = false` fall back to the fail-closed defaults, and its
    // push and pull request settings are dropped, so the task merges locally.
    let (h, rid, repo, _s) = setup_with(
        vec![("fake-a", json!({ "turns": [[ { "write": { "path": "src/lib.rs", "content": with(REFUND) } } ]] }))],
        "[merge]\napproval = false\nrun_checks = false\nvia = \"github\"\nremote = \"fork\"\n",
        false,
    )
    .await;
    let r = repo.path();
    let c = add_card(&h, &rid, "Add refunds", "").await;
    let (aid, abr) = start_task(&h, 0, &rid, "fake-a", r, &c).await;
    h.nodes[0].call("agent.prompt", json!({ "agent": aid, "text": "refunds" })).await.unwrap();
    let f = h.nodes[0].call("task.finish", json!({ "agent": aid })).await.unwrap();
    // The task says the file's push and pull request settings were ignored.
    let notice = f["notice"].as_str().unwrap_or_default();
    assert!(
        notice.contains("merge.via, merge.remote")
            && notice.contains("merges locally")
            && notice.contains("`polyph trust`"),
        "{f}"
    );
    // It still waits for a person's approval.
    let e = wait_state(&h, 0, &rid, &abr, "awaiting_approval").await;
    assert!(e["detail"].as_str().unwrap_or_default().contains("merges locally"), "{e}");
    approve_merge(&h, 0, &rid, &abr).await;
    // No pull request: Polyph merges it into main itself.
    let e = wait_state(&h, 0, &rid, &abr, "merged").await;
    assert!(e["pr"].is_null(), "{e}");
    assert_eq!(std::fs::read_to_string(r.join("src/lib.rs")).unwrap(), with(REFUND));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn untrusted_checks_stop_the_merge_before_anyone_is_asked_to_approve_it() {
    // Its checks are dropped; the merge fails and says how to fix it, instead
    // of skipping them, and before a person is asked to approve it.
    let (h, rid, repo, _s) = setup_with(
        vec![("fake-a", json!({ "turns": [[ { "write": { "path": "src/lib.rs", "content": with(REFUND) } } ]] }))],
        "checks = [\"sh -c 'exit 0'\"]\n\n[merge]\napproval = false\nrun_checks = false\n",
        false,
    )
    .await;
    let r = repo.path();
    let c = add_card(&h, &rid, "Add refunds", "").await;
    let (aid, abr) = start_task(&h, 0, &rid, "fake-a", r, &c).await;
    h.nodes[0].call("agent.prompt", json!({ "agent": aid, "text": "refunds" })).await.unwrap();
    h.nodes[0].call("task.finish", json!({ "agent": aid })).await.unwrap();
    let e = h
        .until_value("the merge fails", || async {
            entry(&h, 0, &rid, &abr).await.filter(|e| e["state"] == "failed" || e["state"] == "merged")
        })
        .await
        .unwrap();
    assert_eq!(e["state"], "failed", "{e}");
    let detail = e["detail"].as_str().unwrap_or_default();
    assert!(detail.contains("Checks did not run") && detail.contains("`polyph trust`"), "{detail}");
    assert_eq!(std::fs::read_to_string(r.join("src/lib.rs")).unwrap(), LIB, "main did not move");
    let pending = h.nodes[0].call("approvals.list", json!({ "room": rid })).await.unwrap();
    assert!(!pending.as_array().unwrap().iter().any(|a| a["kind"]["t"] == "merge"), "{pending}");
}

async fn cards(h: &Harness, i: usize, rid: &str) -> Vec<Value> {
    h.nodes[i].call("board.get", json!({ "room": rid })).await.unwrap()["cards"].as_array().unwrap().clone()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn the_planner_splits_a_goal_and_agents_work_the_tasks() {
    let plan = json!({ "goal": "Billing", "tasks": [
        { "id": "t1", "title": "Add refunds", "description": "refund()", "acceptance": ["refund works"], "predicted_files": ["src/refund.rs"], "size": "s", "suggested_assignee": "Ann/fake-a" },
        { "id": "t2", "title": "Add invoices", "description": "invoice()", "acceptance": ["invoice works"], "predicted_files": ["src/lib.rs"], "depends_on": ["t1"], "size": "m" },
        { "id": "t3", "title": "Document charges", "description": "docs", "acceptance": ["documented"], "predicted_files": ["src/lib.rs"], "size": "s" }
    ] });
    let (h, rid, repo, _s) = setup(
        vec![
            // A first answer that is not a plan, then a good one.
            ("fake-planner", json!({ "turns": [
                [ { "message": "I would split it like this: {\"goal\": \"Billing\"}" } ],
                [ { "echo_prompt": true }, { "message": format!("```json\n{plan}\n```") } ]
            ] })),
            ("fake-a", json!({ "turns": [[
                { "mcp": { "tool": "list_tasks", "args": { "column": "ready" } } },
                { "write": { "path": "src/refund.rs", "content": "pub fn refund() {}\n" } },
                { "mcp": { "tool": "create_task", "args": { "title": "Refund emails", "description": "notify on refund" } } },
                { "mcp": { "tool": "finish_task", "args": { "summary": "Add refund()" } } }
            ]] })),
        ],
        "[merge]\napproval = false\n",
    )
    .await;
    let r = repo.path();
    let p = h.nodes[0]
        .call("plan.create", json!({ "room": rid, "goal": "Add billing", "cwd": r, "kind": "fake-planner" }))
        .await
        .unwrap();
    let ids: Vec<String> =
        p["cards"].as_array().unwrap().iter().map(|c| c["id"].as_str().unwrap().to_string()).collect();
    assert_eq!(ids.len(), 3);
    assert_eq!(p["overlaps"].as_array().unwrap().len(), 1, "t2 and t3 both touch src/lib.rs: {p}");
    let planner = h.nodes[0].call("agent.list", json!({ "room": rid })).await.unwrap();
    let pid = planner.as_array().unwrap().iter().find(|a| a["kind"] == "fake-planner").unwrap()["id"]
        .as_str()
        .unwrap()
        .to_string();
    let retry = messages(&h, 0, &rid, &pid).await;
    assert!(retry.iter().any(|m| m.starts_with("prompt:") && m.contains("Answer again")), "{retry:?}");

    // Drafts until accepted; then tasks without dependencies are ready.
    let all = cards(&h, 0, &rid).await;
    assert!(all.iter().all(|c| c["draft"] == true && c["column"] == "backlog"));
    let t1 = all.iter().find(|c| c["title"] == "Add refunds").unwrap().clone();
    let t2 = all.iter().find(|c| c["title"] == "Add invoices").unwrap().clone();
    assert_eq!(t2["depends_on"], json!([t1["id"]]));
    assert_eq!(t1["suggested_assignee"], "Ann/fake-a");
    let acc = h.nodes[0].call("plan.accept", json!({ "room": rid, "plan": p["plan"] })).await.unwrap();
    assert_eq!((acc["accepted"].as_u64(), acc["ready"].as_u64()), (Some(3), Some(2)));

    // An agent works t1 through its tools and finishes it itself.
    let (aid, abr) = start_task(&h, 0, &rid, "fake-a", r, t1["id"].as_str().unwrap()).await;
    h.nodes[0].call("agent.prompt", json!({ "agent": aid, "text": "go" })).await.unwrap();
    let said = messages(&h, 0, &rid, &aid).await;
    let listed = said.iter().find(|m| m.starts_with("mcp list_tasks")).unwrap();
    assert!(listed.contains("Add refunds (in_progress)") || listed.contains("Document charges (ready)"), "{listed}");
    assert!(said.iter().any(|m| m.starts_with("mcp create_task: Added task")), "{said:?}");
    let fin = said.iter().find(|m| m.starts_with("mcp finish_task")).unwrap();
    assert!(fin.contains(&format!("on {abr}")) && fin.contains("into main"), "{fin}");
    wait_state(&h, 0, &rid, &abr, "merged").await;
    assert!(r.join("src/refund.rs").exists());

    // t1 is done, so t2 is ready now.
    h.until("t2 becomes ready", || async {
        let all = cards(&h, 0, &rid).await;
        let col = |id: &Value| all.iter().find(|c| c["id"] == *id).unwrap()["column"].clone();
        col(&t1["id"]) == "done" && col(&t2["id"]) == "ready"
    })
    .await
    .unwrap();
    assert!(cards(&h, 0, &rid).await.iter().any(|c| c["title"] == "Refund emails" && c["column"] == "backlog"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_task_can_be_handed_to_another_agent() {
    let (h, rid, repo, _s) = setup(
        vec![
            (
                "fake-a",
                json!({ "turns": [[
                { "write": { "path": "src/lib.rs", "content": with(REFUND) } },
                { "message": "refund() is in; rounding is still missing" }
            ]] }),
            ),
            ("fake-b", json!({ "turns": [[ { "echo_prompt": true }, { "read": "src/lib.rs" } ]] })),
        ],
        "",
    )
    .await;
    let r = repo.path();
    let c = add_card(&h, &rid, "Add refunds", "Refunds round to cents.").await;
    let (aid, abr) = start_task(&h, 0, &rid, "fake-a", r, &c).await;
    h.nodes[0].call("agent.prompt", json!({ "agent": aid, "text": "start" })).await.unwrap();
    let out = h.nodes[0]
        .call("agent.handoff", json!({ "agent": aid, "to_kind": "fake-b", "note": "check the rounding" }))
        .await
        .unwrap();
    let next = out["next"]["id"].as_str().unwrap().to_string();
    assert_eq!(out["next"]["branch"], abr.as_str(), "same branch and worktree");
    // The successor holds the claim and gets the brief with its first prompt.
    h.until("successor holds the claim", || async {
        let cl = h.nodes[0].call("claims.list", json!({ "room": rid })).await.unwrap();
        cl.as_array()
            .unwrap()
            .iter()
            .any(|x| x["target"]["path"] == "src/lib.rs" && x["owner"]["label"] == "Ann/fake-b")
    })
    .await
    .unwrap();
    h.nodes[0].call("agent.prompt", json!({ "agent": next, "text": "carry on" })).await.unwrap();
    let said = messages(&h, 0, &rid, &next).await;
    let p = said.iter().find(|m| m.starts_with("prompt:")).unwrap();
    for want in [
        "You are taking over this task from Ann/fake-a",
        "Task: Add refunds",
        "Refunds round to cents.",
        "Work in progress, handed over by Ann/fake-a",
        "Files they edited: src/lib.rs",
        "refund() is in; rounding is still missing",
        "Their note: check the rounding",
    ] {
        assert!(p.contains(want), "missing {want:?} in {p}");
    }
    assert!(said.contains(&"read: ok".to_string()));
    let card = cards(&h, 0, &rid).await.into_iter().find(|x| x["id"] == c).unwrap();
    assert_eq!(card["handoff"], Value::Null, "the brief was taken");
    assert_eq!(card["assignee"], "Ann/fake-b");
    let log = git(r, &["log", "--format=%s", &abr]);
    assert!(log.starts_with("Work in progress, handed over by Ann/fake-a"), "{log}");
}

const TWO: &str = "pub fn charge(n: i64) -> i64 {\n    n\n}\n\npub fn refund(n: i64) -> i64 {\n    -n\n}\n";

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn symbol_claims_let_two_agents_share_a_file() {
    let (h, rid, repo, _s) = setup(
        vec![
            ("fake-a", json!({ "turns": [
                [ { "write": { "path": "src/lib.rs", "content": TWO.replace("    n\n", "    n * 2\n") } } ],
                [ { "write": { "path": "src/lib.rs", "content": TWO.replace("    n\n", "    n * 2\n").replace("    -n\n", "    0\n") } } ]
            ] })),
            ("fake-b", json!({ "turns": [[ { "write": { "path": "src/lib.rs", "content": TWO.replace("    -n\n", "    -n - 1\n") } } ]] })),
        ],
        "[claims]\ngranularity = \"symbol\"\n",
    )
    .await;
    let r = repo.path();
    std::fs::write(r.join("src/lib.rs"), TWO).unwrap();
    git(r, &["commit", "-qam", "two functions"]);
    let c1 = add_card(&h, &rid, "Double charges", "").await;
    let c2 = add_card(&h, &rid, "Refund fee", "").await;
    h.until("cards reach Bo", || async { cards(&h, 1, &rid).await.len() == 2 }).await.unwrap();
    let (aid, _) = start_task(&h, 0, &rid, "fake-a", r, &c1).await;
    let (bid, _) = start_task(&h, 1, &rid, "fake-b", r, &c2).await;
    h.nodes[0].call("agent.prompt", json!({ "agent": aid, "text": "charge" })).await.unwrap();
    h.until("Bo sees the symbol claim", || async {
        h.nodes[1].call("claims.list", json!({ "room": rid })).await.unwrap().as_array().unwrap().len() == 1
    })
    .await
    .unwrap();
    // Different function of the same file: allowed.
    h.nodes[1].call("agent.prompt", json!({ "agent": bid, "text": "refund" })).await.unwrap();
    assert!(messages(&h, 1, &rid, &bid).await.contains(&"write: ok".to_string()));
    let cl = h.nodes[0].call("claims.list", json!({ "room": rid })).await.unwrap();
    let held: Vec<(String, String)> = cl
        .as_array()
        .unwrap()
        .iter()
        .map(|c| {
            (
                format!("{}#{}", c["target"]["path"].as_str().unwrap(), c["target"]["symbol"].as_str().unwrap_or("")),
                c["owner"]["label"].as_str().unwrap().to_string(),
            )
        })
        .collect();
    assert!(held.contains(&("src/lib.rs#charge".into(), "Ann/fake-a".into())), "{held:?}");
    assert!(held.contains(&("src/lib.rs#refund".into(), "Bo/fake-b".into())), "{held:?}");
    // The same function Bo's agent holds: denied.
    h.nodes[0].call("agent.prompt", json!({ "agent": aid, "text": "refund too" })).await.unwrap();
    let am = messages(&h, 0, &rid, &aid).await;
    assert!(am.iter().any(|m| m.starts_with("write: denied") && m.contains("src/lib.rs#refund (Bo/fake-b)")), "{am:?}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn the_watcher_catches_edits_outside_the_protocol() {
    let (h, rid, repo, _s) = setup(
        vec![
            ("fake-a", json!({ "turns": [[ { "write": { "path": "src/lib.rs", "content": with(REFUND) } } ]] })),
            (
                "fake-b",
                json!({ "turns": [
                [
                    // perl -pi edits in place the same way on GNU and BSD systems
                    // (BSD sed takes -i's next argument as a backup suffix).
                    { "run": { "command": "perl", "args": ["-pi", "-e", "s/charge/charge2/", "src/lib.rs"] } },
                    { "run": { "command": "cp", "args": ["src/lib.rs", "src/new.rs"] } }
                ],
                [ { "echo_prompt": true } ]
            ] }),
            ),
        ],
        // Declared checks run without asking (commands with shell operators never do).
        "checks = [\"perl -pi -e s/charge/charge2/ src/lib.rs\", \"cp src/lib.rs src/new.rs\"]\n",
    )
    .await;
    let r = repo.path();
    let c1 = add_card(&h, &rid, "Refunds", "").await;
    let c2 = add_card(&h, &rid, "Scratch", "").await;
    h.until("cards reach Bo", || async { cards(&h, 1, &rid).await.len() == 2 }).await.unwrap();
    let (aid, _) = start_task(&h, 0, &rid, "fake-a", r, &c1).await;
    let (bid, _) = start_task(&h, 1, &rid, "fake-b", r, &c2).await;
    h.nodes[0].call("agent.prompt", json!({ "agent": aid, "text": "refunds" })).await.unwrap();
    h.until("Bo sees Ann's claim", || async {
        h.nodes[1].call("claims.list", json!({ "room": rid })).await.unwrap().as_array().unwrap().len() == 1
    })
    .await
    .unwrap();
    // Bo's agent edits through its terminal, bypassing the file tools.
    h.nodes[1].call("agent.prompt", json!({ "agent": bid, "text": "hack" })).await.unwrap();
    assert!(messages(&h, 1, &rid, &bid).await.iter().filter(|m| *m == "run: exit 0").count() == 2);
    // The free file is claimed for it.
    h.until("new.rs claimed for Bo's agent", || async {
        let cl = h.nodes[0].call("claims.list", json!({ "room": rid })).await.unwrap();
        cl.as_array().unwrap().iter().any(|c| c["target"]["path"] == "src/new.rs" && c["owner"]["label"] == "Bo/fake-b")
    })
    .await
    .unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(600)).await;
    // The held file produces a warning at its next turn.
    h.nodes[1].call("agent.prompt", json!({ "agent": bid, "text": "next" })).await.unwrap();
    let said = messages(&h, 1, &rid, &bid).await;
    let p = said.iter().rev().find(|m| m.starts_with("prompt:")).unwrap();
    assert!(p.contains("You changed src/lib.rs outside Polyph's file tools, but Ann/fake-a holds src/lib.rs"), "{p}");
    // Nothing happens for a person editing while no agent is working.
    std::fs::write(
        r.join(".polyph/worktrees").read_dir().unwrap().next().unwrap().unwrap().path().join("src/person.rs"),
        "x",
    )
    .unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(600)).await;
    let cl = h.nodes[0].call("claims.list", json!({ "room": rid })).await.unwrap();
    assert!(!cl.as_array().unwrap().iter().any(|c| c["target"]["path"] == "src/person.rs"), "{cl}");
}
