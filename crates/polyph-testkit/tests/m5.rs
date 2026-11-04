//! Milestone 5: board, threads, activity with replay, whiteboards.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use polyph_testkit::harness::{AgentBins, Harness};
use serde_json::{Value, json};

fn bins() -> AgentBins {
    AgentBins {
        fake_agent: env!("CARGO_BIN_EXE_polyph-fake-agent").into(),
        sandbox_helper: env!("CARGO_BIN_EXE_polyph-test-sandbox-exec").into(),
        mcp: env!("CARGO_BIN_EXE_polyph-test-mcp").into(),
    }
}

/// Ann (owner) and Bo (member) in one room. Ann may create things first with
/// `before_join`, which runs before Bo joins.
async fn room(before_join: impl AsyncFnOnce(&Harness, &str)) -> (Harness, String) {
    let mut h = Harness::new().await.unwrap();
    h.agents = Some(bins());
    let a = h.add("Ann").await.unwrap();
    let b = h.add("Bo").await.unwrap();
    let room = h.nodes[a].call("room.create", json!({ "name": "m5" })).await.unwrap();
    let rid = room["id"].as_str().unwrap().to_string();
    h.until("connected", || async {
        h.nodes[a].call("room.get", json!({ "room": rid })).await.unwrap()["connected"] == true
    })
    .await
    .unwrap();
    before_join(&h, &rid).await;
    h.join(&rid, a, b, a, "member").await.unwrap();
    (h, rid)
}

