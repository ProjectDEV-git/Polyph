//! Shared context and big-project tools through connected agents: the
//! project brief, get_context within its size limit, search across notes,
//! cards and threads, recent work with summaries, code areas, and next_task
//! handing each ready task to exactly one agent across devices.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::collections::HashSet;

use polyph_testkit::harness::Harness;
use serde_json::{Value, json};

async fn room_of_two() -> (Harness, String, String) {
    let mut h = Harness::new().await.unwrap();
    let a = h.add("Ann").await.unwrap();
    let b = h.add("Bo").await.unwrap();
    let room = h.nodes[a].call("room.create", json!({ "name": "context" })).await.unwrap();
    let rid = room["id"].as_str().unwrap().to_string();
    h.until("connected", || async {
        h.nodes[a].call("room.get", json!({ "room": rid })).await.unwrap()["connected"] == true
    })
    .await
    .unwrap();
    let rid_b = h.join(&rid, a, b, a, "member").await.unwrap();
    (h, rid, rid_b)
}

fn repo(areas: &str) -> tempfile::TempDir {
    let d = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(d.path().join(".git")).unwrap();
    std::fs::create_dir_all(d.path().join(".polyph")).unwrap();
    std::fs::create_dir_all(d.path().join("src/billing")).unwrap();
    std::fs::write(d.path().join("src/billing/charge.rs"), "fn charge() {}\nfn refund() {}\n").unwrap();
    std::fs::write(d.path().join(".polyph/AREAS"), areas).unwrap();
    std::fs::write(d.path().join(".polyph/brief.md"), "Repository brief: Rust, no unwrap.").unwrap();
    d
}

async fn tool(h: &Harness, i: usize, session: &str, tool: &str, args: Value) -> Value {
    h.nodes[i].call("mcp.call", json!({ "session": session, "tool": tool, "args": args })).await.unwrap()
}

