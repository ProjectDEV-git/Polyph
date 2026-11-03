//! `polyph-loadtest`: starts `polyph-relay` as a child process, runs the
//! load, and reports the relay's peak memory.
//!
//! cargo build --release -p polyph-relay -p polyph-testkit
//! target/release/polyph-loadtest --relay-bin target/release/polyph-relay

use std::process::Stdio;
use std::time::Duration;

use clap::Parser;
use polyph_testkit::load::{LoadParams, rss_kib, run};

#[derive(Parser, Debug)]
struct Args {
    #[arg(long, default_value = "target/release/polyph-relay")]
    relay_bin: std::path::PathBuf,
    #[arg(long, default_value_t = 50)]
    rooms: usize,
    #[arg(long, default_value_t = 5)]
    members: usize,
    #[arg(long, default_value_t = 60)]
    seconds: u64,
    #[arg(long, default_value_t = 2)]
    msgs_per_sec: u32,
    #[arg(long, default_value_t = 600)]
    payload: usize,
    #[arg(long, default_value_t = 18787)]
    port: u16,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let a = Args::parse();
    let dir = std::env::temp_dir().join(format!("polyph-load-{}", std::process::id()));
    std::fs::create_dir_all(&dir)?;
    let mut child = tokio::process::Command::new(&a.relay_bin)
        .env("POLYPH_RELAY_BIND", format!("127.0.0.1:{}", a.port))
        .env("POLYPH_RELAY_DATABASE", dir.join("relay.db"))
        // All load clients come from 127.0.0.1, so lift the per-IP connection limit.
        .env("POLYPH_RELAY_CONNECTIONS_PER_IP_PER_MIN", "100000")
        .env("RUST_LOG", "warn")
        .stdout(Stdio::null())
        .stderr(Stdio::inherit())
        .kill_on_drop(true)
        .spawn()?;
    tokio::time::sleep(Duration::from_millis(500)).await;
    let pid = child.id().unwrap_or(0);
    let idle = rss_kib(pid);
    let p = LoadParams {
        url: format!("ws://127.0.0.1:{}/ws", a.port),
        rooms: a.rooms,
        members: a.members,
        seconds: a.seconds,
        msgs_per_sec: a.msgs_per_sec,
        payload: a.payload,
    };
    let rep = run(&p).await?;
    let after = rss_kib(pid);
    child.kill().await.ok();
    let _ = std::fs::remove_dir_all(&dir);
    let out = serde_json::json!({
        "rooms": a.rooms, "members_per_room": a.members, "seconds": a.seconds,
        "msgs_per_sec_per_member": a.msgs_per_sec, "payload_bytes": a.payload,
        "sent": rep.sent, "received": rep.received, "errors": rep.errors,
        "relay_rss_idle_kib": idle.map(|v| v.1),
        "relay_rss_peak_kib": after.map(|v| v.0),
        "relay_rss_end_kib": after.map(|v| v.1),
    });
    println!("{}", serde_json::to_string_pretty(&out)?);
    Ok(())
}
