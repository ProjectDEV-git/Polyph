//! Agents the person runs themselves joining a room: directly (`mcp.connect`
//! and `mcp.call`, as `polyph mcp` does) and over the daemon's `/mcp` web
//! address with a connect key; claims, messages between them, folder links,
//! and messages that survive a daemon restart.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use polyph_testkit::harness::Harness;
use serde_json::{Value, json};

async fn room_of_two() -> (Harness, String, String) {
    let mut h = Harness::new().await.unwrap();
    let a = h.add("Ann").await.unwrap();
    let b = h.add("Bo").await.unwrap();
    let room = h.nodes[a].call("room.create", json!({ "name": "connect" })).await.unwrap();
    let rid = room["id"].as_str().unwrap().to_string();
    h.until("connected", || async {
        h.nodes[a].call("room.get", json!({ "room": rid })).await.unwrap()["connected"] == true
    })
    .await
    .unwrap();
    let rid_b = h.join(&rid, a, b, a, "member").await.unwrap();
    (h, rid, rid_b)
}

fn repo() -> tempfile::TempDir {
    let d = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(d.path().join(".git")).unwrap();
    std::fs::create_dir_all(d.path().join("src")).unwrap();
    std::fs::write(d.path().join("src/lib.rs"), "fn a() {}\n").unwrap();
    d
}

