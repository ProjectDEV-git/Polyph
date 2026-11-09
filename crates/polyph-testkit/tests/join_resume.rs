//! Join codes outlive a host restart, and a joiner waiting for approval
//! survives a dropped connection.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use polyph_testkit::harness::Harness;
use serde_json::json;

async fn hosted_room(h: &mut Harness) -> (usize, usize, String) {
    let a = h.add("Ann").await.unwrap();
    let b = h.add("Bo").await.unwrap();
    let room = h.nodes[a].call("room.create", json!({ "name": "payments" })).await.unwrap();
    let rid = room["id"].as_str().unwrap().to_string();
    connected(h, a, &rid, true).await;
    (a, b, rid)
}

async fn connected(h: &Harness, i: usize, rid: &str, want: bool) {
    h.until("connection state", || async {
        h.nodes[i].call("room.get", json!({ "room": rid })).await.unwrap()["connected"] == want
    })
    .await
    .unwrap();
}

/// The reusable code `polyph start` prints keeps working after the host's
/// daemon restarts, and is kept sealed on disk.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_code_still_works_after_the_host_restarts() {
    let mut h = Harness::new().await.unwrap();
    let (a, b, rid) = hosted_room(&mut h).await;
    let inv = h.nodes[a].call("room.invite", json!({ "room": rid, "reusable": true })).await.unwrap();
    let code = inv["code"].as_str().unwrap().to_string();
    let file = h.nodes[a].daemon.shared.paths.room_codes_file(&rid);
    h.until("codes saved", || async { file.exists() }).await.unwrap();
    let saved = std::fs::read(&file).unwrap();
    let plain = code.replace('-', "");
    assert!(!saved.windows(plain.len()).any(|w| w == plain.as_bytes()), "the code is stored in the clear");

    h.restart(a).await.unwrap();
    connected(&h, a, &rid, true).await;
    // The relay forgot the code when the old daemon left; the new one hosts it.
    h.until("code hosted again", || async { h.relay.hub.stats().2 == 1 }).await.unwrap();

    let bd = h.nodes[b].daemon.clone();
    let mut joining =
        tokio::spawn(async move { bd.call("join", json!({ "code": code, "name": "Bo", "timeout_secs": 30 })).await });
    let knock = tokio::select! {
        k = h.nodes[a].wait_event("knock", |e| e.kind() == "knock") => k.unwrap(),
        r = &mut joining => panic!("join ended early: {:?}", r.unwrap().map_err(|e| e.to_string())),
    };
    let kid = knock.event["knock"]["id"].as_str().unwrap().to_string();
    h.nodes[a].call("knock.decide", json!({ "room": rid, "knock": kid, "approve": true })).await.unwrap();
    let joined = joining.await.unwrap().map_err(|e| e.to_string()).unwrap();
    assert_eq!(joined["id"], rid.as_str());
}

/// The host's connection drops while Bo waits: Bo reconnects with the same
/// one-time code, takes his knock back and gets the answer Ann gave while
/// the host was away.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_waiting_joiner_gets_an_answer_given_while_the_host_was_away() {
    let mut h = Harness::new().await.unwrap();
    let (a, b, rid) = hosted_room(&mut h).await;
    let inv = h.nodes[a].call("room.invite", json!({ "room": rid })).await.unwrap();
    let code = inv["code"].as_str().unwrap().to_string();
    let bd = h.nodes[b].daemon.clone();
    let joining =
        tokio::spawn(async move { bd.call("join", json!({ "code": code, "name": "Bo", "timeout_secs": 60 })).await });
    let knock = h.nodes[a].wait_event("knock", |e| e.kind() == "knock").await.unwrap();
    h.nodes[b].wait_event("pending", |e| e.kind() == "join_pending").await.unwrap();
    let kid = knock.event["knock"]["id"].as_str().unwrap().to_string();

    h.nodes[a].call("room.offline", json!({ "room": rid, "offline": true })).await.unwrap();
    connected(&h, a, &rid, false).await;
    h.nodes[a].call("knock.decide", json!({ "room": rid, "knock": kid, "approve": true })).await.unwrap();
    h.nodes[a].call("room.offline", json!({ "room": rid, "offline": false })).await.unwrap();

    let joined = joining.await.unwrap().map_err(|e| e.to_string()).unwrap();
    assert_eq!(joined["id"], rid.as_str());
}

