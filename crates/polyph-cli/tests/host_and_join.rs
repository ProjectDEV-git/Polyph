//! The host-and-join walk-through with the real `polyph` binary: Ann runs
//! `polyph start`, Bo runs `polyph join <code>`, and every line either of them
//! reads along the way is checked. Each person gets their own temporary
//! POLYPH_HOME and HOME; the relay runs in this process.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

struct Person {
    home: PathBuf,
    user: PathBuf,
}

impl Person {
    fn new(root: &Path, name: &str, relay: &str) -> Self {
        let p = Self { home: root.join(format!("{name}-polyph")), user: root.join(name) };
        std::fs::create_dir_all(&p.user).unwrap();
        let (ok, out) = p.run(&["config", "set", "relay", relay]);
        assert!(ok, "{out}");
        p
    }

    fn cmd(&self, args: &[&str]) -> Command {
        let mut c = Command::new(env!("CARGO_BIN_EXE_polyph"));
        c.args(["--lang", "en", "--home"])
            .arg(&self.home)
            .args(args)
            .env("HOME", &self.user)
            .env_remove("USERPROFILE")
            .env_remove("POLYPH_HOME")
            .env_remove("XDG_CONFIG_HOME")
            .env_remove("POLYPH_LANG")
            .current_dir(&self.user)
            .stdin(Stdio::null());
        c
    }

    fn run(&self, args: &[&str]) -> (bool, String) {
        let out = self.cmd(args).output().unwrap();
        (
            out.status.success(),
            format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr)),
        )
    }

    fn spawn(&self, args: &[&str]) -> Child {
        self.cmd(args).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().unwrap()
    }

    /// Runs `polyph status` until someone is waiting to join; returns their
    /// short knock id.
    fn wait_for_knock(&self, who: &str) -> String {
        let deadline = Instant::now() + Duration::from_secs(30);
        loop {
            let (_, out) = self.run(&["status"]);
            let line = format!("{who} wants to join");
            if let Some(l) = out.lines().find(|l| l.ends_with(&line)) {
                return l.split_whitespace().next().unwrap().to_string();
            }
            assert!(Instant::now() < deadline, "status never showed {who} waiting:\n{out}");
            std::thread::sleep(Duration::from_millis(200));
        }
    }
}

impl Drop for Person {
    fn drop(&mut self) {
        let _ = self.cmd(&["daemon", "stop"]).stdout(Stdio::null()).stderr(Stdio::null()).status();
    }
}

/// Starts `polyph mcp` as Claude Code would and returns it with the agent
/// lines of `team_state`.
#[cfg(unix)]
fn agent(p: &Person) -> (Child, Vec<String>) {
    use std::io::{BufRead, BufReader, Write};
    let mut child =
        p.cmd(&["mcp", "--client", "claude-code"]).stdin(Stdio::piped()).stdout(Stdio::piped()).spawn().unwrap();
    let mut stdin = child.stdin.take().unwrap();
    let mut out = BufReader::new(child.stdout.take().unwrap());
    let mut line = String::new();
    for msg in [
        r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"claude-code","version":"2"}}}"#,
        r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#,
        r#"{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"team_state","arguments":{}}}"#,
    ] {
        writeln!(stdin, "{msg}").unwrap();
        if msg.contains("\"id\"") {
            line.clear();
            out.read_line(&mut line).unwrap();
        }
    }
    let v: serde_json::Value = serde_json::from_str(&line).unwrap();
    let text = v["result"]["content"][0]["text"].as_str().unwrap_or_default().to_string();
    let agents = text.lines().filter(|l| l.contains("Bo/")).map(str::to_string).collect();
    // Keep stdin open: the agent is still running until the caller stops it.
    std::mem::forget(stdin);
    (child, agents)
}

/// MCP clients may stop a stdio server with SIGTERM instead of closing its
/// stdin. The agent must leave the room then too, or its next start shows up
/// as a second agent ("Bo/claude-code2") beside a ghost of the first.
#[cfg(unix)]
fn an_agent_stopped_with_sigterm_leaves_the_room(bo: &Person) {
    let (mut first, agents) = agent(bo);
    assert_eq!(agents.len(), 1, "{agents:?}");
    let ok = Command::new("kill").args(["-TERM", &first.id().to_string()]).status().unwrap().success();
    assert!(ok);
    let deadline = Instant::now() + Duration::from_secs(10);
    while first.try_wait().unwrap().is_none() {
        assert!(Instant::now() < deadline, "polyph mcp did not exit on SIGTERM");
        std::thread::sleep(Duration::from_millis(50));
    }
    let (mut second, agents) = agent(bo);
    assert_eq!(agents.len(), 1, "the stopped agent is still listed: {agents:?}");
    assert!(agents[0].contains("Bo/claude-code "), "{agents:?}");
    second.kill().unwrap();
    let _ = second.wait();
}

