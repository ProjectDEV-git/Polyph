//! KAO-72: three people on three networks. Three daemons, each in its own
//! network namespace and unable to reach the others, talk only through a
//! relay and run the success path from PRODUCT.md: create a room, two members
//! join by code, a planner splits a goal, claims stop a collision, a review
//! and an approval come before each merge, and one conflicting edit is merged
//! keeping both tasks' intent.
//!
//! Needs root for the namespaces, so it only runs under `scripts/multinet.sh`
//! (which sets `POLYPH_MULTINET=1`, `POLYPH_MULTINET_NS` and
//! `POLYPH_MULTINET_RELAY`). Anywhere else it prints "skipped" and passes.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::collections::HashMap;
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use parking_lot::Mutex;
use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin};
use tokio::sync::oneshot;

type Pending = Arc<Mutex<HashMap<u64, oneshot::Sender<Result<Value, String>>>>>;

/// A daemon (`polyph-multinet-node`) running in a network namespace.
struct Remote {
    name: String,
    stdin: tokio::sync::Mutex<ChildStdin>,
    pending: Pending,
    next: AtomicU64,
    _child: Child,
    _home: tempfile::TempDir,
}

fn output(cmd: &str, args: &[&str]) -> String {
    let o = Command::new(cmd).args(args).output().unwrap();
    assert!(o.status.success(), "{cmd} {args:?}");
    String::from_utf8_lossy(&o.stdout).trim().to_string()
}

impl Remote {
    async fn start(ns: &str, name: &str, relay: &str, agents: &Path) -> Remote {
        let home = tempfile::tempdir().unwrap();
        let (uid, gid) = (output("id", &["-u"]), output("id", &["-g"]));
        // Namespaces need root; the daemon itself drops back to this user, so
        // sandboxes and file ownership behave as they do on a laptop.
        let mut cmd = if uid == "0" {
            tokio::process::Command::new("ip")
        } else {
            let mut c = tokio::process::Command::new("sudo");
            c.args(["-n", "ip"]);
            c
        };
        cmd.args(["netns", "exec", ns, "setpriv", "--reuid", &uid, "--regid", &gid, "--clear-groups", "env"])
            .arg(format!("HOME={}", home.path().display()))
            .args(["GIT_AUTHOR_NAME=Test", "GIT_AUTHOR_EMAIL=test@example.invalid"])
            .args(["GIT_COMMITTER_NAME=Test", "GIT_COMMITTER_EMAIL=test@example.invalid"])
            .arg(env!("CARGO_BIN_EXE_polyph-multinet-node"))
            .arg("--home")
            .arg(home.path().join("polyph"))
            .args(["--name", name, "--relay", relay, "--agents"])
            .arg(agents)
            .args(["--fake-agent", env!("CARGO_BIN_EXE_polyph-fake-agent")])
            .args(["--sandbox-helper", env!("CARGO_BIN_EXE_polyph-test-sandbox-exec")])
            .args(["--mcp", env!("CARGO_BIN_EXE_polyph-test-mcp")])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .kill_on_drop(true);
        let mut child = cmd.spawn().unwrap();
        let stdin = child.stdin.take().unwrap();
        let mut lines = BufReader::new(child.stdout.take().unwrap()).lines();
        let pending: Pending = Default::default();
        let (ready_tx, ready_rx) = oneshot::channel();
        let (p, mut ready_tx) = (pending.clone(), Some(ready_tx));
        tokio::spawn(async move {
            while let Ok(Some(line)) = lines.next_line().await {
                let Ok(v) = serde_json::from_str::<Value>(&line) else { continue };
                if v["ready"] == true {
                    if let Some(t) = ready_tx.take() {
                        let _ = t.send(());
                    }
                } else if let Some(tx) = v["id"].as_u64().and_then(|id| p.lock().remove(&id)) {
                    let _ = tx.send(match v.get("err") {
                        Some(e) => Err(e.as_str().unwrap_or_default().to_string()),
                        None => Ok(v["ok"].clone()),
                    });
                }
            }
        });
        tokio::time::timeout(Duration::from_secs(60), ready_rx)
            .await
            .unwrap_or_else(|_| panic!("{name}: the daemon did not start in {ns}"))
            .unwrap_or_else(|_| panic!("{name}: the daemon exited while starting in {ns}"));
        Remote {
            name: name.into(),
            stdin: tokio::sync::Mutex::new(stdin),
            pending,
            next: AtomicU64::new(1),
            _child: child,
            _home: home,
        }
    }

