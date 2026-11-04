//! Milestone 4, hosted mode: one shared working copy on the host, mirrored to
//! members; members' changes reach it only under claims and on a current base.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::Path;

use polyph_testkit::harness::{AgentBins, Harness};
use serde_json::json;

fn bins() -> AgentBins {
    AgentBins {
        fake_agent: env!("CARGO_BIN_EXE_polyph-fake-agent").into(),
        sandbox_helper: env!("CARGO_BIN_EXE_polyph-test-sandbox-exec").into(),
        mcp: env!("CARGO_BIN_EXE_polyph-test-mcp").into(),
    }
}

fn read(p: &Path) -> Option<String> {
    std::fs::read_to_string(p).ok()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn members_work_on_the_hosts_working_copy() {
    let mut h = Harness::new().await.unwrap();
    h.agents = Some(bins());
    let (a, b) = (h.add("Ann").await.unwrap(), h.add("Bo").await.unwrap());
    let room = h.nodes[a].call("room.create", json!({ "name": "hosted" })).await.unwrap();
    let rid = room["id"].as_str().unwrap().to_string();
    h.until("connected", || async {
        h.nodes[a].call("room.get", json!({ "room": rid })).await.unwrap()["connected"] == true
    })
    .await
    .unwrap();
    h.join(&rid, a, b, a, "member").await.unwrap();

    // Ann's working copy, with a file too large to mirror and build output.
    let host = tempfile::tempdir().unwrap();
    let hp = host.path();
    std::fs::create_dir_all(hp.join("src")).unwrap();
    std::fs::create_dir_all(hp.join("target")).unwrap();
    std::fs::write(hp.join("src/lib.rs"), "pub fn a() {}\n").unwrap();
    std::fs::write(hp.join("README.md"), "# Hosted\n").unwrap();
    std::fs::write(hp.join("target/out.bin"), "build output").unwrap();
    std::fs::write(hp.join("big.bin"), vec![b'x'; 600 * 1024]).unwrap();
    let r = h.nodes[a].call("mirror.host", json!({ "room": rid, "dir": hp })).await.unwrap();
    assert_eq!(r["files"], 2);
    assert_eq!(r["too_large"], json!(["big.bin"]));

    // Bo's mirror fills from the host.
    let mirror = tempfile::tempdir().unwrap();
    let mp = mirror.path().join("hosted");
    h.until("hosted setting reaches Bo", || async {
        h.nodes[b].call("mirror.open", json!({ "room": rid, "dir": mp })).await.is_ok()
    })
    .await
    .unwrap();
    h.until("mirror filled", || async {
        read(&mp.join("src/lib.rs")).as_deref() == Some("pub fn a() {}\n") && read(&mp.join("README.md")).is_some()
    })
    .await
    .unwrap();
    assert!(!mp.join("target").exists() && !mp.join("big.bin").exists());

    // A change on the host reaches the mirror.
    std::fs::write(hp.join("src/lib.rs"), "pub fn a() {}\npub fn b() {}\n").unwrap();
    h.until("host change mirrored", || async {
        read(&mp.join("src/lib.rs")).as_deref() == Some("pub fn a() {}\npub fn b() {}\n")
    })
    .await
    .unwrap();

    // A change in the mirror reaches the host.
    std::fs::write(mp.join("NOTES.md"), "from Bo\n").unwrap();
    h.until("mirror change applied on the host", || async {
        read(&hp.join("NOTES.md")).as_deref() == Some("from Bo\n")
    })
    .await
    .unwrap();

    // Ann's agent holds src/lib.rs: Bo's edit there is rejected and undone.
    let script = tempfile::tempdir().unwrap();
    std::fs::write(
        script.path().join("a.json"),
        json!({ "turns": [[ { "write": { "path": "src/lib.rs", "content": "pub fn a() { 1 }\n" } } ]] }).to_string(),
    )
    .unwrap();
    std::fs::write(hp.join(".fake-script.json"), std::fs::read(script.path().join("a.json")).unwrap()).unwrap();
    let ann = h.nodes[a]
        .call("agent.start", json!({ "room": rid, "kind": "fake", "cwd": hp, "preset": "default" }))
        .await
        .unwrap();
    h.nodes[a].call("agent.prompt", json!({ "agent": ann["id"], "text": "edit" })).await.unwrap();
    h.until("Ann's edit mirrored", || async { read(&mp.join("src/lib.rs")).as_deref() == Some("pub fn a() { 1 }\n") })
        .await
        .unwrap();
    std::fs::write(mp.join("src/lib.rs"), "pub fn a() { 2 }\n").unwrap();
    h.until("Bo's conflicting edit undone", || async {
        read(&mp.join("src/lib.rs")).as_deref() == Some("pub fn a() { 1 }\n")
    })
    .await
    .unwrap();
    assert_eq!(read(&hp.join("src/lib.rs")).as_deref(), Some("pub fn a() { 1 }\n"));

    // Bo's agent works in the mirror; its new file lands on the host.
    std::fs::write(
        mp.join(".fake-script.json"),
        json!({ "turns": [[ { "write": { "path": "src/b.rs", "content": "pub fn from_bo() {}\n" } } ]] }).to_string(),
    )
    .unwrap();
    let bo = h.nodes[b]
        .call("agent.start", json!({ "room": rid, "kind": "fake", "cwd": mp, "preset": "default" }))
        .await
        .unwrap();
    h.nodes[b].call("agent.prompt", json!({ "agent": bo["id"], "text": "add b" })).await.unwrap();
    let t = h.nodes[b].call("agent.transcript", json!({ "room": rid, "agent": bo["id"] })).await.unwrap();
    let said: Vec<&str> = t["events"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|e| e["kind"]["t"] == "message")
        .filter_map(|e| e["kind"]["c"]["text"].as_str())
        .collect();
    assert!(said.iter().any(|m| m.contains("write: ok")), "{said:?}");
    assert_eq!(read(&mp.join("src/b.rs")).as_deref(), Some("pub fn from_bo() {}\n"));
    h.until("Bo's agent's file on the host", || async {
        read(&hp.join("src/b.rs")).as_deref() == Some("pub fn from_bo() {}\n")
    })
    .await
    .unwrap();
    h.until("the host confirms Bo's change", || async {
        let s = h.nodes[b].call("mirror.status", json!({ "room": rid })).await.unwrap();
        s["side"] == "member" && s["pending"] == 0 && s["files"] == 5
    })
    .await
    .unwrap();
}