async fn tool(h: &Harness, i: usize, session: &str, tool: &str, args: Value) -> Value {
    h.nodes[i].call("mcp.call", json!({ "session": session, "tool": tool, "args": args })).await.unwrap()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn connected_agents_claim_and_talk() {
    let (h, rid, rid_b) = room_of_two().await;
    let (ra, rb) = (repo(), repo());
    // Ann links her folder; Bo has one room open, so his agent finds it alone.
    h.nodes[0].call("folder.link", json!({ "room": rid, "dir": ra.path().join("src") })).await.unwrap();
    let found = h.nodes[0].call("folder.find", json!({ "dir": ra.path().join("src") })).await.unwrap();
    assert_eq!(found["room"], rid);
    let ca =
        h.nodes[0].call("mcp.connect", json!({ "cwd": ra.path().join("src"), "client": "Claude Code" })).await.unwrap();
    assert_eq!(ca["label"], "Ann/claude-code");
    let sa = ca["session"].as_str().unwrap().to_string();
    let cb = h.nodes[1].call("mcp.connect", json!({ "cwd": rb.path(), "client": "codex" })).await.unwrap();
    assert_eq!(cb["room"], rid_b);
    let sb = cb["session"].as_str().unwrap().to_string();

    // Both show up for each other.
    h.until("Bo sees Ann's agent", || async {
        tool(&h, 1, &sb, "team_state", json!({})).await["text"].as_str().unwrap().contains("Ann/claude-code")
    })
    .await
    .unwrap();

    // Claims are real claims: Bo's agent cannot take Ann's file.
    let r = tool(&h, 0, &sa, "claim_files", json!({ "paths": ["src/lib.rs"], "reason": "refactor" })).await;
    assert_eq!(r["data"]["granted"], json!(["src/lib.rs"]), "{r}");
    h.until("Bo sees the claim", || async {
        tool(&h, 1, &sb, "list_claims", json!({})).await["text"].as_str().unwrap().contains("src/lib.rs")
    })
    .await
    .unwrap();
    let r = tool(&h, 1, &sb, "claim_files", json!({ "paths": ["src/lib.rs"], "reason": "fix" })).await;
    assert_eq!(r["data"]["denied"][0]["holder"], "Ann/claude-code", "{r}");
    // Paths outside the folder are refused.
    let r = tool(&h, 0, &sa, "claim_files", json!({ "paths": ["../x"], "reason": "x" })).await;
    assert_eq!(r["error"], true);

    // Bo's agent asks; Ann's agent is told at its next call, reads and replies.
    let r = tool(&h, 1, &sb, "ask_agent", json!({ "to": "Ann/claude-code", "message": "may I have lib.rs?" })).await;
    assert_eq!(r["error"], false, "{r}");
    let hint = h
        .until_value("Ann's agent is told", || async {
            let t = tool(&h, 0, &sa, "list_claims", json!({})).await;
            let s = t["text"].as_str().unwrap().to_string();
            s.contains("unread message").then_some(s)
        })
        .await
        .unwrap();
    assert!(hint.contains("read_inbox"));
    let inbox = tool(&h, 0, &sa, "read_inbox", json!({})).await;
    let id = inbox["data"]["messages"][0]["id"].as_str().unwrap().to_string();
    assert_eq!(inbox["data"]["messages"][0]["from"], "Bo/codex");
    tool(&h, 0, &sa, "reply", json!({ "message_id": &id[..8], "text": "in ten minutes" })).await;
    h.until("Bo gets the reply", || async {
        let t = tool(&h, 1, &sb, "read_inbox", json!({})).await;
        t["text"].as_str().unwrap().contains("in ten minutes")
    })
    .await
    .unwrap();

    // finish_task from a connected agent releases its claims without committing.
    let r = tool(&h, 0, &sa, "finish_task", json!({ "summary": "split lib" })).await;
    assert!(r["text"].as_str().unwrap().contains("commit and push"), "{r}");
    assert_eq!(r["data"]["released"], 1);

    let list = h.nodes[0].call("mcp.sessions", json!({})).await.unwrap();
    assert_eq!(list[0]["client"], "claude-code");
    h.nodes[0].call("mcp.disconnect", json!({ "session": sa })).await.unwrap();
    let r = h.nodes[0].call("mcp.call", json!({ "session": sa, "tool": "team_state", "args": {} })).await;
    assert!(r.unwrap_err().to_string().contains("reconnect"));
    h.until("Bo sees it leave", || async {
        !tool(&h, 1, &sb, "team_state", json!({})).await["text"].as_str().unwrap().contains("Ann/claude-code")
    })
    .await
    .unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn messages_wait_through_a_restart() {
    let (mut h, rid, _) = room_of_two().await;
    let (ra, rb) = (repo(), repo());
    let ca =
        h.nodes[0].call("mcp.connect", json!({ "room": rid, "cwd": ra.path(), "client": "cursor" })).await.unwrap();
    let sa = ca["session"].as_str().unwrap().to_string();
    let cb = h.nodes[1].call("mcp.connect", json!({ "cwd": rb.path(), "client": "zed" })).await.unwrap();
    let sb = cb["session"].as_str().unwrap().to_string();
    h.until("Bo sees Ann's agent", || async {
        tool(&h, 1, &sb, "team_state", json!({})).await["text"].as_str().unwrap().contains("Ann/cursor")
    })
    .await
    .unwrap();
    tool(&h, 1, &sb, "ask_agent", json!({ "to": "Ann/cursor", "message": "first" })).await;
    h.until("delivered", || async {
        tool(&h, 0, &sa, "list_claims", json!({})).await["text"].as_str().unwrap().contains("unread")
    })
    .await
    .unwrap();

    // Ann's daemon restarts: the connection is gone, the message is not.
    h.restart(0).await.unwrap();
    h.nodes[0].call("room.open", json!({ "room": rid })).await.unwrap();
    let ca =
        h.nodes[0].call("mcp.connect", json!({ "room": rid, "cwd": ra.path(), "client": "cursor" })).await.unwrap();
    assert_eq!(ca["label"], "Ann/cursor", "same label, so the messages find it");
    let sa = ca["session"].as_str().unwrap().to_string();
    let inbox = tool(&h, 0, &sa, "read_inbox", json!({})).await;
    assert!(inbox["text"].as_str().unwrap().contains("first"), "{inbox}");
    // Read messages do not come back after another restart.
    h.restart(0).await.unwrap();
    h.nodes[0].call("room.open", json!({ "room": rid })).await.unwrap();
    let ca =
        h.nodes[0].call("mcp.connect", json!({ "room": rid, "cwd": ra.path(), "client": "cursor" })).await.unwrap();
    let inbox = tool(&h, 0, ca["session"].as_str().unwrap(), "read_inbox", json!({})).await;
    assert_eq!(inbox["text"], "No new messages.");
}

async fn post(port: u16, key: &str, session: Option<&str>, body: Value) -> (u16, Option<String>, Value) {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let mut s = tokio::net::TcpStream::connect(("127.0.0.1", port)).await.unwrap();
    let body = body.to_string();
    let extra = session.map(|s| format!("Mcp-Session-Id: {s}\r\n")).unwrap_or_default();
    let req = format!(
        "POST /mcp HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nAuthorization: Bearer {key}\r\nContent-Type: application/json\r\nAccept: application/json, text/event-stream\r\n{extra}Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    s.write_all(req.as_bytes()).await.unwrap();
    let mut buf = vec![];
    s.read_to_end(&mut buf).await.unwrap();
    let text = String::from_utf8_lossy(&buf).to_string();
    let (head, rest) = text.split_once("\r\n\r\n").unwrap();
    let status: u16 = head.split_whitespace().nth(1).unwrap().parse().unwrap();
    let session = head
        .lines()
        .find(|l| l.to_lowercase().starts_with("mcp-session-id:"))
        .map(|l| l.split_once(':').unwrap().1.trim().to_string());
    (status, session, serde_json::from_str(rest).unwrap_or(Value::Null))
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn agents_over_the_web_address() {
    let (h, rid, _) = room_of_two().await;
    let ra = repo();
    h.nodes[0].call("folder.link", json!({ "room": rid, "dir": ra.path() })).await.unwrap();
    let api = polyph_daemon::api::serve(h.nodes[0].daemon.clone(), 0).await.unwrap();
    let port = api.addr.port();
    let k = h.nodes[0].call("connect.key", json!({ "client": "windsurf", "dir": ra.path() })).await.unwrap();
    let key = k["key"].as_str().unwrap().to_string();

    let (st, _, _) = post(port, "pk_wrong", None, json!({ "jsonrpc": "2.0", "id": 1, "method": "ping" })).await;
    assert_eq!(st, 401);
    let init = json!({ "jsonrpc": "2.0", "id": 1, "method": "initialize", "params": { "protocolVersion": "2025-06-18", "capabilities": {}, "clientInfo": { "name": "w", "version": "1" } } });
    let (st, session, r) = post(port, &key, None, init).await;
    assert_eq!(st, 200, "{r}");
    assert_eq!(r["result"]["protocolVersion"], "2025-06-18");
    let session = session.expect("a session id");
    let (st, _, _) =
        post(port, &key, Some(&session), json!({ "jsonrpc": "2.0", "method": "notifications/initialized" })).await;
    assert_eq!(st, 202);
    let (_, _, r) =
        post(port, &key, Some(&session), json!({ "jsonrpc": "2.0", "id": 2, "method": "tools/list" })).await;
    assert!(r["result"]["tools"].as_array().unwrap().iter().any(|t| t["name"] == "claim_files"));
    let call = json!({ "jsonrpc": "2.0", "id": 3, "method": "tools/call", "params": { "name": "claim_files", "arguments": { "paths": [ra.path().join("src/lib.rs")], "reason": "web" } } });
    let (st, _, r) = post(port, &key, Some(&session), call).await;
    assert_eq!(st, 200);
    assert_eq!(r["result"]["isError"], false, "{r}");
    assert!(r["result"]["content"][0]["text"].as_str().unwrap().contains("src/lib.rs"));
    let claims = h.nodes[0].call("claims.list", json!({ "room": rid })).await.unwrap();
    assert!(claims.to_string().contains("Ann/windsurf"), "{claims}");
    // An unknown session must initialize again.
    let (st, _, _) = post(
        port,
        &key,
        Some("nope"),
        json!({ "jsonrpc": "2.0", "id": 4, "method": "tools/call", "params": { "name": "team_state", "arguments": {} } }),
    )
    .await;
    assert_eq!(st, 404);
    h.nodes[0].call("connect.revoke", json!({ "client": "windsurf" })).await.unwrap();
    let (st, _, _) = post(port, &key, Some(&session), json!({ "jsonrpc": "2.0", "id": 5, "method": "ping" })).await;
    assert_eq!(st, 401);
    api.abort();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn edit_hook_checks_and_claims() {
    let (h, rid, _) = room_of_two().await;
    let ra = repo();
    h.nodes[0].call("folder.link", json!({ "room": rid, "dir": ra.path() })).await.unwrap();
    let file = ra.path().join("src/lib.rs");
    // Nobody connected yet: allowed, nothing to claim for.
    let r = h.nodes[0]
        .call("claims.check", json!({ "cwd": ra.path(), "paths": [file], "client": "claude-code", "claim": true }))
        .await
        .unwrap();
    assert_eq!(r["allowed"], true);
    assert_eq!(r["claimed"], json!([]));
    let cc = h.nodes[0].call("mcp.connect", json!({ "cwd": ra.path(), "client": "claude-code" })).await.unwrap();
    let codex = h.nodes[0].call("mcp.connect", json!({ "cwd": ra.path(), "client": "codex" })).await.unwrap();
    // A free file is claimed for the Claude Code connection as it edits.
    let r = h.nodes[0]
        .call(
            "claims.check",
            json!({ "cwd": ra.path().join("src"), "paths": ["lib.rs"], "client": "claude-code", "claim": true }),
        )
        .await
        .unwrap();
    assert_eq!(r["allowed"], true, "{r}");
    assert_eq!(r["claimed"], json!(["src/lib.rs"]));
    assert_eq!(r["agent"], cc["label"]);
    // Its own claim never blocks it; another agent's edit is stopped.
    let r = h.nodes[0]
        .call("claims.check", json!({ "cwd": ra.path(), "paths": [file], "client": "claude-code", "claim": true }))
        .await
        .unwrap();
    assert_eq!(r["allowed"], true);
    let r = h.nodes[0]
        .call("claims.check", json!({ "cwd": ra.path(), "paths": [file], "client": "codex", "claim": true }))
        .await
        .unwrap();
    assert_eq!(r["allowed"], false, "{r}");
    assert_eq!(r["blocked"][0]["holder"], "Ann/claude-code");
    // A directory claim covers the files in it.
    tool(&h, 0, codex["session"].as_str().unwrap(), "claim_files", json!({ "paths": ["docs/"], "reason": "rewrite" }))
        .await;
    let r = h.nodes[0]
        .call(
            "claims.check",
            json!({ "cwd": ra.path(), "paths": [ra.path().join("docs/new.md")], "client": "claude-code" }),
        )
        .await
        .unwrap();
    assert_eq!(r["allowed"], false);
    assert_eq!(r["blocked"][0]["reason"], "rewrite");
    // Outside any room: allowed.
    let elsewhere = tempfile::tempdir().unwrap();
    h.nodes[0].call("room.close", json!({ "room": rid })).await.unwrap();
    let r = h.nodes[0]
        .call("claims.check", json!({ "cwd": elsewhere.path(), "paths": ["x"], "client": "codex" }))
        .await
        .unwrap();
    assert_eq!(r["allowed"], true);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn setup_status_for_the_checklist() {
    let (h, rid, _) = room_of_two().await;
    let ra = repo();
    let s = h.nodes[0].call("setup.status", json!({})).await.unwrap();
    assert_eq!(s["name"], "Ann");
    assert_eq!(s["rooms"], 1);
    assert_eq!(s["members"], 2);
    assert_eq!(s["agents"], 0);
    assert_eq!(s["folders"], json!([]));
    assert!(s["clients"].as_array().unwrap().iter().any(|c| c["id"] == "claude-code"));
    h.nodes[0].call("folder.link", json!({ "room": rid, "dir": ra.path() })).await.unwrap();
    h.nodes[0].call("mcp.connect", json!({ "cwd": ra.path(), "client": "zed" })).await.unwrap();
    let s = h.nodes[0].call("setup.status", json!({})).await.unwrap();
    assert_eq!(s["folders"][0]["room"], rid.as_str());
    assert_eq!(s["agents"], 1);
}