    async fn call(&self, method: &str, params: Value) -> anyhow::Result<Value> {
        let id = self.next.fetch_add(1, Ordering::SeqCst);
        let (tx, rx) = oneshot::channel();
        self.pending.lock().insert(id, tx);
        let mut line = json!({ "id": id, "method": method, "params": params }).to_string();
        line.push('\n');
        self.stdin.lock().await.write_all(line.as_bytes()).await?;
        rx.await?.map_err(|e| anyhow::anyhow!("{} {method}: {e}", self.name))
    }

    async fn ok(&self, method: &str, params: Value) -> Value {
        self.call(method, params).await.unwrap()
    }
}

/// Polls `f` until it gives a value; the relay is a real network hop away, so
/// this waits longer than the in-process harness does.
async fn until<T, F, Fut>(what: &str, mut f: F) -> T
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = Option<T>>,
{
    let deadline = tokio::time::Instant::now() + Duration::from_secs(60);
    while tokio::time::Instant::now() < deadline {
        if let Some(v) = f().await {
            return v;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    panic!("timed out waiting until {what}");
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

const LIB: &str = "pub fn charge(n: i64) -> i64 {\n    n\n}\n";
const REFUND: &str = "pub fn refund(n: i64) -> i64 {\n    -n\n}\n";
const INVOICE: &str = "pub fn invoice(n: i64) -> String {\n    format!(\"due: {n}\")\n}\n";

fn with(f: &str) -> String {
    format!("{LIB}\n{f}")
}

/// A fake agent kind running `script`.
fn scripted(dir: &Path, name: &str, script: Value) -> Value {
    let p = dir.join(format!("{name}.json"));
    std::fs::write(&p, script.to_string()).unwrap();
    json!({ "command": env!("CARGO_BIN_EXE_polyph-fake-agent"), "env": { "POLYPH_FAKE_SCRIPT": p } })
}

async fn messages(n: &Remote, rid: &str, agent: &str) -> Vec<String> {
    let t = n.ok("agent.transcript", json!({ "room": rid, "agent": agent })).await;
    t["events"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|e| e["kind"]["t"] == "message")
        .flat_map(|e| e["kind"]["c"]["text"].as_str().unwrap().lines().map(str::to_string).collect::<Vec<_>>())
        .collect()
}

async fn entry(n: &Remote, rid: &str, branch: &str) -> Option<Value> {
    let q = n.ok("merge.queue", json!({ "room": rid })).await;
    q.as_array().unwrap().iter().rev().find(|e| e["branch"] == branch).cloned()
}

/// Approves the merge of `branch` as `who`, once the approval shows up.
async fn approve_merge(who: &Remote, rid: &str, branch: &str) {
    let id = until("merge approval", || async {
        let l = who.ok("approvals.list", json!({ "room": rid })).await;
        l.as_array()
            .unwrap()
            .iter()
            .find(|a| a["kind"]["t"] == "merge" && a["kind"]["c"]["branch"] == branch)
            .map(|a| a["id"].as_str().unwrap().to_string())
    })
    .await;
    who.ok("approve", json!({ "room": rid, "id": id })).await;
}

async fn wait_state(n: &Remote, rid: &str, branch: &str, state: &str) -> Value {
    until(&format!("{branch} is {state}"), || async {
        let e = entry(n, rid, branch).await?;
        assert_ne!(e["state"], "failed", "merge failed: {e}");
        (e["state"] == state).then_some(e)
    })
    .await
}

/// `joiner` joins with a fresh code from `inviter`, who approves the knock.
async fn join(rid: &str, inviter: &Remote, joiner: &Remote) {
    let code = inviter.ok("room.invite", json!({ "room": rid })).await["code"].as_str().unwrap().to_string();
    let joining = joiner.call("join", json!({ "code": code, "name": joiner.name, "timeout_secs": 60 }));
    let approving = async {
        let knock = until("a knock", || async {
            inviter.ok("knock.list", json!({ "room": rid })).await.as_array().unwrap().first().cloned()
        })
        .await;
        inviter
            .ok("knock.decide", json!({ "room": rid, "knock": knock["id"], "approve": true, "role": "member" }))
            .await;
    };
    let (joined, ()) = tokio::join!(joining, approving);
    joined.unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn three_networks_one_relay_the_whole_success_path() {
    let (Some(_), Ok(ns), Ok(relay)) = (
        std::env::var_os("POLYPH_MULTINET"),
        std::env::var("POLYPH_MULTINET_NS"),
        std::env::var("POLYPH_MULTINET_RELAY"),
    ) else {
        eprintln!("skipped: run scripts/multinet.sh (POLYPH_MULTINET=1)");
        return;
    };
    let ns: Vec<&str> = ns.split_whitespace().collect();
    assert_eq!(ns.len(), 3, "POLYPH_MULTINET_NS names three namespaces");

    let plan = json!({ "goal": "Billing", "tasks": [
        { "id": "t1", "title": "Add refunds", "description": "refund()", "acceptance": ["refund works"], "predicted_files": ["src/lib.rs"], "size": "s", "suggested_assignee": "Bo/fake-a" },
        { "id": "t2", "title": "Add invoices", "description": "invoice()", "acceptance": ["invoice works"], "predicted_files": ["src/lib.rs"], "size": "s", "suggested_assignee": "Cy/fake-b" }
    ] });
    // Agents' scripts live outside /tmp: bubblewrap gives agents an empty /tmp.
    let scripts = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).unwrap();
    let kinds = json!({
        "fake-planner": scripted(scripts.path(), "planner", json!({ "turns": [[ { "message": format!("```json\n{plan}\n```") } ]] })),
        "fake-a": scripted(scripts.path(), "a", json!({ "turns": [[ { "write": { "path": "src/lib.rs", "content": with(REFUND) } } ]] })),
        "fake-b": scripted(scripts.path(), "b", json!({ "turns": [
            [ { "write": { "path": "src/lib.rs", "content": with(INVOICE) } } ],
            [ { "write": { "path": "src/lib.rs", "content": with(INVOICE) } } ]
        ] })),
        "fake-reviewer": scripted(scripts.path(), "r", json!({ "turns": [[ { "message": "Does what the card asks.\nVERDICT: APPROVE" } ]] })),
    });
    let agents = scripts.path().join("agents.json");
    std::fs::write(&agents, kinds.to_string()).unwrap();

    // The repository the tasks work on. The three machines share this one
    // directory; only their networks are separate.
    let repo = tempfile::tempdir().unwrap();
    let r = repo.path();
    git(r, &["init", "-q", "-b", "main"]);
    git(r, &["config", "core.autocrlf", "false"]);
    std::fs::create_dir_all(r.join("src")).unwrap();
    std::fs::write(r.join("src/lib.rs"), LIB).unwrap();
    std::fs::write(
        r.join("polyph.toml"),
        "checks = [\"sh -c 'grep -q charge src/lib.rs'\"]\n\n[merge]\nreviewer = \"fake-reviewer\"\n",
    )
    .unwrap();
    git(r, &["add", "-A"]);
    git(r, &["commit", "-q", "-m", "initial"]);

    let (ann, bo, cy) = tokio::join!(
        Remote::start(ns[0], "Ann", &relay, &agents),
        Remote::start(ns[1], "Bo", &relay, &agents),
        Remote::start(ns[2], "Cy", &relay, &agents),
    );

    // Ann creates the room; Bo and Cy join by code, each approved by Ann.
    let rid = ann.ok("room.create", json!({ "name": "payments" })).await["id"].as_str().unwrap().to_string();
    until("Ann is connected to the relay", || async {
        (ann.ok("room.get", json!({ "room": rid })).await["connected"] == true).then_some(())
    })
    .await;
    join(&rid, &ann, &bo).await;
    join(&rid, &ann, &cy).await;
    for n in [&ann, &bo, &cy] {
        until("everyone sees three members", || async {
            let m = n.ok("room.get", json!({ "room": rid })).await;
            (m["members"].as_array().map(Vec::len) == Some(3)).then_some(())
        })
        .await;
    }

    // The planner splits the goal into two cards, both touching src/lib.rs.
    let p =
        ann.ok("plan.create", json!({ "room": rid, "goal": "Add billing", "cwd": r, "kind": "fake-planner" })).await;
    assert_eq!(p["cards"].as_array().unwrap().len(), 2, "{p}");
    assert_eq!(p["overlaps"].as_array().unwrap().len(), 1, "both tasks touch src/lib.rs: {p}");
    let acc = ann.ok("plan.accept", json!({ "room": rid, "plan": p["plan"] })).await;
    assert_eq!((acc["accepted"].as_u64(), acc["ready"].as_u64()), (Some(2), Some(2)));
    let card = |title: &'static str| {
        let (n, rid) = (&bo, rid.clone());
        async move {
            until("the planned cards reach Bo", || async {
                let b = n.ok("board.get", json!({ "room": rid })).await;
                b["cards"].as_array().unwrap().iter().find(|c| c["title"] == title).map(|c| c["id"].clone())
            })
            .await
        }
    };
    let (refunds, invoices) = (card("Add refunds").await, card("Add invoices").await);

    // Each member starts an agent on a card, in its own worktree and branch.
    let sa = bo.ok("agent.start", json!({ "room": rid, "kind": "fake-a", "cwd": r, "task": refunds })).await;
    let sb = cy.ok("agent.start", json!({ "room": rid, "kind": "fake-b", "cwd": r, "task": invoices })).await;
    let (aid, bid) = (sa["id"].as_str().unwrap().to_string(), sb["id"].as_str().unwrap().to_string());
    let (abr, bbr) = (sa["branch"].as_str().unwrap().to_string(), sb["branch"].as_str().unwrap().to_string());
    assert_ne!(sa["worktree"], sb["worktree"]);

    // Bo's agent edits lib.rs; the claim crosses the relay and stops Cy's agent.
    bo.ok("agent.prompt", json!({ "agent": aid, "text": "add refunds" })).await;
    until("the claim reaches Cy", || async {
        (cy.ok("claims.list", json!({ "room": rid })).await.as_array().unwrap().len() == 1).then_some(())
    })
    .await;
    cy.ok("agent.prompt", json!({ "agent": bid, "text": "add invoices" })).await;
    let cm = messages(&cy, &rid, &bid).await;
    assert!(cm.iter().any(|m| m.starts_with("write: denied") && m.contains("Bo/fake-a")), "{cm:?}");

    // Bo's task: review, Ann's approval, merge.
    bo.ok("task.finish", json!({ "agent": aid })).await;
    approve_merge(&ann, &rid, &abr).await;
    let e = wait_state(&ann, &rid, &abr, "merged").await;
    assert_eq!(e["detail"], "Does what the card asks.", "the reviewer's note stays on the entry");
    assert_eq!(std::fs::read_to_string(r.join("src/lib.rs")).unwrap(), with(REFUND));

    // Finishing released the claim, so Cy's agent can write, on the old base.
    until("the release reaches Cy", || async {
        cy.ok("claims.list", json!({ "room": rid })).await.as_array().unwrap().is_empty().then_some(())
    })
    .await;
    cy.ok("agent.prompt", json!({ "agent": bid, "text": "try again" })).await;
    assert!(messages(&cy, &rid, &bid).await.contains(&"write: ok".to_string()));

    // Both tasks appended a function after charge(): git conflicts, Polyph
    // keeps both.
    cy.ok("task.finish", json!({ "agent": bid, "message": "Add invoices" })).await;
    approve_merge(&ann, &rid, &bbr).await;
    let e = wait_state(&cy, &rid, &bbr, "merged").await;
    assert_eq!(e["resolved"], json!([["src/lib.rs", "symbols"]]), "{e}");
    assert_eq!(std::fs::read_to_string(r.join("src/lib.rs")).unwrap(), format!("{LIB}\n{REFUND}\n{INVOICE}"));
    let log = git(r, &["log", "--format=%s", "main"]);
    assert!(log.starts_with(&format!("Merge {bbr} into main: Add invoices")), "{log}");

    // Everyone sees both merges and both cards done.
    for n in [&ann, &bo, &cy] {
        until("both merges and both cards are done on every device", || async {
            let q = n.ok("merge.queue", json!({ "room": rid })).await;
            let b = n.ok("board.get", json!({ "room": rid })).await;
            let done = b["cards"].as_array().unwrap().iter().filter(|c| c["column"] == "done").count();
            (q.as_array().unwrap().iter().filter(|e| e["state"] == "merged").count() == 2 && done == 2).then_some(())
        })
        .await;
    }
}
