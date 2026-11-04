//! Milestone 9: extensibility. Templates, skills agents read through the
//! Polyph MCP tools, a plugin's tool server called from inside an agent's
//! sandbox, and a plugin hook that answers in the room chat.
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

/// Test fixtures live under the target directory: agents under bubblewrap
/// get a private /tmp.
fn fixture_dir(name: &str) -> std::path::PathBuf {
    let d = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("m9-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

async fn room(h: &mut Harness, names: &[&str]) -> String {
    for n in names {
        h.add(n).await.unwrap();
    }
    let room = h.nodes[0].call("room.create", json!({ "name": "payments" })).await.unwrap();
    let rid = room["id"].as_str().unwrap().to_string();
    h.until("connected", || async {
        h.nodes[0].call("room.get", json!({ "room": rid })).await.unwrap()["connected"] == true
    })
    .await
    .unwrap();
    for i in 1..names.len() {
        h.join(&rid, 0, i, 0, "member").await.unwrap();
    }
    rid
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn templates_skills_and_plugin_tools_reach_agents() {
    let mut h = Harness::new().await.unwrap();
    h.agents = Some(bins());
    let rid = room(&mut h, &["Ann"]).await;
    let a = 0;

    // A template sets the repository up: polyph.toml and shared skills.
    let repo = fixture_dir("repo");
    std::fs::create_dir(repo.join(".git")).unwrap();
    let list = h.nodes[a].call("template.list", json!({})).await.unwrap();
    let names: Vec<&str> = list.as_array().unwrap().iter().map(|t| t["name"].as_str().unwrap()).collect();
    assert_eq!(names, vec!["startup", "hackathon", "open-source", "agency"]);
    let applied = h.nodes[a].call("template.apply", json!({ "name": "startup", "dir": repo })).await.unwrap();
    assert!(applied["written"].as_array().unwrap().iter().any(|f| f == "polyph.toml"));
    // Every built-in template's polyph.toml is a valid project config.
    for t in &names {
        let d = fixture_dir(&format!("tpl-{t}"));
        h.nodes[a].call("template.apply", json!({ "name": t, "dir": d })).await.unwrap();
        polyph_agents::project::ProjectConfig::load(&d).unwrap_or_else(|e| panic!("{t}: {e}"));
    }
    let skills = h.nodes[a].call("skills.list", json!({ "cwd": repo })).await.unwrap();
    assert_eq!(skills.as_array().unwrap().len(), 2, "{skills}");

    // A plugin with a tool server.
    let plug = fixture_dir("greeter");
    std::fs::write(plug.join("marker.txt"), "the plugin folder\n").unwrap();
    std::fs::write(
        plug.join("polyph-plugin.toml"),
        format!(
            "name = \"greeter\"\nversion = \"0.1.0\"\ndescription = \"Says hello.\"\n[mcp]\ncommand = \"{}\"\n",
            env!("CARGO_BIN_EXE_polyph-test-plugin-mcp")
        ),
    )
    .unwrap();
    let added = h.nodes[a].call("plugin.add", json!({ "path": plug })).await.unwrap();
    assert_eq!(added["name"], "greeter");
    let plugins = h.nodes[a].call("plugin.list", json!({})).await.unwrap();
    assert_eq!(plugins[0]["tools"], true);

    // An agent in the repository reads the skills and calls the plugin.
    std::fs::write(
        repo.join(".fake-script.json"),
        json!({ "turns": [[
            { "mcp": { "tool": "list_skills", "args": {} } },
            { "mcp": { "tool": "read_skill", "args": { "name": "review" } } },
            { "mcp": { "server": "plugin-greeter", "tool": "greet", "args": { "who": "Ann" } } }
        ]] })
        .to_string(),
    )
    .unwrap();
    let r = h.nodes[a]
        .call("agent.start", json!({ "room": rid, "kind": "fake", "cwd": repo, "preset": "default" }))
        .await
        .unwrap();
    let id = r["id"].as_str().unwrap().to_string();
    h.nodes[a].call("agent.prompt", json!({ "agent": id, "text": "go" })).await.unwrap();
    let t = h.nodes[a].call("agent.transcript", json!({ "room": rid, "agent": id })).await.unwrap().to_string();
    assert!(t.contains("mcp list_skills: Skills") && t.contains("review: The checklist"), "{t}");
    assert!(t.contains("mcp read_skill: Before you call a task finished"), "{t}");
    assert!(t.contains("mcp greet: hello Ann from the plugin folder"), "{t}");

    // Plugins can be removed; bad ones are refused with a reason.
    h.nodes[a].call("plugin.remove", json!({ "name": "greeter" })).await.unwrap();
    let bad = fixture_dir("bad");
    std::fs::write(
        bad.join("polyph-plugin.toml"),
        "name = \"bad\"\nversion = \"1\"\n[[hooks]]\non = [\"everything\"]\ncommand = \"sh\"\n",
    )
    .unwrap();
    let e = h.nodes[a].call("plugin.add", json!({ "path": bad })).await.unwrap_err().to_string();
    assert!(e.contains("unknown event \"everything\""), "{e}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn plugin_hooks_answer_in_chat_once() {
    let mut h = Harness::new().await.unwrap();
    h.agents = Some(bins());
    let rid = room(&mut h, &["Ann", "Bo"]).await;
    let (a, b) = (0, 1);
    let plug = fixture_dir("echo");
    std::fs::write(
        plug.join("polyph-plugin.toml"),
        "name = \"echo\"\nversion = \"0.1.0\"\n[[hooks]]\non = [\"chat\"]\ncommand = \"sh\"\nargs = [\"hook.sh\"]\n",
    )
    .unwrap();
    std::fs::write(plug.join("hook.sh"), "if grep -q ping; then echo '{\"chat\": \"pong from hook\"}'; fi\n").unwrap();
    h.nodes[a].call("plugin.add", json!({ "path": plug })).await.unwrap();

    h.nodes[b].call("chat.send", json!({ "room": rid, "text": "ping" })).await.unwrap();
    let pongs = || async {
        let v = h.nodes[b].call("chat.list", json!({ "room": rid })).await.unwrap();
        v.as_array().unwrap().iter().filter(|m| m["text"] == "pong from hook" && m["agent"] == "plugin/echo").count()
    };
    h.until("the hook answered", || async { pongs().await == 1 }).await.unwrap();
    // Its own post does not trigger it again.
    tokio::time::sleep(std::time::Duration::from_millis(800)).await;
    assert_eq!(pongs().await, 1);
}