async fn connect(h: &Harness, i: usize, room: &str, dir: &std::path::Path, client: &str) -> String {
    let r = h.nodes[i].call("mcp.connect", json!({ "room": room, "cwd": dir, "client": client })).await.unwrap();
    r["session"].as_str().unwrap().to_string()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn brief_context_search_and_recent_work() {
    let (h, rid, rid_b) = room_of_two().await;
    let ra = repo("src/billing/ @Ann # money: every change needs a test\n");
    let sa = connect(&h, 0, &rid, ra.path(), "claude-code").await;

    // Before anyone writes a brief, the repository's file stands in.
    let c = tool(&h, 0, &sa, "get_context", json!({})).await;
    assert!(c["text"].as_str().unwrap().contains("Repository brief"), "{c}");
    assert_eq!(c["data"]["brief"], ".polyph/brief.md");

    // A person writes the room's brief; it reaches Bo's side too.
    h.nodes[0]
        .call("brief.set", json!({ "room": rid, "text": "Payments service. Decision: all money is in cents." }))
        .await
        .unwrap();
    h.until("Bo has the brief", || async {
        let b = h.nodes[1].call("brief.get", json!({ "room": rid_b })).await.unwrap();
        b["text"].as_str().is_some_and(|t| t.contains("cents"))
    })
    .await
    .unwrap();

    // A note, a card with a summary, a thread.
    tool(
        &h,
        0,
        &sa,
        "remember",
        json!({ "path": "src/billing/charge.rs", "line": 2, "note": "refund must be idempotent" }),
    )
    .await;
    let card = h.nodes[0]
        .call("board.add", json!({ "room": rid, "title": "Retry webhooks", "column": "ready", "files": ["src/billing/charge.rs"], "acceptance": ["retries 3 times"] }))
        .await
        .unwrap();
    let card_id = card["id"].as_str().unwrap().to_string();
    let t = h.nodes[0].call("thread.create", json!({ "room": rid, "title": "Currency rounding" })).await.unwrap();
    let tid = t["id"].as_str().or(t["thread"]["id"].as_str()).unwrap().to_string();
    h.nodes[0].call("chat.send", json!({ "room": rid, "thread": tid, "text": "we round half to even" })).await.unwrap();

    // next_task gives Ann's agent the card; get_context now has everything.
    let n = tool(&h, 0, &sa, "next_task", json!({})).await;
    assert_eq!(n["data"]["id"], card_id.as_str(), "{n}");
    assert!(n["text"].as_str().unwrap().contains("looked after by @Ann"), "{n}");
    let again = tool(&h, 0, &sa, "next_task", json!({})).await;
    assert_eq!(again["data"]["already"], true);
    let c = tool(&h, 0, &sa, "get_context", json!({})).await;
    let text = c["text"].as_str().unwrap();
    for want in
        ["Retry webhooks", "retries 3 times", "cents", "refund must be idempotent", "money: every change needs a test"]
    {
        assert!(text.contains(want), "missing {want} in {text}");
    }
    assert_eq!(c["data"]["task"], card_id.as_str());
    // A small limit cuts the lower sections and says so.
    let small = tool(&h, 0, &sa, "get_context", json!({ "max_chars": 1000 })).await;
    assert!(small["text"].as_str().unwrap().chars().count() <= 1300);
    assert!(small["text"].as_str().unwrap().contains("Your task"));

    // Search finds each kind of thing.
    let s = tool(&h, 0, &sa, "search", json!({ "query": "idempotent" })).await;
    assert_eq!(s["data"]["hits"][0]["kind"], "note", "{s}");
    let s = tool(&h, 0, &sa, "search", json!({ "query": "round even" })).await;
    assert_eq!(s["data"]["hits"][0]["kind"], "thread", "{s}");
    let s = h.nodes[0].call("search", json!({ "room": rid, "query": "webhooks" })).await.unwrap();
    assert_eq!(s[0]["kind"], "card");

    // Finishing leaves a summary that late joiners read.
    tool(&h, 0, &sa, "finish_task", json!({ "summary": "Webhooks retry with backoff; see charge.rs" })).await;
    let sb = connect(&h, 1, &rid_b, ra.path(), "codex").await;
    h.until("Bo's agent sees the summary", || async {
        let r = tool(&h, 1, &sb, "recent_work", json!({})).await;
        r["text"].as_str().unwrap().contains("retry with backoff")
    })
    .await
    .unwrap();
    let s = tool(&h, 1, &sb, "search", json!({ "query": "backoff" })).await;
    assert_eq!(s["data"]["hits"][0]["kind"], "card");
    // Claiming a file in someone else's area says whose it is.
    let r = tool(&h, 1, &sb, "claim_files", json!({ "paths": ["src/billing/charge.rs"], "reason": "fix" })).await;
    assert!(r["text"].as_str().unwrap().contains("looked after by @Ann"), "{r}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 8)]
async fn next_task_gives_each_card_to_one_agent() {
    let (h, rid, rid_b) = room_of_two().await;
    let dir = repo("");
    let cards = 6;
    for i in 0..cards {
        h.nodes[0]
            .call("board.add", json!({ "room": rid, "title": format!("task {i}"), "column": "ready" }))
            .await
            .unwrap();
    }
    // A blocked card is never handed out.
    let dep =
        h.nodes[0].call("board.add", json!({ "room": rid, "title": "later", "column": "backlog" })).await.unwrap();
    let blocked =
        h.nodes[0].call("board.add", json!({ "room": rid, "title": "blocked", "column": "ready" })).await.unwrap();
    h.nodes[0]
        .call("board.update", json!({ "room": rid, "card": blocked["id"], "depends_on": [dep["id"]] }))
        .await
        .unwrap();
    h.until("Bo sees the cards", || async {
        let b = h.nodes[1].call("board.get", json!({ "room": rid_b })).await.unwrap();
        b.to_string().matches("task ").count() >= cards
    })
    .await
    .unwrap();
    // Eight agents on two devices ask at the same moment.
    let mut sessions = vec![];
    for i in 0..8 {
        let node = i % 2;
        let room = if node == 0 { &rid } else { &rid_b };
        sessions.push((node, connect(&h, node, room, dir.path(), &format!("agent{i}")).await));
    }
    let h = std::sync::Arc::new(h);
    let mut tasks = vec![];
    for (node, s) in sessions {
        let h = h.clone();
        tasks.push(tokio::spawn(async move { tool(&h, node, &s, "next_task", json!({})).await }));
    }
    let mut got = vec![];
    for t in tasks {
        let r = t.await.unwrap();
        if let Some(id) = r["data"]["id"].as_str() {
            got.push(id.to_string());
        }
    }
    let unique: HashSet<&String> = got.iter().collect();
    assert_eq!(unique.len(), got.len(), "a card went to two agents: {got:?}");
    assert_eq!(got.len(), cards, "every ready, unblocked card was taken: {got:?}");
    assert!(!got.contains(&blocked["id"].as_str().unwrap().to_string()));
}