/// The host's connection drops while Bo waits and comes back before anyone
/// answers: Bo's knock is still the one waiting, and approving it lets him in.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_waiting_joiner_keeps_its_knock_across_a_host_reconnect() {
    let mut h = Harness::new().await.unwrap();
    let (a, b, rid) = hosted_room(&mut h).await;
    let inv = h.nodes[a].call("room.invite", json!({ "room": rid })).await.unwrap();
    let code = inv["code"].as_str().unwrap().to_string();
    let bd = h.nodes[b].daemon.clone();
    let joining =
        tokio::spawn(async move { bd.call("join", json!({ "code": code, "name": "Bo", "timeout_secs": 60 })).await });
    let knock = h.nodes[a].wait_event("knock", |e| e.kind() == "knock").await.unwrap();
    h.nodes[b].wait_event("pending", |e| e.kind() == "join_pending").await.unwrap();
    let kid = knock.event["knock"]["id"].as_str().unwrap().to_string();

    h.nodes[a].call("room.offline", json!({ "room": rid, "offline": true })).await.unwrap();
    connected(&h, a, &rid, false).await;
    h.nodes[a].call("room.offline", json!({ "room": rid, "offline": false })).await.unwrap();
    connected(&h, a, &rid, true).await;
    // Bo is back on his knock: a second "pending" answer reaches him.
    h.nodes[b].wait_event("pending again", |e| e.kind() == "join_pending").await.unwrap();

    let listed = h.nodes[a].call("knock.list", json!({ "room": rid })).await.unwrap();
    let ids: Vec<&str> = listed.as_array().unwrap().iter().filter_map(|k| k["id"].as_str()).collect();
    assert_eq!(ids, vec![kid.as_str()], "one knock, the original");
    h.nodes[a].call("knock.decide", json!({ "room": rid, "knock": kid, "approve": true })).await.unwrap();
    let joined = joining.await.unwrap().map_err(|e| e.to_string()).unwrap();
    assert_eq!(joined["id"], rid.as_str());
}

/// A one-time code Bo used is held for Bo alone while he reconnects: Chai
/// cannot use it, and Bo, back later, takes his knock back and gets in.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_held_one_time_code_lets_only_its_joiner_back() {
    let mut h = Harness::new().await.unwrap();
    let (a, b, rid) = hosted_room(&mut h).await;
    let c = h.add("Chai").await.unwrap();
    let inv = h.nodes[a].call("room.invite", json!({ "room": rid })).await.unwrap();
    let code = inv["code"].as_str().unwrap().to_string();
    let bd = h.nodes[b].daemon.clone();
    let params = json!({ "code": code, "name": "Bo", "timeout_secs": 60 });
    let first = tokio::spawn({
        let (bd, params) = (bd.clone(), params.clone());
        async move { bd.call("join", params).await }
    });
    let knock = h.nodes[a].wait_event("knock", |e| e.kind() == "knock").await.unwrap();
    h.nodes[b].wait_event("pending", |e| e.kind() == "join_pending").await.unwrap();
    let kid = knock.event["knock"]["id"].as_str().unwrap().to_string();

    // The host drops, and Bo gives up before it is back.
    h.nodes[a].call("room.offline", json!({ "room": rid, "offline": true })).await.unwrap();
    connected(&h, a, &rid, false).await;
    first.abort();
    h.nodes[a].call("room.offline", json!({ "room": rid, "offline": false })).await.unwrap();
    connected(&h, a, &rid, true).await;

    // The host hosts the held code again as it reconnects; Chai may beat it.
    let e = h
        .until_value("Chai refused", || async {
            let e = h.nodes[c]
                .call("join", json!({ "code": code, "name": "Chai", "timeout_secs": 20 }))
                .await
                .unwrap_err()
                .to_string();
            (!e.contains("no room is waiting")).then_some(e)
        })
        .await
        .unwrap();
    assert!(e.contains("already used"), "{e}");

    let joining = tokio::spawn(async move { bd.call("join", params).await });
    h.nodes[b].wait_event("pending again", |e| e.kind() == "join_pending").await.unwrap();
    h.nodes[a].call("knock.decide", json!({ "room": rid, "knock": kid, "approve": true })).await.unwrap();
    let joined = joining.await.unwrap().map_err(|e| e.to_string()).unwrap();
    assert_eq!(joined["id"], rid.as_str());
}
