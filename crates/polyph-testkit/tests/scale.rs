//! Big projects: 30 agents on three devices working through a repository of
//! 100,000 files, with code areas, overlapping files and dependencies.
//!
//! Each agent loops: `next_task`, `get_context` for the task's files,
//! `claim_files` (waiting while another agent holds one), `remember` a note,
//! `finish_task`. A stand-in for the merge queue moves finished cards to
//! done, which unblocks the cards that depend on them. The test checks that
//! every card went to exactly one agent, that no file was ever held by two
//! agents at once, and that every card got done; it prints how long each
//! tool took.
//!
//! The size comes from the environment, so CI can run a smaller one:
//! `POLYPH_SCALE_FILES` (default 100000), `POLYPH_SCALE_AGENTS` (default 30),
//! `POLYPH_SCALE_CARDS` (default 150). Results for the default size are in
//! `docs/big-projects.md`.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

use parking_lot::Mutex;
use polyph_testkit::harness::Harness;
use serde_json::{Value, json};

fn env(name: &str, default: usize) -> usize {
    std::env::var(name).ok().and_then(|v| v.parse().ok()).unwrap_or(default)
}

fn git(dir: &Path, args: &[&str]) {
    let out = std::process::Command::new("git")
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
}

/// `pkgNNN/modN/fileNNN.rs`, 100 files per module, 10 modules per package.
fn file(i: usize) -> String {
    format!("pkg{:03}/mod{}/file{:03}.rs", i / 1000, (i / 100) % 10, i % 100)
}

fn make_repo(dir: &Path, files: usize) {
    for i in 0..files {
        let f = dir.join(file(i));
        if i % 100 == 0 {
            std::fs::create_dir_all(f.parent().unwrap()).unwrap();
        }
        std::fs::write(&f, format!("pub fn f{i}() -> usize {{ {i} }}\n")).unwrap();
    }
    // Twenty areas over the packages, owned by the three people in turn.
    let people = ["Ann", "Bo", "Cy"];
    let mut areas = String::from("# pattern  owners\n");
    for a in 0..20 {
        areas.push_str(&format!("/pkg{:03}/ @{}\n", a, people[a % 3]));
    }
    std::fs::create_dir_all(dir.join(".polyph")).unwrap();
    std::fs::write(dir.join(".polyph/AREAS"), areas).unwrap();
    git(dir, &["init", "-q", "-b", "main"]);
    git(dir, &["add", "-A"]);
    git(dir, &["commit", "-qm", "initial"]);
}

#[derive(Default)]
struct Stats {
    by_tool: HashMap<&'static str, Vec<Duration>>,
}

impl Stats {
    fn add(&mut self, tool: &'static str, d: Duration) {
        self.by_tool.entry(tool).or_default().push(d);
    }

    fn report(&mut self) -> String {
        let mut out = String::new();
        let mut tools: Vec<_> = self.by_tool.keys().copied().collect();
        tools.sort();
        for t in tools {
            let v = self.by_tool.get_mut(t).unwrap();
            v.sort();
            let pick = |q: f64| v[((v.len() as f64 - 1.0) * q).round() as usize].as_millis();
            out.push_str(&format!(
                "{t:<12} calls {:>5}  p50 {:>5} ms  p95 {:>5} ms  max {:>5} ms\n",
                v.len(),
                pick(0.5),
                pick(0.95),
                v.last().unwrap().as_millis()
            ));
        }
        out
    }
}

async fn call(h: &Harness, node: usize, session: &str, stats: &Mutex<Stats>, tool: &'static str, args: Value) -> Value {
    let t = Instant::now();
    let r = h.nodes[node]
        .call("mcp.call", json!({ "session": session, "tool": tool, "args": args }))
        .await
        .unwrap_or_else(|e| panic!("{tool}: {e}"));
    stats.lock().add(tool, t.elapsed());
    r
}

