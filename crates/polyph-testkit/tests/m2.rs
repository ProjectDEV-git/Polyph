//! Milestone 2 acceptance tests: real daemons talking through a real relay.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use polyph_testkit::harness::Harness;
use serde_json::json;

async fn chat_texts(h: &Harness, i: usize, room: &str) -> Vec<String> {
    let v = h.nodes[i].call("chat.list", json!({ "room": room })).await.unwrap();
    v.as_array().unwrap().iter().map(|e| e["text"].as_str().unwrap().to_string()).collect()
}

async fn host_of(h: &Harness, i: usize, room: &str) -> Option<String> {
    let _ = room;
    let s = h.nodes[i].daemon.status().await;
    s.room
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn three_daemons_share_a_room_through_the_relay() {
    let mut h = Harness::new().await.unwrap();
    let a = h.add("Ann").await.unwrap();
    let b = h.add("Bo").await.unwrap();
    let c = h.add("Chai").await.unwrap();
    let room = h.nodes[a].call("room.create", json!({ "name": "payments" })).await.unwrap();
    let rid = room["id"].as_str().unwrap().to_string();
    h.until("owner connected", || async {
        h.nodes[a].call("room.get", json!({ "room": rid })).await.unwrap()["connected"] == true
    })
    .await
    .unwrap();

    let rb = h.join(&rid, a, b, a, "member").await.unwrap();
    assert_eq!(rb, rid);
    // Bo invites Chai; Ann approves the knock.
    let rc = h.join(&rid, b, c, a, "member").await.unwrap();
    assert_eq!(rc, rid);

    h.nodes[c].call("chat.send", json!({ "room": rid, "text": "สวัสดีครับ — hello from Chai" })).await.unwrap();
    h.nodes[a].call("chat.send", json!({ "room": rid, "text": "welcome" })).await.unwrap();
    for i in [a, b, c] {
        h.until("chat converged", || async { chat_texts(&h, i, &rid).await.len() == 2 }).await.unwrap();
    }
    assert_eq!(chat_texts(&h, a, &rid).await, chat_texts(&h, c, &rid).await);

    // Board edits converge too.
    let card = h.nodes[b].call("board.add", json!({ "room": rid, "title": "Add OAuth login" })).await.unwrap();
    let card_id = card["id"].as_str().unwrap().to_string();
    h.until("card reached Chai", || async {
        let b = h.nodes[c].call("board.get", json!({ "room": rid })).await.unwrap();
        b["cards"].as_array().unwrap().iter().any(|x| x["id"] == card_id.as_str())
    })
    .await
    .unwrap();
    h.nodes[c]
        .call("board.move", json!({ "room": rid, "card": card_id.clone(), "column": "in_progress" }))
        .await
        .unwrap();
    for i in [a, b, c] {
        h.until("card moved", || async {
            let b = h.nodes[i].call("board.get", json!({ "room": rid })).await.unwrap();
            b["cards"].as_array().unwrap().iter().any(|c| c["id"] == card_id.as_str() && c["column"] == "in_progress")
        })
        .await
        .unwrap();
    }

    let members = h.nodes[a].call("room.members", json!({ "room": rid })).await.unwrap();
    let names: Vec<&str> = members.as_array().unwrap().iter().map(|m| m["name"].as_str().unwrap()).collect();
    assert!(names.contains(&"Bo") && names.contains(&"Chai"), "{names:?}");
    assert!(host_of(&h, a, &rid).await.is_some());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn three_daemons_share_a_lan_room_without_internet() {
    let mut h = Harness::new().await.unwrap();
    let a = h.add("Ann").await.unwrap();
    let b = h.add("Bo").await.unwrap();
    let c = h.add("Chai").await.unwrap();
    // Point everyone's default relay at an address nothing listens on, so
    // only the LAN relay can work.
    for i in [a, b, c] {
        h.nodes[i].call("config.set", json!({ "key": "relay", "value": "ws://127.0.0.1:9/ws" })).await.unwrap();
    }
    let room = h.nodes[a]
        .call("room.create", json!({ "name": "hackathon", "lan": true, "require_approval": false }))
        .await
        .unwrap();
    let rid = room["id"].as_str().unwrap().to_string();
    let lan = room["lan"].as_str().unwrap().to_string();
    assert!(lan.starts_with("wss://127.0.0.1:"), "{lan}");
    let lan_url = lan.split('#').next().unwrap().to_string();
    h.until("owner connected to LAN relay", || async {
        h.nodes[a].call("room.get", json!({ "room": rid })).await.unwrap()["connected"] == true
    })
    .await
    .unwrap();

    for j in [b, c] {
        let inv = h.nodes[a].call("room.invite", json!({ "room": rid })).await.unwrap();
        // The joiner knows only the code and the LAN address (as from discovery),
        // not the certificate fingerprint; it is bound into the PAKE instead.
        let r = h.nodes[j]
            .call("join", json!({ "code": inv["code"], "relay": lan_url, "lan": true, "timeout_secs": 10 }))
            .await
            .unwrap();
        assert_eq!(r["id"], rid.as_str());
    }
    h.nodes[b].call("chat.send", json!({ "room": rid, "text": "offline and fine" })).await.unwrap();
    for i in [a, b, c] {
        h.until("LAN chat", || async { chat_texts(&h, i, &rid).await == vec!["offline and fine".to_string()] })
            .await
            .unwrap();
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn wrong_code_on_a_lan_relay_fails() {
    let mut h = Harness::new().await.unwrap();
    let a = h.add("Ann").await.unwrap();
    let b = h.add("Bo").await.unwrap();
    let room =
        h.nodes[a].call("room.create", json!({ "name": "lan", "lan": true, "require_approval": false })).await.unwrap();
    let rid = room["id"].as_str().unwrap().to_string();
    let lan_url = room["lan"].as_str().unwrap().split('#').next().unwrap().to_string();
    h.until("connected", || async {
        h.nodes[a].call("room.get", json!({ "room": rid })).await.unwrap()["connected"] == true
    })
    .await
    .unwrap();
    let inv = h.nodes[a].call("room.invite", json!({ "room": rid })).await.unwrap();
    let code = inv["code"].as_str().unwrap();
    let last = code.chars().last().unwrap();
    let bad = format!("{}{}", &code[..code.len() - 1], if last == '0' { '1' } else { '0' });
    let r = h.nodes[b].call("join", json!({ "code": bad, "relay": lan_url, "lan": true, "timeout_secs": 5 })).await;
    assert!(r.is_err());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn room_survives_everyone_leaving_and_a_member_catches_up() {
    // A one-envelope replay buffer forces the returning member onto the
    // encrypted snapshot instead of the relay's short-term buffer.
    let mut cfg = polyph_relay::RelayConfig::ephemeral();
    cfg.buffer_max_envelopes = 1;
    let mut h = Harness::with_relay(cfg).await.unwrap();
    let a = h.add("Ann").await.unwrap();
    let b = h.add("Bo").await.unwrap();
    let room = h.nodes[a].call("room.create", json!({ "name": "persist" })).await.unwrap();
    let rid = room["id"].as_str().unwrap().to_string();
    h.until("connected", || async {
        h.nodes[a].call("room.get", json!({ "room": rid })).await.unwrap()["connected"] == true
    })
    .await
    .unwrap();
    h.join(&rid, a, b, a, "member").await.unwrap();

    // Bo leaves. Ann keeps working, then leaves too (closing uploads a snapshot).
    h.nodes[b].call("room.close", json!({ "room": rid })).await.unwrap();
    h.nodes[a].call("chat.send", json!({ "room": rid, "text": "decided: use Postgres" })).await.unwrap();
    h.nodes[a].call("board.add", json!({ "room": rid, "title": "Write migration" })).await.unwrap();
    h.nodes[a].call("room.close", json!({ "room": rid })).await.unwrap();

    // The next day Bo's daemon restarts and reopens the room with nobody online.
    h.clock.advance(86_400);
    h.restart(b).await.unwrap();
    h.nodes[b].call("room.open", json!({ "room": rid })).await.unwrap();
    h.until("Bo caught up from the snapshot", || async {
        chat_texts(&h, b, &rid).await.contains(&"decided: use Postgres".to_string())
    })
    .await
    .unwrap();
    h.until("board caught up", || async {
        let board = h.nodes[b].call("board.get", json!({ "room": rid })).await.unwrap();
        board["cards"].as_array().unwrap().iter().any(|c| c["title"] == "Write migration")
    })
    .await
    .unwrap();
    let info = h.nodes[b].call("room.get", json!({ "room": rid })).await.unwrap();
    assert_eq!(info["name"], "persist");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn viewer_cannot_approve_and_cannot_chat() {
    let mut h = Harness::new().await.unwrap();
    let a = h.add("Ann").await.unwrap();
    let v = h.add("Vee").await.unwrap();
    let room = h.nodes[a].call("room.create", json!({ "name": "review" })).await.unwrap();
    let rid = room["id"].as_str().unwrap().to_string();
    h.until("connected", || async {
        h.nodes[a].call("room.get", json!({ "room": rid })).await.unwrap()["connected"] == true
    })
    .await
    .unwrap();
    h.join(&rid, a, v, a, "viewer").await.unwrap();
    let info = h.nodes[v].call("room.get", json!({ "room": rid })).await.unwrap();
    assert_eq!(info["role"], "viewer");
    assert!(h.nodes[v].call("chat.send", json!({ "room": rid, "text": "hi" })).await.is_err());
    assert!(h.nodes[v].call("room.invite", json!({ "room": rid })).await.is_err());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn acting_host_fails_over() {
    let mut h = Harness::new().await.unwrap();
    let a = h.add("Ann").await.unwrap();
    let b = h.add("Bo").await.unwrap();
    let c = h.add("Chai").await.unwrap();
    let room = h.nodes[a].call("room.create", json!({ "name": "failover" })).await.unwrap();
    let rid = room["id"].as_str().unwrap().to_string();
    h.until("connected", || async {
        h.nodes[a].call("room.get", json!({ "room": rid })).await.unwrap()["connected"] == true
    })
    .await
    .unwrap();
    h.join(&rid, a, b, a, "maintainer").await.unwrap();
    h.join(&rid, a, c, a, "member").await.unwrap();
    h.advance(20).await;

    // Ann (owner, acting host) goes offline. Bo, the maintainer, takes over.
    h.nodes[a].call("room.offline", json!({ "room": rid, "offline": true })).await.unwrap();
    h.nodes[c].wait_event("Bo becomes acting host", |e| e.kind() == "acting_host" && e.event["me"] == false).await.ok();
    h.advance(50).await;
    h.nodes[b].wait_event("acting host is me", |e| e.kind() == "acting_host" && e.event["me"] == true).await.unwrap();

    // Claims still work through the new host.
    let claims_before = h.nodes[c].call("claims.list", json!({ "room": rid })).await.unwrap();
    assert!(claims_before.as_array().unwrap().is_empty());

    // Ann returns and takes the role back.
    h.nodes[a].call("room.offline", json!({ "room": rid, "offline": false })).await.unwrap();
    h.advance(30).await;
    h.nodes[a].wait_event("Ann host again", |e| e.kind() == "acting_host" && e.event["me"] == true).await.unwrap();
}

/// Only codes made for another device of this member (`phone.pair`,
/// `host.invite`) let a joiner claim the inviter's member id, and never
/// without a person approving it, even with approval off.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn only_pairing_codes_let_a_device_join_as_the_inviter() {
    let mut h = Harness::new().await.unwrap();
    let a = h.add("Ann").await.unwrap();
    let eve = h.add("Eve").await.unwrap();
    let server = h.add("server").await.unwrap();
    let room = h.nodes[a].call("room.create", json!({ "name": "payments", "require_approval": false })).await.unwrap();
    let rid = room["id"].as_str().unwrap().to_string();
    h.until("connected", || async {
        h.nodes[a].call("room.get", json!({ "room": rid })).await.unwrap()["connected"] == true
    })
    .await
    .unwrap();
    let ann = h.nodes[a].call("whoami", json!({})).await.unwrap()["member"].as_str().unwrap().to_string();
    let dir = tempfile::tempdir().unwrap();

    // A plain invite: Eve claims to be Ann. Not let in automatically, and
    // refused when Ann tries to approve it.
    let inv = h.nodes[a].call("room.invite", json!({ "room": rid })).await.unwrap();
    let ed = h.nodes[eve].daemon.clone();
    let params =
        json!({ "code": inv["code"], "member": ann, "relay": h.relay.url(), "workdir": dir.path(), "name": "Ann" });
    let joining = tokio::spawn(async move { ed.call("host.join", params).await });
    let knock = h.nodes[a].wait_event("knock", |e| e.kind() == "knock").await.unwrap();
    assert_eq!(knock.event["knock"]["member"], ann.as_str());
    assert_eq!(knock.event["knock"]["own_device"], false);
    h.nodes[eve].wait_event("pending", |e| e.kind() == "join_pending").await.unwrap();
    let kid = knock.event["knock"]["id"].as_str().unwrap().to_string();
    let e = h.nodes[a]
        .call("knock.decide", json!({ "room": rid, "knock": kid, "approve": true }))
        .await
        .unwrap_err()
        .to_string();
    assert!(e.contains("existing member"), "{e}");
    h.nodes[a].call("knock.decide", json!({ "room": rid, "knock": kid, "approve": false })).await.unwrap();
    assert!(joining.await.unwrap().is_err());

    // The same through `host.invite`: it waits for Ann, then gets in.
    let inv = h.nodes[a].call("host.invite", json!({ "room": rid })).await.unwrap();
    assert_eq!(inv["member"], ann.as_str());
    let sd = h.nodes[server].daemon.clone();
    let params = json!({ "code": inv["code"], "member": inv["member"], "relay": inv["relay"], "workdir": dir.path(), "name": "Ann" });
    let joining = tokio::spawn(async move { sd.call("host.join", params).await });
    let knock = h.nodes[a].wait_event("knock", |e| e.kind() == "knock").await.unwrap();
    assert_eq!(knock.event["knock"]["own_device"], true);
    h.nodes[server].wait_event("pending", |e| e.kind() == "join_pending").await.unwrap();
    let listed = h.nodes[a].call("knock.list", json!({ "room": rid })).await.unwrap();
    assert_eq!(listed[0]["own_device"], true);
    let kid = knock.event["knock"]["id"].as_str().unwrap().to_string();
    h.nodes[a].call("knock.decide", json!({ "room": rid, "knock": kid, "approve": true })).await.unwrap();
    let joined = joining.await.unwrap().map_err(|e| e.to_string()).unwrap();
    assert_eq!(joined["id"], rid.as_str());
}

/// Only an approver's own device can let in a knock naming its member, so a
/// Member is told up front that it cannot pair a phone or add an agent host.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn members_are_told_they_cannot_add_their_own_devices() {
    let mut h = Harness::new().await.unwrap();
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
    h.nodes[b].call("config.set", json!({ "key": "lang", "value": "en" })).await.unwrap();
    for method in ["phone.pair", "host.invite"] {
        let e = h.nodes[b].call(method, json!({ "room": rid })).await.unwrap_err().to_string();
        assert!(e.contains("Only an Owner or Maintainer can add another device"), "{method}: {e}");
    }
    // The Owner still can.
    for method in ["phone.pair", "host.invite"] {
        h.nodes[a].call(method, json!({ "room": rid })).await.unwrap();
    }
}
