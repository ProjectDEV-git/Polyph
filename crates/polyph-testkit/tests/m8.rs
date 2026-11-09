//! Milestone 8: a browser-only member (the phone engine) in a room with
//! daemons, through the real relay. The PWA runs the same engine in wasm.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::sync::Arc;

use polyph_phone::Phone;
use polyph_proto::identity::DeviceIdentity;
use polyph_proto::ids::MemberId;
use polyph_testkit::harness::{AgentBins, Harness};
use polyph_testkit::phone::PhoneDriver;
use serde_json::json;

fn bins() -> AgentBins {
    AgentBins {
        fake_agent: env!("CARGO_BIN_EXE_polyph-fake-agent").into(),
        sandbox_helper: env!("CARGO_BIN_EXE_polyph-test-sandbox-exec").into(),
        mcp: env!("CARGO_BIN_EXE_polyph-test-mcp").into(),
    }
}

/// Ann's room, and a phone joined to it. With `pair`, the phone joins as
/// Ann's own member (another device of hers); otherwise as a new member.
async fn room_with_phone(pair: bool) -> (Harness, String, PhoneDriver) {
    let mut h = Harness::new().await.unwrap();
    h.agents = Some(bins());
    let a = h.add("Ann").await.unwrap();
    let room = h.nodes[a].call("room.create", json!({ "name": "payments" })).await.unwrap();
    let rid = room["id"].as_str().unwrap().to_string();
    h.until("connected", || async {
        h.nodes[a].call("room.get", json!({ "room": rid })).await.unwrap()["connected"] == true
    })
    .await
    .unwrap();
    let member = if pair {
        let who = h.nodes[a].call("whoami", json!({})).await.unwrap();
        Some(MemberId::from_hex(who["member"].as_str().unwrap()).unwrap())
    } else {
        None
    };
    let identity = Arc::new(DeviceIdentity::generate().unwrap());
    let phone = Phone::new(identity, member, "Ann's phone", &h.relay.url()).unwrap();
    let mut p = PhoneDriver::connect(phone, &h.relay.url(), h.clock.clone()).await.unwrap();

    // Pairing uses the real pairing call: only its codes may add a device of Ann's own.
    let inv = h.nodes[a].call(if pair { "phone.pair" } else { "room.invite" }, json!({ "room": rid })).await.unwrap();
    p.start_join(inv["code"].as_str().unwrap(), if pair { "Ann" } else { "Pim" }).await.unwrap();
    let knock = h.nodes[a].wait_event("knock", |e| e.kind() == "knock").await.unwrap();
    assert_eq!(knock.event["knock"]["surface"], "phone");
    let kid = knock.event["knock"]["id"].as_str().unwrap().to_string();
    p.wait_event("join_pending", |_| true).await.unwrap();
    h.nodes[a].call("knock.decide", json!({ "room": rid, "knock": kid, "approve": true })).await.unwrap();
    let joined = p.wait_event("joined", |_| true).await.unwrap();
    assert_eq!(joined["event"]["room"], rid.as_str());
    assert_eq!(joined["event"]["name"], "payments");
    (h, rid, p)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn phone_joins_chats_and_moves_cards() {
    let (h, rid, p) = room_with_phone(false).await;
    let a = 0;
    // Chat both ways.
    p.call("chat.send", json!({ "room": rid, "text": "from the phone — ส่งจากมือถือ" })).await.unwrap();
    h.until("Ann reads the phone", || async {
        let v = h.nodes[a].call("chat.list", json!({ "room": rid })).await.unwrap();
        v.as_array().unwrap().iter().any(|m| m["text"] == "from the phone — ส่งจากมือถือ" && m["name"] == "Pim")
    })
    .await
    .unwrap();
    h.nodes[a].call("chat.send", json!({ "room": rid, "text": "got it" })).await.unwrap();
    h.until("the phone reads Ann", || async {
        let v = p.call("chat.list", json!({ "room": rid })).await.unwrap();
        v.as_array().unwrap().iter().any(|m| m["text"] == "got it" && m["name"] == "Ann")
    })
    .await
    .unwrap();

    // The board syncs, and a move on the phone reaches the daemon.
    let card = h.nodes[a].call("board.add", json!({ "room": rid, "title": "Ship the PWA" })).await.unwrap();
    let cid = card["id"].as_str().unwrap().to_string();
    h.until("card on the phone", || async {
        let b = p.call("board.get", json!({ "room": rid })).await.unwrap();
        b["cards"].as_array().unwrap().iter().any(|c| c["id"] == cid.as_str())
    })
    .await
    .unwrap();
    p.call("board.move", json!({ "room": rid, "card": cid, "column": "in_progress" })).await.unwrap();
    h.until("move reaches Ann", || async {
        let b = h.nodes[a].call("board.get", json!({ "room": rid })).await.unwrap();
        b["cards"].as_array().unwrap().iter().any(|c| c["id"] == cid.as_str() && c["column"] == "in_progress")
    })
    .await
    .unwrap();

    // A card added on the phone reaches the daemon.
    p.call("board.add", json!({ "room": rid, "title": "From the phone" })).await.unwrap();
    h.until("phone card reaches Ann", || async {
        let b = h.nodes[a].call("board.get", json!({ "room": rid })).await.unwrap();
        b["cards"].as_array().unwrap().iter().any(|c| c["title"] == "From the phone")
    })
    .await
    .unwrap();

    // The activity feed reads like the daemon's.
    let act = p.call("activity.list", json!({ "room": rid, "limit": 300 })).await.unwrap();
    assert!(act.is_array(), "{act}");

    // Members and rooms look the same as on a daemon.
    let members = p.call("room.members", json!({ "room": rid })).await.unwrap();
    let names: Vec<&str> = members.as_array().unwrap().iter().map(|m| m["name"].as_str().unwrap()).collect();
    assert!(names.contains(&"Ann") && names.contains(&"Pim"), "{names:?}");
    let rooms = p.call("room.list", json!({})).await.unwrap();
    assert_eq!(rooms[0]["id"], rid.as_str());
    assert_eq!(rooms[0]["connected"], true);
    // Daemon-only features say so instead of failing silently.
    let e = p.call("agent.start", json!({ "room": rid })).await.unwrap_err().to_string();
    assert!(e.contains("not available on this phone"), "{e}");

    // A phone that saves and restores keeps its room.
    let saved = p.phone.lock().await.save().unwrap();
    let identity = Arc::new(DeviceIdentity::generate().unwrap());
    assert!(Phone::restore(identity, &saved, h.clock.now()).is_ok());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn paired_phone_answers_its_owners_approval() {
    let (h, rid, mut p) = room_with_phone(true).await;
    let a = 0;
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join(".fake-script.json"),
        json!({ "turns": [[ { "permission": { "kind": "execute", "title": "Run cargo publish", "command": "cargo publish" } } ]] })
            .to_string(),
    )
    .unwrap();
    std::fs::write(dir.path().join("polyph.toml"), "").unwrap();
    let r = h.nodes[a]
        .call("agent.start", json!({ "room": rid, "kind": "fake", "cwd": dir.path(), "preset": "default" }))
        .await
        .unwrap();
    let agent = r["id"].as_str().unwrap().to_string();
    let ad = h.nodes[a].daemon.clone();
    let agent2 = agent.clone();
    let prompt =
        tokio::spawn(async move { ad.call("agent.prompt", json!({ "agent": agent2, "text": "publish" })).await });

    let ev = p.wait_event("approval_pending", |_| true).await.unwrap();
    let id = ev["event"]["approval"]["id"].as_str().unwrap().to_string();
    // The phone sees the agent and the approval in its status, as Ann's own.
    let s = p.call("status", json!({})).await.unwrap();
    assert_eq!(s["approvals"][0]["title"], "Run cargo publish");
    assert_eq!(s["approvals"][0]["mine"], true);
    assert!(s["agents"].as_array().unwrap().iter().any(|x| x["id"] == agent.as_str()), "{s}");
    // Same member as the agent's owner: no escalation wait.
    p.call("approve", json!({ "room": rid, "id": id })).await.unwrap();
    let r = prompt.await.unwrap().map_err(|e| e.to_string()).unwrap();
    assert_eq!(r["stop_reason"], "end_turn");
    // The approval is credited in the activity feed on the daemon.
    h.until("activity shows the approval", || async {
        let v = h.nodes[a].call("activity.list", json!({ "room": rid })).await.unwrap();
        v.as_array().unwrap().iter().any(|e| e["kind"] == "approval" && e["args"]["approved"] == true)
    })
    .await
    .unwrap();
    // The phone keeps the transcript it saw.
    let t = p.call("agent.transcript", json!({ "room": rid, "agent": agent })).await.unwrap();
    assert!(!t["events"].as_array().unwrap().is_empty());
}

async fn connected(h: &Harness, rid: &str, want: bool) {
    h.until("connection state", || async {
        h.nodes[0].call("room.get", json!({ "room": rid })).await.unwrap()["connected"] == want
    })
    .await
    .unwrap();
}

/// The host's connection drops while a phone waits for approval: the phone
/// goes back for its knock and gets the answer Ann gave meanwhile.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_waiting_phone_gets_an_answer_given_while_the_host_was_away() {
    let mut h = Harness::new().await.unwrap();
    let a = h.add("Ann").await.unwrap();
    let room = h.nodes[a].call("room.create", json!({ "name": "payments" })).await.unwrap();
    let rid = room["id"].as_str().unwrap().to_string();
    connected(&h, &rid, true).await;
    let identity = Arc::new(DeviceIdentity::generate().unwrap());
    let phone = Phone::new(identity, None, "Pim's phone", &h.relay.url()).unwrap();
    let mut p = PhoneDriver::connect(phone, &h.relay.url(), h.clock.clone()).await.unwrap();
    let inv = h.nodes[a].call("room.invite", json!({ "room": rid })).await.unwrap();
    p.start_join(inv["code"].as_str().unwrap(), "Pim").await.unwrap();
    let knock = h.nodes[a].wait_event("knock", |e| e.kind() == "knock").await.unwrap();
    p.wait_event("join_pending", |_| true).await.unwrap();
    let kid = knock.event["knock"]["id"].as_str().unwrap().to_string();

    h.nodes[a].call("room.offline", json!({ "room": rid, "offline": true })).await.unwrap();
    connected(&h, &rid, false).await;
    h.nodes[a].call("knock.decide", json!({ "room": rid, "knock": kid, "approve": true })).await.unwrap();
    h.nodes[a].call("room.offline", json!({ "room": rid, "offline": false })).await.unwrap();

    // The phone waits between attempts on its own clock.
    let clock = h.clock.clone();
    let ticking = tokio::spawn(async move {
        loop {
            clock.advance(1);
            tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        }
    });
    let joined = p.wait_event("joined", |_| true).await.unwrap();
    ticking.abort();
    assert_eq!(joined["event"]["room"], rid.as_str());
}