#[tokio::test(flavor = "multi_thread", worker_threads = 8)]
async fn thirty_agents_on_a_large_repository() {
    let files = env("POLYPH_SCALE_FILES", 100_000);
    let agents = env("POLYPH_SCALE_AGENTS", 30);
    let cards = env("POLYPH_SCALE_CARDS", 150);
    let repo = tempfile::tempdir().unwrap();
    let t = Instant::now();
    make_repo(repo.path(), files);
    let setup_repo = t.elapsed();

    // Areas and file counts at this size.
    let t = Instant::now();
    let areas = polyph_daemon::areas::load(repo.path());
    let in_areas = (0..files).filter(|i| areas.owner_of(&file(*i)).is_some()).count();
    let area_lookups = t.elapsed();
    let t = Instant::now();
    assert_eq!(polyph_git::Repo::open(repo.path()).unwrap().file_count().unwrap(), files + 1);
    let ls_files = t.elapsed();

    let mut h = Harness::new().await.unwrap();
    for name in ["Ann", "Bo", "Cy"] {
        h.add(name).await.unwrap();
    }
    let room = h.nodes[0].call("room.create", json!({ "name": "big" })).await.unwrap();
    let rid = room["id"].as_str().unwrap().to_string();
    h.until("connected", || async {
        h.nodes[0].call("room.get", json!({ "room": rid })).await.unwrap()["connected"] == true
    })
    .await
    .unwrap();
    let mut rooms = vec![rid.clone()];
    for i in 1..3 {
        rooms.push(h.join(&rid, 0, i, 0, "member").await.unwrap());
    }
    h.nodes[0]
        .call("brief.set", json!({ "room": rid, "text": "A large codebase. Keep functions pure." }))
        .await
        .unwrap();

    // Cards: three files each, spread over the repository; every tenth card
    // shares a file with the card before it, and the last third depend on
    // earlier cards.
    let mut ids: Vec<String> = vec![];
    let stride = (files / cards).max(1);
    for c in 0..cards {
        let base = (c * stride) % files;
        let mut fs: Vec<String> = (0..3).map(|k| file((base + k) % files)).collect();
        if c % 10 == 9 {
            fs[0] = file(((c - 1) * stride) % files);
        }
        let r = h.nodes[0]
            .call("board.add", json!({ "room": rid, "title": format!("task {c}"), "column": "ready", "files": fs }))
            .await
            .unwrap();
        ids.push(r["id"].as_str().unwrap().to_string());
    }
    let dependent = cards * 2 / 3;
    for c in dependent..cards {
        h.nodes[0]
            .call("board.update", json!({ "room": rid, "card": ids[c], "depends_on": [ids[c - dependent]] }))
            .await
            .unwrap();
    }
    for (i, r) in rooms.iter().enumerate().skip(1) {
        h.until("the board reached everyone", || async {
            let b = h.nodes[i].call("board.get", json!({ "room": r })).await.unwrap();
            b.to_string().matches("\"task ").count() >= cards
        })
        .await
        .unwrap();
    }

    // Agents, spread over the three devices.
    let mut sessions = vec![];
    for a in 0..agents {
        let node = a % 3;
        let r = h.nodes[node]
            .call("mcp.connect", json!({ "room": rooms[node], "cwd": repo.path(), "client": format!("agent{a}") }))
            .await
            .unwrap();
        sessions.push((node, r["session"].as_str().unwrap().to_string(), r["label"].as_str().unwrap().to_string()));
    }

    let h = Arc::new(h);
    let stats = Arc::new(Mutex::new(Stats::default()));
    let taken: Arc<Mutex<Vec<(String, String)>>> = Arc::default();
    // Who holds each file right now, as the agents see their own claims.
    let holding: Arc<Mutex<HashMap<String, String>>> = Arc::default();
    let overlaps = Arc::new(Mutex::new(0usize));
    let errors: Arc<Mutex<Vec<String>>> = Arc::default();
    let done = Arc::new(std::sync::atomic::AtomicBool::new(false));

    // The merge queue's stand-in: finished cards (review) move to done.
    let merger = {
        let (h, rid, done) = (h.clone(), rid.clone(), done.clone());
        tokio::spawn(async move {
            while !done.load(std::sync::atomic::Ordering::Relaxed) {
                let b = h.nodes[0].call("board.get", json!({ "room": rid })).await.unwrap();
                let review: Vec<String> = b
                    .as_array()
                    .or_else(|| b["cards"].as_array())
                    .into_iter()
                    .flatten()
                    .filter(|c| c["column"] == "review")
                    .map(|c| c["id"].as_str().unwrap().to_string())
                    .collect();
                for id in review {
                    h.nodes[0].call("board.move", json!({ "room": rid, "card": id, "column": "done" })).await.unwrap();
                }
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        })
    };

    let started = Instant::now();
    let mut workers = vec![];
    for (node, session, label) in sessions {
        let (h, stats, taken, holding, overlaps, errors) =
            (h.clone(), stats.clone(), taken.clone(), holding.clone(), overlaps.clone(), errors.clone());
        let total = cards;
        workers.push(tokio::spawn(async move {
            let mut idle = 0;
            loop {
                let n = call(&h, node, &session, &stats, "next_task", json!({})).await;
                let Some(id) = n["data"]["id"].as_str().map(str::to_string) else {
                    if taken.lock().len() >= total || idle > 600 {
                        return;
                    }
                    idle += 1;
                    tokio::time::sleep(Duration::from_millis(100)).await;
                    continue;
                };
                idle = 0;
                taken.lock().push((id.clone(), label.clone()));
                let fs: Vec<String> =
                    n["data"]["files"].as_array().unwrap().iter().map(|f| f.as_str().unwrap().to_string()).collect();
                let c = call(&h, node, &session, &stats, "get_context", json!({ "paths": fs })).await;
                assert!(c["text"].as_str().unwrap().contains("Keep functions pure"), "{c}");
                // Claim the files, waiting while another agent holds one.
                for attempt in 0.. {
                    let r = call(&h, node, &session, &stats, "claim_files", json!({ "paths": fs, "reason": id })).await;
                    if r["error"] == true {
                        // The arbiter did not answer in time: count it, try again.
                        errors.lock().push(r["text"].as_str().unwrap_or_default().to_string());
                        assert!(attempt < 40, "{label}: the arbiter never answered for {fs:?}: {r}");
                        tokio::time::sleep(Duration::from_millis(200)).await;
                        continue;
                    }
                    if r["data"]["denied"].as_array().unwrap().is_empty() {
                        break;
                    }
                    *overlaps.lock() += 1;
                    call(&h, node, &session, &stats, "release_files", json!({})).await;
                    assert!(attempt < 400, "{label} never got {fs:?}");
                    tokio::time::sleep(Duration::from_millis(50)).await;
                }
                {
                    let mut held = holding.lock();
                    for f in &fs {
                        if let Some(other) = held.insert(f.clone(), label.clone()) {
                            panic!("{f} was granted to {label} while {other} held it");
                        }
                    }
                }
                call(
                    &h,
                    node,
                    &session,
                    &stats,
                    "remember",
                    json!({ "path": fs[0], "note": format!("done by {label} for {id}") }),
                )
                .await;
                {
                    let mut held = holding.lock();
                    for f in &fs {
                        held.remove(f);
                    }
                }
                let f =
                    call(&h, node, &session, &stats, "finish_task", json!({ "summary": format!("did {id}") })).await;
                assert_eq!(f["error"], false, "{f}");
            }
        }));
    }
    for w in workers {
        w.await.unwrap();
    }
    let work = started.elapsed();
    done.store(true, std::sync::atomic::Ordering::Relaxed);
    merger.await.unwrap();

    let taken = taken.lock().clone();
    let unique: HashSet<&String> = taken.iter().map(|(id, _)| id).collect();
    assert_eq!(unique.len(), taken.len(), "a card went to two agents");
    assert_eq!(unique.len(), cards, "every card was taken");
    let by_agent: HashSet<&String> = taken.iter().map(|(_, a)| a).collect();
    let t = Instant::now();
    let hits = h.nodes[2].call("search", json!({ "room": rooms[2], "query": "done by" , "limit": 50 })).await.unwrap();
    let search = t.elapsed();
    assert!(!hits.as_array().unwrap().is_empty());
    let report = format!(
        "files {files}, agents {agents} on 3 devices, cards {cards} ({} depend on others)\n\
         repository written and committed in {:.1} s; git ls-files {} ms; area lookups for every file {} ms ({in_areas} in areas)\n\
         all cards done in {:.1} s by {} agents; {} claim(s) had to wait for another agent; {} claim call(s) failed and were retried; search {} ms\n{}",
        cards - dependent,
        setup_repo.as_secs_f64(),
        ls_files.as_millis(),
        area_lookups.as_millis(),
        work.as_secs_f64(),
        by_agent.len(),
        overlaps.lock(),
        errors.lock().len(),
        search.as_millis(),
        stats.lock().report()
    );
    println!("{report}");
    if let Some(e) = errors.lock().first() {
        println!("first failed claim: {e}");
    }
}
