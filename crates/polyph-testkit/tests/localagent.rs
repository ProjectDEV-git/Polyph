//! polyph-localagent against a scripted model, through a real daemon, the
//! sandbox and the allowlisting proxy.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use polyph_agents::registry::AgentConfig;
use polyph_testkit::fake_model::{FakeModel, Reply};
use polyph_testkit::harness::{AgentBins, Harness};
use serde_json::json;

fn bins() -> AgentBins {
    AgentBins {
        fake_agent: env!("CARGO_BIN_EXE_polyph-fake-agent").into(),
        sandbox_helper: env!("CARGO_BIN_EXE_polyph-test-sandbox-exec").into(),
        mcp: env!("CARGO_BIN_EXE_polyph-test-mcp").into(),
    }
}

async fn setup(model: &FakeModel, env: &[(&str, &str)]) -> (Harness, String, tempfile::TempDir) {
    let mut h = Harness::new().await.unwrap();
    h.agents = Some(bins());
    let mut cfg = AgentConfig {
        command: env!("CARGO_BIN_EXE_polyph-test-localagent").into(),
        hosts: vec!["127.0.0.1".into()],
        ..Default::default()
    };
    cfg.env.insert("POLYPH_LOCAL_ENDPOINT".into(), model.url.clone());
    cfg.env.insert("POLYPH_LOCAL_MODEL".into(), "fake-coder".into());
    for (k, v) in env {
        cfg.env.insert(k.to_string(), v.to_string());
    }
    h.extra_agents.insert("local".into(), cfg);
    let a = h.add("Ann").await.unwrap();
    let room = h.nodes[a].call("room.create", json!({ "name": "local" })).await.unwrap();
    let rid = room["id"].as_str().unwrap().to_string();
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(dir.path().join("src")).unwrap();
    std::fs::write(dir.path().join("src/lib.rs"), "pub fn existing() {}\n").unwrap();
    (h, rid, dir)
}