fn finish(child: Child) -> (bool, String) {
    let out = child.wait_with_output().unwrap();
    (out.status.success(), format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr)))
}

#[test]
fn ann_hosts_bo_joins_and_each_reads_plain_answers() {
    let rt = tokio::runtime::Builder::new_multi_thread().worker_threads(2).enable_all().build().unwrap();
    let relay = rt
        .block_on(polyph_relay::serve(polyph_relay::RelayConfig::ephemeral(), polyph_relay::ServeOptions::default()))
        .unwrap();
    let url = relay.url();
    let t = tempfile::tempdir().unwrap();
    let ann = Person::new(t.path(), "ann", &url);
    let bo = Person::new(t.path(), "bo", &url);

    // Ann starts a room. --no-connect means she hears nothing about agents.
    let (ok, out) = ann.run(&["start", "--room", "demo", "--me", "Ann", "--yes", "--no-connect"]);
    assert!(ok, "{out}");
    assert!(!out.contains("--no-connect"), "start told Ann to pass the flag she passed:\n{out}");
    let code = out
        .lines()
        .find_map(|l| l.strip_prefix("Teammates join with: polyph join "))
        .unwrap_or_else(|| panic!("no join code in:\n{out}"))
        .trim()
        .to_string();

    // A mistyped code fails at once and never claims to connect.
    let (ok, out) = bo.run(&["join", "ABCD", "--me", "Bo", "--no-clone"]);
    assert!(!ok);
    assert!(!out.contains("Connecting"), "{out}");
    assert!(out.contains("codes look like"), "{out}");

    // Bo knocks; Ann's status shows him; she turns him away with a note.
    let joining = bo.spawn(&["join", &code, "--me", "Bo", "--no-clone"]);
    let id = ann.wait_for_knock("Bo");
    let (_, out) = ann.run(&["status"]);
    assert!(!out.contains("No approvals waiting"), "status contradicts the knock under it:\n{out}");
    let (ok, out) = ann.run(&["deny", &id, "--note", "wrong room, ask Kim"]);
    assert!(ok, "{out}");
    assert!(out.contains("Turned Bo away."), "{out}");
    let (ok, out) = finish(joining);
    assert!(!ok, "{out}");
    assert!(out.contains("Waiting for someone in the room to approve you"), "{out}");
    assert!(out.contains("the room declined the request: wrong room, ask Kim"), "{out}");

    // The code is reusable: Bo knocks again and Ann lets him in.
    let joining = bo.spawn(&["join", &code, "--me", "Bo", "--no-clone"]);
    let id = ann.wait_for_knock("Bo");
    let (ok, out) = ann.run(&["approve", &id]);
    assert!(ok, "{out}");
    assert!(out.contains("Let Bo in as Member."), "{out}");
    let (ok, out) = finish(joining);
    assert!(ok, "{out}");
    assert!(out.contains("Joined demo as Member."), "{out}");

    // Nobody is waiting any more, and both of them are online.
    let (ok, out) = ann.run(&["status"]);
    assert!(ok, "{out}");
    assert!(!out.contains("wants to join"), "{out}");
    assert!(out.contains("2 people online"), "{out}");

    // As a Viewer Bo is told why he cannot write and who can change that.
    let (ok, out) = ann.run(&["room", "role", "Bo", "viewer"]);
    assert!(ok, "{out}");
    assert!(out.contains("Bo's role is now Viewer."), "{out}");
    let (ok, out) = bo.run(&["chat", "hello?"]);
    assert!(!ok);
    assert!(out.contains("Viewers can read the room but not write in it"), "{out}");
    let (ok, out) = ann.run(&["room", "role", "Bo", "member"]);
    assert!(ok, "{out}");
    let (ok, out) = bo.run(&["chat", "back in"]);
    assert!(ok, "{out}");

    #[cfg(unix)]
    an_agent_stopped_with_sigterm_leaves_the_room(&bo);

    drop((ann, bo));
    rt.block_on(relay.shutdown());
}
