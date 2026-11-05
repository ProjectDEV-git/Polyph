//! The room's repository: remembered without credentials and shared with
//! members, set only by owners and maintainers; `sync.run` moving a linked
//! checkout forward; folders fetched when a merge lands.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::Path;
use std::process::Command;

use polyph_daemon::events::ApiEvent;
use polyph_testkit::harness::Harness;
use serde_json::json;

fn git(dir: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .env("GIT_AUTHOR_NAME", "t")
        .env("GIT_AUTHOR_EMAIL", "t@t")
        .env("GIT_COMMITTER_NAME", "t")
        .env("GIT_COMMITTER_EMAIL", "t@t")
        .output()
        .unwrap();
    assert!(out.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&out.stderr));
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

/// A bare origin and two clones of it.
fn repos() -> (tempfile::TempDir, std::path::PathBuf, std::path::PathBuf) {
    let d = tempfile::tempdir().unwrap();
    let origin = d.path().join("origin.git");
    git(d.path(), &["init", "-q", "--bare", "-b", "main", &origin.to_string_lossy()]);
    let seed = d.path().join("seed");
    git(d.path(), &["clone", "-q", &origin.to_string_lossy(), &seed.to_string_lossy()]);
    git(&seed, &["checkout", "-q", "-b", "main"]);
    std::fs::write(seed.join("a.txt"), "a\n").unwrap();
    git(&seed, &["add", "."]);
    git(&seed, &["commit", "-qm", "a"]);
    git(&seed, &["push", "-q", "origin", "main"]);
    let mine = d.path().join("mine");
    git(d.path(), &["clone", "-q", &origin.to_string_lossy(), &mine.to_string_lossy()]);
    (d, seed, mine)
}

fn push_commit(seed: &Path, file: &str) {
    std::fs::write(seed.join(file), "x\n").unwrap();
    git(seed, &["add", "."]);
    git(seed, &["commit", "-qm", file]);
    git(seed, &["push", "-q", "origin", "main"]);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn the_room_remembers_its_repository() {
    let mut h = Harness::new().await.unwrap();
    let a = h.add("Ann").await.unwrap();
    let b = h.add("Bo").await.unwrap();
    let room = h.nodes[a].call("room.create", json!({ "name": "repo" })).await.unwrap();
    let rid = room["id"].as_str().unwrap().to_string();
    h.until("connected", || async {
        h.nodes[a].call("room.get", json!({ "room": rid })).await.unwrap()["connected"] == true
    })
    .await
    .unwrap();
    let rid_b = h.join(&rid, a, b, a, "member").await.unwrap();
    let set = h.nodes[a]
        .call(
            "repo.set",
            json!({ "room": rid, "url": "https://ann:ghp_TOKEN123@github.com/acme/pay.git?x=1", "branch": "main", "files": 120000 }),
        )
        .await
        .unwrap();
    assert_eq!(set["url"], "https://github.com/acme/pay.git");
    assert_eq!(set["large"], true);
    h.until("Bo knows the repository", || async {
        let r = h.nodes[b].call("repo.get", json!({ "room": rid_b })).await.unwrap();
        r["url"] == "https://github.com/acme/pay.git" && r["branch"] == "main"
    })
    .await
    .unwrap();
    // Nothing secret travelled.
    let r = h.nodes[b].call("repo.get", json!({ "room": rid_b })).await.unwrap();
    assert!(!r.to_string().contains("ghp_"));
    // Members cannot point the room somewhere else; local paths are refused.
    let e = h.nodes[b].call("repo.set", json!({ "room": rid_b, "url": "https://evil.example/x.git" })).await;
    assert!(e.unwrap_err().to_string().contains("Owners and Maintainers"));
    let e = h.nodes[a].call("repo.set", json!({ "room": rid, "url": "/home/ann/pay" })).await;
    assert!(e.is_err());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn sync_and_fetch_on_merge() {
    let mut h = Harness::new().await.unwrap();
    let a = h.add("Ann").await.unwrap();
    let room = h.nodes[a].call("room.create", json!({ "name": "sync" })).await.unwrap();
    let rid = room["id"].as_str().unwrap().to_string();
    let (_d, seed, mine) = repos();
    h.nodes[a].call("folder.link", json!({ "room": rid, "dir": mine })).await.unwrap();

    push_commit(&seed, "b.txt");
    let out = h.nodes[a].call("sync.run", json!({ "room": rid })).await.unwrap();
    let rep = &out[0]["report"];
    assert_eq!(rep["fast_forwarded"], true, "{out}");
    assert_eq!(rep["behind"], 1);
    assert!(mine.join("b.txt").exists());

    // A merge landing somewhere fetches linked folders without anyone asking.
    push_commit(&seed, "c.txt");
    let before = git(&mine, &["rev-parse", "origin/main"]);
    h.nodes[a].daemon.shared.emit(ApiEvent::room_kind(
        &polyph_proto::ids::RoomId::from_hex(&rid).unwrap(),
        "merge_queue",
        json!({ "entry": { "state": "merged" } }),
    ));
    h.until("fetched", || async { git(&mine, &["rev-parse", "origin/main"]) != before }).await.unwrap();
    assert!(!mine.join("c.txt").exists(), "fetching never changes the person's files");
}
