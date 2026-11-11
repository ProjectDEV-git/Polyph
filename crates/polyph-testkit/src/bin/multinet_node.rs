//! One Polyph daemon as a child process, for the multi-network test
//! (`tests/multinet.rs`, `scripts/multinet.sh`). The test starts it inside a
//! network namespace and drives it over stdio: one JSON request per line,
//! `{"id":1,"method":"room.list","params":{}}`, one JSON answer per line,
//! `{"id":1,"ok":...}` or `{"id":1,"err":"..."}`. It exits when stdin closes.

use std::sync::Arc;

use anyhow::Context;
use clap::Parser;
use polyph_agents::registry::AgentConfig;
use polyph_daemon::{Daemon, DaemonOptions, Paths};
use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

#[derive(Parser)]
struct Args {
    #[arg(long)]
    home: std::path::PathBuf,
    #[arg(long)]
    name: String,
    #[arg(long)]
    relay: String,
    /// JSON file: agent kind -> `AgentConfig`.
    #[arg(long)]
    agents: std::path::PathBuf,
    #[arg(long)]
    fake_agent: String,
    #[arg(long)]
    sandbox_helper: String,
    #[arg(long)]
    mcp: String,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let a = Args::parse();
    let mut opts = DaemonOptions::new(Paths::at(&a.home));
    opts.tick_every = std::time::Duration::from_millis(100);
    // A real network with a real clock: no LAN discovery, no session bus.
    opts.mdns = false;
    opts.file_keystore = true;
    opts.dbus = "off".into();
    opts.sandbox_helper = vec![a.sandbox_helper];
    opts.mcp_command = vec![a.mcp];
    opts.extra_agents.insert("fake".into(), AgentConfig { command: a.fake_agent, ..Default::default() });
    let extra: std::collections::BTreeMap<String, AgentConfig> =
        serde_json::from_slice(&std::fs::read(&a.agents).context("agents file")?)?;
    opts.extra_agents.extend(extra);
    if polyph_sandbox::best(&opts.sandbox_helper, false) == polyph_sandbox::Mechanism::None {
        opts.sandbox = Some(polyph_sandbox::Mechanism::None);
    }
    let daemon = Daemon::start(opts).await?;
    daemon.call("config.set", json!({ "key": "relay", "value": a.relay })).await.map_err(|e| anyhow::anyhow!("{e}"))?;
    daemon.call("config.set", json!({ "key": "name", "value": a.name })).await.map_err(|e| anyhow::anyhow!("{e}"))?;

    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<String>();
    let writer = tokio::spawn(async move {
        let mut out = tokio::io::stdout();
        while let Some(line) = rx.recv().await {
            if out.write_all(line.as_bytes()).await.is_err() || out.write_all(b"\n").await.is_err() {
                break;
            }
            let _ = out.flush().await;
        }
    });
    tx.send(json!({ "ready": true }).to_string())?;
    let mut lines = BufReader::new(tokio::io::stdin()).lines();
    while let Some(line) = lines.next_line().await? {
        let req: Value = serde_json::from_str(&line)?;
        let (d, tx) = (Arc::clone(&daemon), tx.clone());
        // Requests run concurrently: `join` waits for an approval that a later request gives.
        tokio::spawn(async move {
            let method = req["method"].as_str().unwrap_or_default();
            let answer = match d.call(method, req["params"].clone()).await {
                Ok(v) => json!({ "id": req["id"], "ok": v }),
                Err(e) => json!({ "id": req["id"], "err": e.to_string() }),
            };
            let _ = tx.send(answer.to_string());
        });
    }
    daemon.stop().await;
    drop(tx);
    let _ = writer.await;
    Ok(())
}
