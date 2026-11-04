//! Milestone 5: GitHub and Linear, against local stand-ins for their APIs.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::Path;
use std::process::Command;
use std::sync::Arc;

use axum::Json;
use axum::extract::{Path as UrlPath, State};
use axum::http::{HeaderMap, StatusCode};
use axum::routing::{get, post};
use parking_lot::Mutex;
use polyph_testkit::harness::{AgentBins, Harness};
use serde_json::{Value, json};

fn bins() -> AgentBins {
    AgentBins {
        fake_agent: env!("CARGO_BIN_EXE_polyph-fake-agent").into(),
        sandbox_helper: env!("CARGO_BIN_EXE_polyph-test-sandbox-exec").into(),
        mcp: env!("CARGO_BIN_EXE_polyph-test-mcp").into(),
    }
}

#[derive(Default)]
struct Gh {
    pulls: Vec<Value>,
    merged: bool,
    reviews: Vec<Value>,
    linear_updates: Vec<Value>,
}

type S = Arc<Mutex<Gh>>;

fn authed(h: &HeaderMap, want: &str) -> bool {
    h.get("authorization").and_then(|v| v.to_str().ok()) == Some(want)
}

async fn mock() -> (String, S) {
    let st: S = Default::default();
    let app = axum::Router::new()
        .route(
            "/user",
            get(|h: HeaderMap| async move {
                if authed(&h, "Bearer gh-token") { (StatusCode::OK, Json(json!({ "login": "ann" }))) } else { (StatusCode::UNAUTHORIZED, Json(json!({ "message": "Bad credentials" }))) }
            }),
        )
        .route(
            "/repos/o/r/issues",
            get(|| async {
                Json(json!([
                    { "number": 1, "title": "Login fails on Safari", "body": "Steps: open /login", "html_url": "https://github.com/o/r/issues/1" },
                    { "number": 2, "title": "A pull request", "pull_request": {}, "html_url": "https://github.com/o/r/pull/2" }
                ]))
            }),
        )
        .route(
            "/repos/o/r/pulls",
            post(|State(s): State<S>, Json(b): Json<Value>| async move {
                let mut g = s.lock();
                if !g.pulls.is_empty() {
                    return (StatusCode::UNPROCESSABLE_ENTITY, Json(json!({ "message": "A pull request already exists" })));
                }
                g.pulls.push(b);
                (StatusCode::CREATED, Json(json!({ "number": 7, "html_url": "https://github.com/o/r/pull/7" })))
            })
            .get(|| async { Json(json!([{ "number": 7, "html_url": "https://github.com/o/r/pull/7" }])) }),
        )
        .route(
            "/repos/o/r/pulls/{n}",
            get(|State(s): State<S>, UrlPath(_n): UrlPath<u64>| async move {
                let g = s.lock();
                Json(json!({ "merged": g.merged, "state": if g.merged { "closed" } else { "open" }, "merge_commit_sha": "abc123" }))
            }),
        )
        .route("/repos/o/r/pulls/{n}/reviews", get(|State(s): State<S>, UrlPath(_n): UrlPath<u64>| async move { Json(json!(s.lock().reviews)) }))
        .route(
            "/graphql",
            post(|State(s): State<S>, h: HeaderMap, Json(b): Json<Value>| async move {
                if !authed(&h, "lin-key") {
                    return Json(json!({ "errors": [{ "message": "Authentication required" }] }));
                }
                let q = b["query"].as_str().unwrap_or_default();
                if q.contains("viewer") {
                    Json(json!({ "data": { "viewer": { "name": "Ann", "email": "ann@example.invalid" } } }))
                } else if q.contains("issueUpdate") {
                    s.lock().linear_updates.push(b["variables"].clone());
                    Json(json!({ "data": { "issueUpdate": { "success": true } } }))
                } else if q.contains("issue(id") {
                    Json(json!({ "data": { "issue": { "id": "uuid-12", "state": { "type": "unstarted" }, "team": { "states": { "nodes": [
                        { "id": "s-todo", "type": "unstarted", "position": 0 },
                        { "id": "s-doing", "type": "started", "position": 1 },
                        { "id": "s-review", "type": "started", "position": 2 },
                        { "id": "s-done", "type": "completed", "position": 3 }
                    ] } } } } }))
                } else {
                    assert_eq!(b["variables"]["team"], "ENG");
                    Json(json!({ "data": { "issues": { "nodes": [
                        { "id": "uuid-12", "identifier": "ENG-12", "title": "Rate limit the API", "description": "429 after 100/min", "url": "https://linear.app/x/issue/ENG-12", "state": { "type": "unstarted" } }
                    ] } } }))
                }
            }),
        )
        .with_state(st.clone());
    let l = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = l.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(l, app).await.unwrap() });
    (format!("http://{addr}"), st)
}