async fn run(h: &Harness, rid: &str, dir: &std::path::Path, prompt: &str) -> (serde_json::Value, serde_json::Value) {
    let r = h.nodes[0]
        .call("agent.start", json!({ "room": rid, "kind": "local", "cwd": dir, "preset": "auto" }))
        .await
        .unwrap();
    let id = r["id"].as_str().unwrap().to_string();
    let p = h.nodes[0].call("agent.prompt", json!({ "agent": id, "text": prompt })).await.unwrap();
    let t = h.nodes[0].call("agent.transcript", json!({ "room": rid, "agent": id })).await.unwrap();
    (p, t)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn reads_edits_and_runs_with_native_tools() {
    let model = FakeModel::start(vec![
        Reply::Tools(vec![("read".into(), r#"{"path":"src/lib.rs"}"#.into())]),
        Reply::Tools(vec![("edit".into(), r#"{"path":"src/lib.rs","old":"existing","new":"renamed"}"#.into())]),
        Reply::Tools(vec![("shell".into(), r#"{"command":"echo 2 passed, 0 failed"}"#.into())]),
        Reply::Text("Renamed the function and the tests pass.".into()),
    ])
    .await
    .unwrap();
    let (h, rid, dir) = setup(&model, &[]).await;
    let (p, t) = run(&h, &rid, dir.path(), "rename existing to renamed").await;
    assert_eq!(p["stop_reason"], "end_turn");
    assert_eq!(std::fs::read_to_string(dir.path().join("src/lib.rs")).unwrap(), "pub fn renamed() {}\n");
    let keys: Vec<&str> = t["summary"].as_array().unwrap().iter().map(|l| l["key"].as_str().unwrap()).collect();
    assert_eq!(keys, vec!["tool-read", "tool-edit", "tool-run-tests"], "{}", t["summary"]);
    assert_eq!(p["usage"]["input_tokens"], 200);
    assert_eq!(p["usage"]["output_tokens"], 40);
    let reqs = model.requests();
    assert!(reqs[0]["tools"].as_array().unwrap().len() >= 7);
    assert!(
        reqs[1]["messages"]
            .as_array()
            .unwrap()
            .iter()
            .any(|m| m["role"] == "tool" && m["content"].as_str().unwrap().contains("existing"))
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn retries_once_after_invalid_tool_json() {
    let model = FakeModel::start(vec![
        Reply::Tools(vec![("read".into(), "{not json".into())]),
        Reply::Tools(vec![("read".into(), r#"{"path":"src/lib.rs"}"#.into())]),
        Reply::Text("Read it.".into()),
    ])
    .await
    .unwrap();
    let (h, rid, dir) = setup(&model, &[]).await;
    let (p, _) = run(&h, &rid, dir.path(), "read lib").await;
    assert_eq!(p["stop_reason"], "end_turn");
    let reqs = model.requests();
    assert_eq!(reqs.len(), 3);
    let retry_msgs = reqs[1]["messages"].as_array().unwrap();
    assert!(
        retry_msgs.iter().any(|m| m["role"] == "tool" && m["content"].as_str().unwrap().contains("not valid JSON"))
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn text_protocol_for_models_without_tool_calls() {
    let model = FakeModel::start(vec![
        Reply::Text(r#"I'll write it. <tool name="write">{"path":"notes.md","content":"hello\n"}</tool>"#.into()),
        Reply::Text("Wrote notes.md.".into()),
    ])
    .await
    .unwrap();
    let (h, rid, dir) = setup(&model, &[("POLYPH_LOCAL_TOOLS", "text")]).await;
    let (p, _) = run(&h, &rid, dir.path(), "write notes").await;
    assert_eq!(p["stop_reason"], "end_turn");
    assert_eq!(std::fs::read_to_string(dir.path().join("notes.md")).unwrap(), "hello\n");
    let reqs = model.requests();
    assert!(reqs[0].get("tools").is_none());
    assert!(reqs[0]["messages"][0]["content"].as_str().unwrap().contains("<tool name="));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn summarizes_old_turns_near_the_context_limit() {
    let read = || Reply::Tools(vec![("read".into(), r#"{"path":"big.txt"}"#.into())]);
    let model = FakeModel::start(vec![read(), read(), read(), Reply::Text("Done reading.".into())]).await.unwrap();
    let (h, rid, dir) = setup(&model, &[("POLYPH_LOCAL_CONTEXT", "1200")]).await;
    std::fs::write(dir.path().join("big.txt"), "lorem ipsum dolor sit amet ".repeat(80)).unwrap();
    let (p, _) = run(&h, &rid, dir.path(), "read big.txt three times").await;
    assert_eq!(p["stop_reason"], "end_turn");
    let reqs = model.requests();
    assert!(reqs.iter().any(|r| r.get("tools").is_none() && r["messages"][0]["content"].as_str().unwrap().starts_with("Summarize")), "no summary request");
    let last = reqs.last().unwrap();
    assert!(
        last["messages"]
            .as_array()
            .unwrap()
            .iter()
            .any(|m| m["content"].as_str().is_some_and(|c| c.contains("SUMMARY-OF-EARLIER-WORK")))
    );
}

/// The built-in model router: `[router.tiers]` in polyph.toml picks the model
/// before the first turn, through `session/set_model`.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn router_switches_the_model_before_the_first_turn() {
    let model = FakeModel::start(vec![Reply::Text("ok".into()), Reply::Text("ok".into())]).await.unwrap();
    let (h, rid, dir) = setup(&model, &[]).await;
    std::fs::write(
        dir.path().join("polyph.toml"),
        "[router]\ndefault_tier = \"standard\"\n\n[router.tiers.local]\nstandard = \"routed-model\"\n",
    )
    .unwrap();
    let (p, _) = run(&h, &rid, dir.path(), "say ok").await;
    assert_eq!(p["stop_reason"], "end_turn");
    let reqs = model.requests();
    assert!(!reqs.is_empty(), "no request reached the model");
    assert_eq!(reqs[0]["model"], "routed-model", "{reqs:?}");
}

/// Opt-in: a real model through Ollama. Run with POLYPH_SMOKE=1 and Ollama
/// serving POLYPH_SMOKE_MODEL (default qwen2.5-coder:7b) on localhost:11434.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn smoke_real_ollama() {
    if std::env::var("POLYPH_SMOKE").is_err() {
        eprintln!("skipped: set POLYPH_SMOKE=1 to run against a local Ollama");
        return;
    }
    let mut h = Harness::new().await.unwrap();
    h.agents = Some(bins());
    let mut cfg = AgentConfig {
        command: env!("CARGO_BIN_EXE_polyph-test-localagent").into(),
        hosts: vec!["127.0.0.1".into(), "localhost".into()],
        ..Default::default()
    };
    cfg.env.insert(
        "POLYPH_LOCAL_MODEL".into(),
        std::env::var("POLYPH_SMOKE_MODEL").unwrap_or_else(|_| "qwen2.5-coder:7b".into()),
    );
    h.extra_agents.insert("local".into(), cfg);
    let a = h.add("Ann").await.unwrap();
    let room = h.nodes[a].call("room.create", json!({ "name": "smoke" })).await.unwrap();
    let rid = room["id"].as_str().unwrap().to_string();
    let dir = tempfile::tempdir().unwrap();
    let (p, _) = run(&h, &rid, dir.path(), "Create a file hello.txt containing the word hello.").await;
    eprintln!("ollama smoke: {p}");
    assert!(dir.path().join("hello.txt").exists());
}
