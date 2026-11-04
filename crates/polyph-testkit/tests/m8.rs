//! Milestone 8: linking another device (a phone) to a person already in the
//! room, and refusing joins that would take over someone else's identity.
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

/// Ann (owner), Bo (member) and a third, not yet joined node.
async fn room() -> (Harness, String) {
    let mut h = Harness::new().await.unwrap();
    h.agents = Some(bins());
    let a = h.add("Ann").await.unwrap();
    let b = h.add("Bo").await.unwrap();
    h.add("Third").await.unwrap();
    let room = h.nodes[a].call("room.create", json!({ "name": "phones" })).await.unwrap();
    let rid = room["id"].as_str().unwrap().to_string();
    h.until("connected", || async {
        h.nodes[a].call("room.get", json!({ "room": rid })).await.unwrap()["connected"] == true
    })
    .await
    .unwrap();
    h.join(&rid, a, b, a, "member").await.unwrap();
    (h, rid)
}

async fn my_member(h: &Harness, i: usize, rid: &str) -> String {
    let m = h.nodes[i].call("room.members", json!({ "room": rid })).await.unwrap();
    m.as_array().unwrap().iter().find(|m| m["me"] == true).unwrap()["member"].as_str().unwrap().to_string()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_join_code_cannot_take_an_existing_members_identity() {
    let (h, rid) = room().await;
    let ann = my_member(&h, 0, &rid).await;
    let inv = h.nodes[0].call("room.invite", json!({ "room": rid })).await.unwrap();
    let err = h.nodes[2]
        .call("join", json!({ "code": inv["code"], "member": ann, "timeout_secs": 10 }))
        .await
        .expect_err("joining as Ann must fail");
    assert!(err.to_string().contains("already in the room"), "{err}");

    // A link code links only the device of the person who made it.
    let link = h.nodes[1].call("room.link", json!({ "room": rid })).await.unwrap();
    let err = h.nodes[2]
        .call("join", json!({ "code": link["code"], "member": ann, "timeout_secs": 10 }))
        .await
        .expect_err("Bo's link code cannot link a device to Ann");
    assert!(err.to_string().contains("links another device"), "{err}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_linked_device_answers_its_persons_approvals() {
    let (mut h, rid) = room().await;
    let (a, b, phone) = (0, 1, 2);
    let bo = my_member(&h, b, &rid).await;

    // Bo links a second device. Bo cannot admit people, so Ann approves the knock.
    let link = h.nodes[b].call("room.link", json!({ "room": rid })).await.unwrap();
    assert_eq!(link["member"], bo);
    let pd = h.nodes[phone].daemon.clone();
    let code = link["code"].clone();
    let bo2 = bo.clone();
    let join = tokio::spawn(async move {
        pd.call("join", json!({ "code": code, "member": bo2, "name": "Bo", "timeout_secs": 20 })).await
    });
    let knock = match h.nodes[a].wait_event("knock", |e| e.kind() == "knock").await {
        Ok(k) => k,
        Err(e) => panic!("{e}; join: {:?}", join.await.map(|r| r.map_err(|e| e.to_string()))),
    };
    assert_eq!(knock.event["knock"]["link"], true);
    assert_eq!(knock.event["knock"]["member"], bo);
    let kid = knock.event["knock"]["id"].as_str().unwrap().to_string();
    h.nodes[a].call("knock.decide", json!({ "room": rid, "knock": kid, "approve": true })).await.unwrap();
    join.await.unwrap().map_err(|e| e.to_string()).unwrap();
    assert_eq!(my_member(&h, phone, &rid).await, bo, "the phone is Bo");
    h.until("Bo shows two devices", || async {
        let m = h.nodes[a].call("room.members", json!({ "room": rid })).await.unwrap();
        m.as_array().unwrap().iter().any(|m| m["member"] == bo.as_str() && m["devices"] == 2)
    })
    .await
    .unwrap();

    // Bo's agent asks; Bo's phone answers at once, without waiting for escalation.
    let dir = tempfile::tempdir().unwrap();
    let script = json!({ "turns": [[ { "permission": { "kind": "execute", "title": "Run cargo publish", "command": "cargo publish" } } ]] });
    std::fs::write(dir.path().join(".fake-script.json"), script.to_string()).unwrap();
    std::fs::write(dir.path().join("polyph.toml"), "").unwrap();
    let r = h.nodes[b]
        .call("agent.start", json!({ "room": rid, "kind": "fake", "cwd": dir.path(), "preset": "default" }))
        .await
        .unwrap();
    let id = r["id"].as_str().unwrap().to_string();
    let bd = h.nodes[b].daemon.clone();
    let id2 = id.clone();
    let prompt = tokio::spawn(async move { bd.call("agent.prompt", json!({ "agent": id2, "text": "publish" })).await });
    let ev = h.nodes[phone].wait_event("approval on the phone", |e| e.kind() == "approval_pending").await.unwrap();
    let approval = ev.event["approval"]["id"].as_str().unwrap().to_string();
    h.nodes[phone].call("approve", json!({ "room": rid, "id": approval })).await.unwrap();
    let r: Value = prompt.await.unwrap().map_err(|e| e.to_string()).unwrap();
    assert_eq!(r["stop_reason"], "end_turn");
}