async fn call(h: &Harness, i: usize, m: &str, p: Value) -> Value {
    h.nodes[i].call(m, p).await.unwrap()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn board_threads_and_activity_with_replay() {
    let (mut h, rid) = room(async |_, _| {}).await;
    let (a, b) = (0, 1);
    h.clock.advance(10);
    let c = call(&h, a, "board.add", json!({ "room": rid, "title": "Add login", "column": "ready" })).await;
    let card = c["id"].as_str().unwrap().to_string();
    h.clock.advance(10);
    let t_ready = h.clock.now();
    h.clock.advance(10);
    call(&h, a, "board.move", json!({ "room": rid, "card": card, "column": "in_progress" })).await;
    h.clock.advance(10);
    call(
        &h,
        a,
        "board.update",
        json!({ "room": rid, "card": &card[..8], "title": "Add OAuth login", "assignee": "Bo/claude" }),
    )
    .await;
    let t_moved = h.clock.now();

    // Bo sees the edits and the activity feed, attributed to Ann.
    h.until("activity reaches Bo", || async {
        call(&h, b, "activity.list", json!({ "room": rid, "kinds": ["card_created", "card_moved", "card_updated"] }))
            .await
            .as_array()
            .unwrap()
            .len()
            == 3
    })
    .await
    .unwrap();
    let feed = call(&h, b, "activity.list", json!({ "room": rid })).await;
    let kinds: Vec<&str> = feed.as_array().unwrap().iter().map(|e| e["kind"].as_str().unwrap()).collect();
    assert!(kinds.ends_with(&["card_created", "card_moved", "card_updated"]), "{kinds:?}");
    let upd = feed.as_array().unwrap().iter().find(|e| e["kind"] == "card_updated").unwrap();
    assert_eq!(upd["args"]["fields"], json!(["title", "assignee"]));
    assert_eq!(upd["args"]["assignee"], "Bo/claude");
    let whoami = call(&h, a, "whoami", json!({})).await;
    assert_eq!(upd["actor"], whoami["member"]);

    // Replay: before the move the card was ready, under its old title.
    let r = call(&h, b, "activity.replay", json!({ "room": rid, "at": t_ready })).await;
    assert_eq!(r["columns"]["ready"], json!([[card, "Add login"]]), "{r}");
    let r = call(&h, b, "activity.replay", json!({ "room": rid, "at": t_moved })).await;
    assert_eq!(r["columns"]["in_progress"], json!([[card, "Add OAuth login"]]), "{r}");

    // A thread about the card, with posts from both.
    let t = call(
        &h,
        b,
        "thread.create",
        json!({ "room": rid, "title": "Which provider?", "card": card, "text": "GitHub or Google first?" }),
    )
    .await;
    let tid = t["id"].as_str().unwrap().to_string();
    h.until("thread reaches Ann", || async {
        call(&h, a, "thread.list", json!({ "room": rid }))
            .await
            .as_array()
            .unwrap()
            .iter()
            .any(|x| x["thread"]["id"] == tid && x["posts"] == 1)
    })
    .await
    .unwrap();
    h.clock.advance(5);
    call(&h, a, "chat.send", json!({ "room": rid, "text": "GitHub", "thread": tid })).await;
    h.until("reply reaches Bo", || async {
        let g = call(&h, b, "thread.get", json!({ "room": rid, "thread": &tid[..8] })).await;
        g["posts"].as_array().unwrap().len() == 2
    })
    .await
    .unwrap();
    let g = call(&h, b, "thread.get", json!({ "room": rid, "thread": tid })).await;
    assert_eq!((g["posts"][0]["name"].as_str(), g["posts"][1]["name"].as_str()), (Some("Bo"), Some("Ann")));
    assert_eq!(g["thread"]["card"], card.as_str());
    let board = call(&h, a, "board.get", json!({ "room": rid })).await;
    assert_eq!(board["cards"][0]["thread"], tid.as_str());

    // Removing is logged too.
    call(&h, a, "board.remove", json!({ "room": rid, "card": card })).await;
    h.until("removal reaches Bo", || async {
        call(&h, b, "board.get", json!({ "room": rid })).await["cards"].as_array().unwrap().is_empty()
    })
    .await
    .unwrap();
    let feed = call(&h, a, "activity.list", json!({ "room": rid, "kinds": ["card_removed"] })).await;
    assert_eq!(feed[0]["args"]["title"], "Add OAuth login");
    h.clock.advance(1);
    assert!(
        call(&h, a, "activity.replay", json!({ "room": rid, "at": h.clock.now() })).await["columns"]
            .as_object()
            .unwrap()
            .is_empty()
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn whiteboards_sync_and_agents_draw_and_read_them() {
    // A whiteboard that exists before Bo joins must still reach him.
    let (h, rid) = room(async |h: &Harness, rid: &str| {
        let w = h.nodes[0].call("whiteboard.create", json!({ "room": rid, "name": "Sketches" })).await.unwrap();
        let rect = json!({ "id": "r1", "type": "rectangle", "x": 10, "y": 20, "width": 100, "height": 50 });
        h.nodes[0].call("whiteboard.put", json!({ "room": rid, "board": w["id"], "elements": [rect] })).await.unwrap();
    })
    .await;
    let (a, b) = (0, 1);
    h.until("old whiteboard reaches Bo", || async {
        let l = call(&h, b, "whiteboard.list", json!({ "room": rid })).await;
        if l.as_array().unwrap().len() != 1 {
            return false;
        }
        let g = h.nodes[b].call("whiteboard.get", json!({ "room": rid, "board": "sketches" })).await;
        g.is_ok_and(|g| g["elements"].as_array().unwrap().len() == 1)
    })
    .await
    .unwrap();

    // Bo's agent draws a diagram on a new board; then reads Ann's sketch.
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("polyph.toml"), "").unwrap();
    std::fs::write(
        dir.path().join(".fake-script.json"),
        json!({ "turns": [[
            { "mcp": { "tool": "draw_diagram", "args": {
                "board": "Architecture", "diagram": "auth",
                "nodes": [{ "id": "web", "label": "Web app" }, { "id": "api", "label": "API" }, { "id": "db", "label": "Postgres" }],
                "edges": [{ "from": "web", "to": "api", "label": "login" }, { "from": "api", "to": "db" }]
            } } },
            { "mcp": { "tool": "draw_diagram", "args": {
                "board": "Architecture", "diagram": "auth",
                "nodes": [{ "id": "web", "label": "Web app" }, { "id": "api", "label": "Auth API" }],
                "edges": [{ "from": "web", "to": "api", "label": "login" }]
            } } },
            { "mcp": { "tool": "list_whiteboards", "args": {} } },
            { "mcp": { "tool": "read_whiteboard", "args": { "board": "Architecture" } } }
        ]] })
        .to_string(),
    )
    .unwrap();
    let ag = call(&h, b, "agent.start", json!({ "room": rid, "kind": "fake", "cwd": dir.path(), "preset": "default" }))
        .await;
    let id = ag["id"].as_str().unwrap().to_string();
    call(&h, b, "agent.prompt", json!({ "agent": id, "text": "draw the auth flow" })).await;
    let t = call(&h, b, "agent.transcript", json!({ "room": rid, "agent": id })).await;
    let said: Vec<String> = t["events"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|e| e["kind"]["t"] == "message")
        .flat_map(|e| e["kind"]["c"]["text"].as_str().unwrap().lines().map(str::to_string).collect::<Vec<_>>())
        .filter(|l| l.starts_with("mcp "))
        .collect();
    assert!(said[0].contains("Drew 3 boxes and 2 arrows on a new whiteboard \"Architecture\""), "{said:?}");
    assert!(said[1].contains("Redrew 2 boxes and 1 arrows"), "{said:?}");
    assert!(said[2].contains("Sketches") && said[2].contains("Architecture"), "{said:?}");
    assert!(said[3].contains("rectangle \"Auth API\"") && said[3].contains("from Web app to Auth API"), "{said:?}");
    assert!(!said[3].contains("Postgres"), "the redraw replaced the old diagram: {said:?}");

    // Ann sees the agent's diagram.
    h.until("diagram reaches Ann", || async {
        h.nodes[a]
            .call("whiteboard.get", json!({ "room": rid, "board": "Architecture" }))
            .await
            .is_ok_and(|g| g["text"].as_str().unwrap_or_default().contains("arrow \"login\" from Web app to Auth API"))
    })
    .await
    .unwrap();
}

#[cfg(unix)]
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn shared_terminal_with_input_approval() {
    let (h, rid) = room(async |_, _| {}).await;
    let (a, b) = (0, 1);
    let dir = tempfile::tempdir().unwrap();
    let t = call(
        &h,
        a,
        "term.open",
        json!({ "room": rid, "cwd": dir.path(), "command": ["sh"], "title": "Ann's shell", "cols": 80, "rows": 24 }),
    )
    .await;
    let id = t["id"].as_str().unwrap().to_string();
    h.until("Bo sees the terminal", || async {
        call(&h, b, "term.list", json!({ "room": rid }))
            .await
            .as_array()
            .unwrap()
            .iter()
            .any(|x| x["id"] == id && x["title"] == "Ann's shell")
    })
    .await
    .unwrap();
    let read = |i: usize| {
        let (h, rid, id) = (&h, rid.clone(), id.clone());
        async move {
            call(h, i, "term.read", json!({ "room": rid, "term": id })).await["text"].as_str().unwrap().to_string()
        }
    };

    // Ann types; Bo watches. A token in the output is redacted on the way.
    let key = ["gh", "p_aBcDeFgHiJkLmNoPqRsTuVwXyZ0123456789"].concat();
    call(&h, a, "term.input", json!({ "room": rid, "term": id, "text": format!("echo ann-''was-here {key}\n") })).await;
    h.until("output reaches Bo", || async { read(b).await.contains("ann-was-here") }).await.unwrap();
    let seen = read(b).await;
    assert!(!seen.contains(&key) && seen.contains("[REDACTED:github-token]"), "{seen}");

    // Bo cannot type until Ann lets him, and cannot approve his own request.
    assert!(h.nodes[b].call("term.input", json!({ "room": rid, "term": id, "text": "id\n" })).await.is_err());
    call(&h, b, "term.ask", json!({ "room": rid, "term": id })).await;
    let ask = h
        .until_value("Ann is asked", || async {
            let l = call(&h, a, "approvals.list", json!({ "room": rid })).await;
            l.as_array()
                .unwrap()
                .iter()
                .find(|x| x["kind"]["t"] == "terminal_input")
                .map(|x| x["id"].as_str().unwrap().to_string())
        })
        .await
        .unwrap();

    h.clock.advance(300);
    h.until("Bo sees the request", || async {
        call(&h, b, "approvals.list", json!({ "room": rid })).await.as_array().unwrap().len() == 1
    })
    .await
    .unwrap();
    assert!(h.nodes[b].call("approve", json!({ "room": rid, "id": ask })).await.is_err(), "the asker cannot approve");
    call(&h, a, "approve", json!({ "room": rid, "id": ask })).await;
    let bo = call(&h, b, "whoami", json!({})).await["member"].as_str().unwrap().to_string();
    h.until("Bo holds the keyboard", || async {
        call(&h, b, "term.list", json!({ "room": rid })).await[0]["writer"] == bo.as_str()
    })
    .await
    .unwrap();
    call(&h, b, "term.input", json!({ "room": rid, "term": &id[..8], "text": "echo bo-''typed\n" })).await;
    h.until("Bo's typing runs on Ann's machine", || async { read(a).await.contains("bo-typed") }).await.unwrap();

    // Ann takes the keyboard back.
    call(&h, a, "term.revoke", json!({ "room": rid, "term": id })).await;
    h.until("Bo lost the keyboard", || async {
        call(&h, b, "term.list", json!({ "room": rid })).await[0]["writer"].is_null()
    })
    .await
    .unwrap();
    assert!(h.nodes[b].call("term.input", json!({ "room": rid, "term": id, "text": "id\n" })).await.is_err());

    // Closing ends it for everyone, with the exit status.
    call(&h, a, "term.input", json!({ "room": rid, "term": id, "text": "exit 4\n" })).await;
    h.until("closed for Bo", || async {
        let l = call(&h, b, "term.list", json!({ "room": rid })).await;
        l[0]["closed"] == true && l[0]["exit"] == 4
    })
    .await
    .unwrap();
}
