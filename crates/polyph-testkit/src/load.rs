//! Relay load test: many rooms with several members each, every member
//! publishing encrypted envelopes, against a relay running as a separate
//! process so its memory can be measured on its own.

use std::time::{Duration, Instant};

use polyph_proto::cert::{CertChain, MembershipCert, Role, RoomGenesis};
use polyph_proto::group::EpochKey;
use polyph_proto::identity::DeviceIdentity;
use polyph_proto::ids::MemberId;
use polyph_proto::relay_wire::{ClientFrame, RelayFrame};
use polyph_relay::client::RelayClient;

#[derive(Debug, Clone)]
pub struct LoadParams {
    pub url: String,
    pub rooms: usize,
    pub members: usize,
    pub seconds: u64,
    pub msgs_per_sec: u32,
    pub payload: usize,
}

#[derive(Debug, Default, Clone)]
pub struct LoadReport {
    pub sent: u64,
    pub received: u64,
    pub errors: u64,
    pub elapsed_secs: f64,
}

pub async fn run(p: &LoadParams) -> anyhow::Result<LoadReport> {
    let now = polyph_proto::now();
    let mut tasks = Vec::new();
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<(u64, u64, u64)>();
    for _ in 0..p.rooms {
        let owner = DeviceIdentity::generate()?;
        let om = MemberId::random()?;
        let (g, room) = RoomGenesis::create(&owner, om, now, false)?;
        let chain = CertChain::for_owner(&owner, g, om, [0; 32], now)?;
        let key = EpochKey::generate(room, 1)?;
        let mut members = vec![(owner, chain.clone())];
        for _ in 1..p.members {
            let id = DeviceIdentity::generate()?;
            let cert = MembershipCert {
                room,
                member: MemberId::random()?,
                device: id.public(),
                name_commit: [0; 32],
                role: Role::Member,
                issued_at: now,
                expires_at: None,
                capabilities: vec![],
            };
            let c = chain.issue(&members[0].0, &cert)?;
            members.push((id, c));
        }
        for (id, chain) in members {
            let url = p.url.clone();
            let key = key.clone();
            let p = p.clone();
            let tx = tx.clone();
            tasks.push(tokio::spawn(async move {
                let (mut sent, mut recv, mut errs) = (0u64, 0u64, 0u64);
                let Ok(c) = RelayClient::connect(&url, &id, None).await else {
                    let _ = tx.send((0, 0, 1));
                    return;
                };
                let (s, mut r, _task) = c.into_parts();
                if s.send(ClientFrame::Subscribe { room, chain: Some(Box::new(chain)), after: None }).await.is_err() {
                    let _ = tx.send((0, 0, 1));
                    return;
                }
                let payload = vec![0x5a; p.payload];
                let end = Instant::now() + Duration::from_secs(p.seconds);
                let mut tick = tokio::time::interval(Duration::from_secs_f64(1.0 / p.msgs_per_sec.max(1) as f64));
                let mut seq = 0u64;
                while Instant::now() < end {
                    tokio::select! {
                        _ = tick.tick() => {
                            seq += 1;
                            match key.seal(&id, seq, &payload) {
                                Ok(env) => if s.send(ClientFrame::Publish { envelope: env.encode() }).await.is_ok() { sent += 1 } else { errs += 1 },
                                Err(_) => errs += 1,
                            }
                        }
                        f = r.recv() => match f {
                            Some(RelayFrame::Deliver { .. }) => recv += 1,
                            Some(RelayFrame::Error { .. }) => errs += 1,
                            Some(_) => {}
                            None => { errs += 1; break }
                        }
                    }
                }
                let _ = tx.send((sent, recv, errs));
            }));
        }
    }
    drop(tx);
    let start = Instant::now();
    let mut rep = LoadReport::default();
    while let Some((s, r, e)) = rx.recv().await {
        rep.sent += s;
        rep.received += r;
        rep.errors += e;
    }
    for t in tasks {
        let _ = t.await;
    }
    rep.elapsed_secs = start.elapsed().as_secs_f64();
    Ok(rep)
}

/// Peak and current resident memory of a process in KiB (Linux only).
pub fn rss_kib(pid: u32) -> Option<(u64, u64)> {
    let s = std::fs::read_to_string(format!("/proc/{pid}/status")).ok()?;
    let field = |name: &str| {
        s.lines()
            .find(|l| l.starts_with(name))
            .and_then(|l| l.split_whitespace().nth(1))
            .and_then(|v| v.parse::<u64>().ok())
    };
    Some((field("VmHWM:")?, field("VmRSS:")?))
}