fn git(dir: &Path, args: &[&str]) -> String {
    let o = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(["-c", "user.name=Test", "-c", "user.email=test@example.invalid"])
        .args(args)
        .output()
        .unwrap();
    assert!(o.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&o.stderr));
    String::from_utf8_lossy(&o.stdout).into_owned()
}

async fn call(h: &Harness, m: &str, p: Value) -> Value {
    h.nodes[0].call(m, p).await.unwrap()
}

async fn approve_next(h: &Harness, rid: &str, kind: &str) {
    let id = h
        .until_value("approval", || async {
            let l = call(h, "approvals.list", json!({ "room": rid })).await;
            l.as_array().unwrap().iter().find(|a| a["kind"]["t"] == kind).map(|a| a["id"].as_str().unwrap().to_string())
        })
        .await
        .unwrap();
    call(h, "approve", json!({ "room": rid, "id": id })).await;
}

async fn entry_state(h: &Harness, rid: &str, want: &str) -> Value {
    h.until_value(want, || async {
        let q = call(h, "merge.queue", json!({ "room": rid })).await;
        let last = q.as_array().unwrap().last().cloned()?;
        assert_ne!(last["state"], "failed", "{last}");
        (last["state"] == want).then_some(last)
    })
    .await
    .unwrap()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn github_issues_and_pull_requests() {
    let (api, st) = mock().await;
    let mut h = Harness::new().await.unwrap();
    h.agents = Some(bins());
    h.add("Ann").await.unwrap();
    let room = call(&h, "room.create", json!({ "name": "gh" })).await;
    let rid = room["id"].as_str().unwrap().to_string();

    assert!(h.nodes[0].call("github.connect", json!({ "token": "wrong", "api_url": api })).await.is_err());
    let c = call(&h, "github.connect", json!({ "token": "gh-token", "api_url": api })).await;
    assert_eq!(c["login"], "ann");
    let status = call(&h, "integrations.status", json!({})).await;
    assert_eq!(status["github"]["login"], "ann");
    assert!(!status.to_string().contains("gh-token"));

    // A repository whose tasks merge through pull requests, with a remote.
    let remote = tempfile::tempdir().unwrap();
    git(remote.path(), &["init", "-q", "--bare", "-b", "main"]);
    let repo = tempfile::tempdir().unwrap();
    let r = repo.path();
    git(r, &["init", "-q", "-b", "main"]);
    std::fs::write(r.join("README.md"), "# app\n").unwrap();
    std::fs::write(r.join("polyph.toml"), "[merge]\nvia = \"github\"\n\n[github]\nrepo = \"o/r\"\n").unwrap();
    std::fs::write(
        r.join(".fake-script.json"),
        json!({ "turns": [
            [ { "write": { "path": "login.js", "content": "export const ok = true;\n" } } ],
            [ { "write": { "path": "login.test.js", "content": "test('ok', () => {});\n" } } ]
        ] })
        .to_string(),
    )
    .unwrap();
    git(r, &["add", "-A"]);
    git(r, &["commit", "-qm", "init"]);
    git(r, &["remote", "add", "origin", &remote.path().to_string_lossy()]);

    // Issues become cards once; pull requests in the list are skipped.
    let imp = call(&h, "github.import", json!({ "room": rid, "cwd": r })).await;
    assert_eq!(imp["added"].as_array().unwrap().len(), 1);
    assert!(call(&h, "github.import", json!({ "room": rid, "cwd": r })).await["added"].as_array().unwrap().is_empty());
    let board = call(&h, "board.get", json!({ "room": rid })).await;
    let card = &board["cards"][0];
    assert_eq!(
        (card["title"].as_str(), card["external"].as_str()),
        (Some("Login fails on Safari"), Some("github:o/r#1"))
    );
    assert_eq!(card["url"], "https://github.com/o/r/issues/1");
    let card_id = card["id"].as_str().unwrap().to_string();

    // The agent's work goes out as a pull request after Ann approves the push.
    let s = call(&h, "agent.start", json!({ "room": rid, "kind": "fake", "cwd": r, "task": card_id })).await;
    let (aid, branch) = (s["id"].as_str().unwrap().to_string(), s["branch"].as_str().unwrap().to_string());
    call(&h, "agent.prompt", json!({ "agent": aid, "text": "fix it" })).await;
    call(&h, "task.finish", json!({ "agent": aid })).await;
    approve_next(&h, &rid, "push").await;
    let e = entry_state(&h, &rid, "pull_request").await;
    assert_eq!(e["pr"], "https://github.com/o/r/pull/7");
    assert!(git(remote.path(), &["branch", "--list"]).contains(&branch), "the branch was pushed");
    {
        let g = st.lock();
        let pr = &g.pulls[0];
        assert_eq!((pr["head"].as_str(), pr["base"].as_str()), (Some(branch.as_str()), Some("main")));
        assert!(pr["body"].as_str().unwrap().contains("Closes #1"), "{pr}");
    }
    let board = call(&h, "board.get", json!({ "room": rid })).await;
    assert_eq!(
        (board["cards"][0]["column"].as_str(), board["cards"][0]["pr"].as_str()),
        (Some("review"), Some("https://github.com/o/r/pull/7"))
    );

    // A reviewer on GitHub asks for changes: the card goes back.
    st.lock().reviews =
        vec![json!({ "user": { "login": "carol" }, "state": "CHANGES_REQUESTED", "body": "Add a test" })];
    assert_eq!(call(&h, "github.sync", json!({ "room": rid })).await["changed"], 1);
    let e = entry_state(&h, &rid, "changes_requested").await;
    assert_eq!(e["detail"], "carol: Add a test");
    assert_eq!(call(&h, "board.get", json!({ "room": rid })).await["cards"][0]["column"], "in_progress");

    // Fixed and finished again: the same pull request; then it merges.
    h.clock.advance(60);
    call(&h, "agent.prompt", json!({ "agent": aid, "text": "add a test" })).await;
    call(&h, "task.finish", json!({ "agent": aid })).await;
    approve_next(&h, &rid, "push").await;
    let e = entry_state(&h, &rid, "pull_request").await;
    assert_eq!(e["pr_number"], 7);
    {
        let mut g = st.lock();
        g.reviews.push(json!({ "user": { "login": "carol" }, "state": "APPROVED", "body": "" }));
        g.merged = true;
    }
    call(&h, "github.sync", json!({ "room": rid })).await;
    let e = entry_state(&h, &rid, "merged").await;
    assert_eq!(e["merge_commit"], "abc123");
    assert_eq!(call(&h, "board.get", json!({ "room": rid })).await["cards"][0]["column"], "done");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn linear_issues_follow_their_cards() {
    let (api, st) = mock().await;
    let mut h = Harness::new().await.unwrap();
    h.add("Ann").await.unwrap();
    let room = call(&h, "room.create", json!({ "name": "lin" })).await;
    let rid = room["id"].as_str().unwrap().to_string();
    assert!(
        h.nodes[0]
            .call("linear.connect", json!({ "token": "nope", "api_url": format!("{api}/graphql") }))
            .await
            .is_err()
    );
    let c = call(&h, "linear.connect", json!({ "token": "lin-key", "api_url": format!("{api}/graphql") })).await;
    assert_eq!(c["login"], "Ann");
    let imp = call(&h, "linear.import", json!({ "room": rid, "team": "ENG" })).await;
    assert_eq!(imp["added"][0]["identifier"], "ENG-12");
    let board = call(&h, "board.get", json!({ "room": rid })).await;
    let card = board["cards"][0]["id"].as_str().unwrap().to_string();
    assert_eq!(board["cards"][0]["title"], "ENG-12 Rate limit the API");
    assert_eq!(board["cards"][0]["external"], "linear:ENG-12");

    // Moving the card moves the issue: started, then completed.
    call(&h, "board.move", json!({ "room": rid, "card": card, "column": "in_progress" })).await;
    h.until("started in Linear", || async { st.lock().linear_updates.len() == 1 }).await.unwrap();
    call(&h, "board.move", json!({ "room": rid, "card": card, "column": "done" })).await;
    h.until("completed in Linear", || async { st.lock().linear_updates.len() == 2 }).await.unwrap();
    let u = st.lock().linear_updates.clone();
    assert_eq!(u[0], json!({ "id": "uuid-12", "state": "s-doing" }));
    assert_eq!(u[1], json!({ "id": "uuid-12", "state": "s-done" }));
    // Columns with no Linear meaning change nothing.
    call(&h, "board.move", json!({ "room": rid, "card": card, "column": "backlog" })).await;
    tokio::time::sleep(std::time::Duration::from_millis(300)).await;
    assert_eq!(st.lock().linear_updates.len(), 2);
}
