//! Milestone 9: agent hosts. Ann's server joins her room as another device
//! of hers with the `agent-host` capability; her laptop starts, prompts,
//! approves and stops an agent running there, and a teammate cannot use it.
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

async fn messages(h: &Harness, i: usize, rid: &str, agent: &str) -> Vec<String> {
    let Ok(t) = h.nodes[i].call("agent.transcript", json!({ "room": rid, "agent": agent })).await else {
        return vec![];
    };
    t["events"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|e| e["kind"]["t"] == "message")
        .flat_map(|e| e["kind"]["c"]["text"].as_str().unwrap().lines().map(str::to_string).collect::<Vec<_>>())
        .collect()
}

/// Ann (laptop), her server as an agent host, and Bo. Returns the room id.
async fn setup(workdir: &std::path::Path) -> (Harness, String) {
    let mut h = Harness::new().await.unwrap();
    h.agents = Some(bins());
    let (a, s, b) = (h.add("Ann").await.unwrap(), h.add("server").await.unwrap(), h.add("Bo").await.unwrap());
    let room = h.nodes[a].call("room.create", json!({ "name": "payments" })).await.unwrap();
    let rid = room["id"].as_str().unwrap().to_string();
    h.until("connected", || async {
        h.nodes[a].call("room.get", json!({ "room": rid })).await.unwrap()["connected"] == true
    })
    .await
    .unwrap();
    h.join(&rid, a, b, a, "member").await.unwrap();

    // Enroll the server: an invite from the laptop, joined on the server.
    let inv = h.nodes[a].call("host.invite", json!({ "room": rid })).await.unwrap();
    assert!(inv["command"].as_str().unwrap().starts_with("polyph agent-host join "));
    let sd = h.nodes[s].daemon.clone();
    let params = json!({
        "code": inv["code"], "member": inv["member"], "relay": inv["relay"],
        "workdir": workdir, "name": "Ann",
    });
    let joining = tokio::spawn(async move { sd.call("host.join", params).await });
    let knock = h.nodes[a].wait_event("knock", |e| e.kind() == "knock").await.unwrap();
    assert_eq!(knock.event["knock"]["capabilities"], json!(["agent-host"]));
    assert_eq!(knock.event["knock"]["member"], inv["member"]);
    let kid = knock.event["knock"]["id"].as_str().unwrap().to_string();
    h.nodes[a].call("knock.decide", json!({ "room": rid, "knock": kid, "approve": true })).await.unwrap();
    joining.await.unwrap().map_err(|e| e.to_string()).unwrap();
    (h, rid)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn laptop_runs_an_agent_on_its_agent_host() {
    let dir = tempfile::tempdir().unwrap();
    let script = json!({ "turns": [
        [ { "message": "on the server" } ],
        [ { "permission": { "kind": "execute", "title": "Run cargo publish", "command": "cargo publish" } } ]
    ] });
    std::fs::write(dir.path().join(".fake-script.json"), script.to_string()).unwrap();
    std::fs::write(dir.path().join("polyph.toml"), "").unwrap();
    let (mut h, rid) = setup(dir.path()).await;
    let (a, s, b) = (0, 1, 2);

    // The laptop sees its host online.
    let hosts: Value = h
        .until_value("host online", || async {
            let v = h.nodes[a].call("agent.hosts", json!({ "room": rid })).await.unwrap();
            (v.as_array().unwrap().iter().any(|x| x["online"] == true)).then_some(v)
        })
        .await
        .unwrap();
    let host_dev = hosts[0]["device"].as_str().unwrap().to_string();
    assert_eq!(h.nodes[s].call("whoami", json!({})).await.unwrap()["device"], host_dev.as_str());
    // Bo has no host of his own, and cannot use Ann's.
    assert!(h.nodes[b].call("agent.hosts", json!({ "room": rid })).await.unwrap().as_array().unwrap().is_empty());
    let e = h.nodes[b]
        .call("agent.start", json!({ "room": rid, "kind": "fake", "on": &host_dev[..8] }))
        .await
        .unwrap_err()
        .to_string();
    assert!(e.contains("no agent host"), "{e}");

    // Start on the host, with a first prompt.
    let r = h.nodes[a]
        .call("agent.start", json!({ "room": rid, "kind": "fake", "on": "cloud", "prompt": "hello" }))
        .await
        .unwrap();
    let id = r["id"].as_str().unwrap().to_string();
    assert_eq!(r["host"], host_dev.as_str());
    assert_eq!(r["label"], "Ann/fake");
    // It runs on the server, not the laptop, and the laptop follows it.
    assert!(h.nodes[s].daemon.agents.map.lock().await.keys().any(|k| k.to_hex() == id));
    assert!(h.nodes[a].daemon.agents.map.lock().await.is_empty());
    h.until("first turn seen on the laptop", || async {
        messages(&h, a, &rid, &id).await.contains(&"on the server".to_string())
    })
    .await
    .unwrap();
    let st = h.nodes[a].daemon.status().await;
    let ag = st.agents.iter().find(|x| x.id == id).unwrap();
    assert!(ag.cloud, "agents on a host are marked as cloud agents");

    // Prompt it from the laptop; its approval comes to Ann and she answers.
    let fw = h.nodes[a].call("agent.prompt", json!({ "agent": id, "text": "publish" })).await.unwrap();
    assert_eq!(fw["forwarded"], true);
    let ev = h.nodes[a].wait_event("approval", |e| e.kind() == "approval_pending").await.unwrap();
    let ap = ev.event["approval"]["id"].as_str().unwrap().to_string();
    h.nodes[a].call("approve", json!({ "room": rid, "id": ap })).await.unwrap();
    h.until("approved on the server", || async {
        messages(&h, a, &rid, &id).await.contains(&"permission: allowed".to_string())
    })
    .await
    .unwrap();

    // Stop it from the laptop.
    h.nodes[a].call("agent.stop", json!({ "agent": id })).await.unwrap();
    h.until("stopped on the server", || async { h.nodes[s].daemon.agents.map.lock().await.is_empty() }).await.unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn host_survives_a_restart_and_needs_a_workdir() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join(".fake-script.json"), json!({ "turns": [] }).to_string()).unwrap();
    let (mut h, rid) = setup(dir.path()).await;
    let (a, s) = (0, 1);
    // After a restart the server is still a host (its certificate says so).
    h.restart(s).await.unwrap();
    h.until("host back online", || async {
        let v = h.nodes[a].call("agent.hosts", json!({ "room": rid })).await.unwrap();
        v.as_array().unwrap().iter().any(|x| x["online"] == true)
    })
    .await
    .unwrap();
    // Without a working directory it refuses, and says how to fix it. The
    // laptop can still show the server's presence from before the restart and
    // then briefly see it go away, so retry while the host reads as offline.
    h.nodes[s].daemon.shared.config.lock().host_workdir = None;
    let e = h
        .until_value("the host answers a start", || async {
            let e = h.nodes[a]
                .call("agent.start", json!({ "room": rid, "kind": "fake", "on": "cloud" }))
                .await
                .unwrap_err()
                .to_string();
            let unreachable = e.contains("no agent host of yours matches") || e.contains("is offline");
            (!unreachable).then_some(e)
        })
        .await
        .unwrap();
    assert!(e.contains("polyph agent-host workdir"), "{e}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn phone_starts_an_agent_on_the_host_with_the_laptop_closed() {
    use polyph_phone::Phone;
    use polyph_proto::identity::DeviceIdentity;
    use polyph_proto::ids::MemberId;
    use polyph_testkit::phone::PhoneDriver;
    use std::sync::Arc;

    let dir = tempfile::tempdir().unwrap();
    let script = json!({ "turns": [[ { "message": "started from a phone" } ]] });
    std::fs::write(dir.path().join(".fake-script.json"), script.to_string()).unwrap();
    let (mut h, rid) = setup(dir.path()).await;
    let (a, s) = (0, 1);

    // Ann pairs her phone (as her own member).
    let who = h.nodes[a].call("whoami", json!({})).await.unwrap();
    let member = MemberId::from_hex(who["member"].as_str().unwrap()).unwrap();
    let phone = Phone::new(Arc::new(DeviceIdentity::generate().unwrap()), Some(member), "Ann", &h.relay.url()).unwrap();
    let mut p = PhoneDriver::connect(phone, &h.relay.url(), h.clock.clone()).await.unwrap();
    let inv = h.nodes[a].call("room.invite", json!({ "room": rid })).await.unwrap();
    p.start_join(inv["code"].as_str().unwrap(), "Ann").await.unwrap();
    let knock = h.nodes[a].wait_event("knock", |e| e.kind() == "knock").await.unwrap();
    let kid = knock.event["knock"]["id"].as_str().unwrap().to_string();
    h.nodes[a].call("knock.decide", json!({ "room": rid, "knock": kid, "approve": true })).await.unwrap();
    p.wait_event("joined", |_| true).await.unwrap();

    // The laptop closes.
    h.nodes[a].daemon.stop().await;

    h.until("phone sees the host", || async {
        let v = p.call("agent.hosts", json!({ "room": rid })).await.unwrap();
        v.as_array().unwrap().iter().any(|x| x["online"] == true)
    })
    .await
    .unwrap();
    let r = p.call("agent.start", json!({ "room": rid, "kind": "fake", "on": "cloud", "prompt": "go" })).await.unwrap();
    assert_eq!(r["pending"], true);
    let id: String = h
        .until_value("agent appears on the phone", || async {
            let v = p.call("agent.list", json!({ "room": rid })).await.unwrap();
            v.as_array().unwrap().first().and_then(|x| x["id"].as_str().map(str::to_string))
        })
        .await
        .unwrap();
    assert!(h.nodes[s].daemon.agents.map.lock().await.keys().any(|k| k.to_hex() == id));
    h.until("its output reaches the phone", || async {
        let t = p.call("agent.transcript", json!({ "room": rid, "agent": id })).await.unwrap();
        t.to_string().contains("started from a phone")
    })
    .await
    .unwrap();
    let st = p.call("status", json!({})).await.unwrap();
    assert!(st["agents"].as_array().unwrap().iter().any(|x| x["id"] == id.as_str() && x["cloud"] == true));

    p.call("agent.stop", json!({ "agent": id })).await.unwrap();
    h.until("stopped on the server", || async { h.nodes[s].daemon.agents.map.lock().await.is_empty() }).await.unwrap();
}
