//! LAN mode: an embedded relay with a self-signed certificate, advertised
//! over mDNS as `_polyph._tcp`, plus an optional bridge to an internet relay.
//! Advertisements carry the room's display name and nothing else.

use std::collections::{HashSet, VecDeque};
use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;
use std::time::Duration;

use mdns_sd::{ServiceDaemon, ServiceEvent, ServiceInfo};
use polyph_proto::cert::CertChain;
use polyph_proto::identity::DeviceIdentity;
use polyph_proto::ids::RoomId;
use polyph_proto::relay_wire::{ClientFrame, RelayFrame};
use polyph_relay::client::RelayClient;
use polyph_relay::{RelayConfig, RelayHandle, ServeOptions};
use serde::Serialize;
use tokio::task::JoinHandle;
use tracing::{debug, info, warn};

use crate::runtime::RelayAddr;

pub const SERVICE: &str = "_polyph._tcp.local.";

pub struct LanRoom {
    pub relay: RelayHandle,
    pub addr: RelayAddr,
    pub fingerprint: [u8; 32],
    mdns: Option<ServiceDaemon>,
    bridge: Option<JoinHandle<()>>,
}

impl std::fmt::Debug for LanRoom {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "LanRoom({})", self.addr.url)
    }
}

impl Drop for LanRoom {
    fn drop(&mut self) {
        if let Some(b) = self.bridge.take() {
            b.abort();
        }
        if let Some(m) = self.mdns.take() {
            let _ = m.shutdown();
        }
    }
}

/// First non-loopback IPv4 address, for the URL other machines will dial.
pub fn lan_ip() -> Option<IpAddr> {
    if_addrs::get_if_addrs().ok()?.into_iter().filter(|i| !i.is_loopback()).map(|i| i.ip()).find(|ip| ip.is_ipv4())
}

/// Starts the embedded relay. `bind_ip` is normally 0.0.0.0; tests use 127.0.0.1.
pub async fn start(room_name: &str, bind_ip: IpAddr, advertise: bool) -> anyhow::Result<LanRoom> {
    let host_ip = if bind_ip.is_unspecified() { lan_ip().unwrap_or(IpAddr::from([127, 0, 0, 1])) } else { bind_ip };
    let ss = polyph_relay::tls::self_signed(&[host_ip.to_string(), "polyph.local".into()])?;
    let mut cfg = RelayConfig::ephemeral();
    cfg.bind = SocketAddr::new(bind_ip, 0);
    let relay = polyph_relay::serve(cfg, ServeOptions { tls: Some(ss.config.clone()), ..Default::default() }).await?;
    let port = relay.addr.port();
    let addr = RelayAddr { url: format!("wss://{host_ip}:{port}/ws"), pin: Some(ss.fingerprint) };
    let mdns = if advertise {
        match advertise_room(room_name, host_ip, port) {
            Ok(d) => Some(d),
            Err(e) => {
                warn!(error = %e, "mDNS advertising failed; others can still join with the code and address");
                None
            }
        }
    } else {
        None
    };
    info!(url = %addr.url, "LAN relay started");
    Ok(LanRoom { relay, addr, fingerprint: ss.fingerprint, mdns, bridge: None })
}

fn advertise_room(name: &str, ip: IpAddr, port: u16) -> anyhow::Result<ServiceDaemon> {
    let d = ServiceDaemon::new()?;
    let instance = format!("polyph-{}", &hex::encode(polyph_proto::ids::ObjectId::random()?.0)[..8]);
    let host = format!("{instance}.local.");
    let display: String = name.chars().take(60).collect();
    let props = [("v", "1"), ("name", display.as_str())];
    let info = ServiceInfo::new(SERVICE, &instance, &host, ip, port, &props[..])?;
    d.register(info)?;
    Ok(d)
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct Discovered {
    pub name: String,
    pub url: String,
}

/// Browses for LAN rooms for `wait`. Reveals only that a room exists and its name.
pub async fn discover(wait: Duration) -> anyhow::Result<Vec<Discovered>> {
    let d = ServiceDaemon::new()?;
    let rx = d.browse(SERVICE)?;
    let mut out: Vec<Discovered> = vec![];
    let deadline = tokio::time::Instant::now() + wait;
    loop {
        let left = deadline.saturating_duration_since(tokio::time::Instant::now());
        if left.is_zero() {
            break;
        }
        match tokio::time::timeout(left, rx.recv_async()).await {
            Ok(Ok(ServiceEvent::ServiceResolved(info))) => {
                let name = info.get_property_val_str("name").unwrap_or("").to_string();
                let port = info.get_port();
                if let Some(ip) =
                    info.get_addresses().iter().find(|a| a.is_ipv4()).or_else(|| info.get_addresses().iter().next())
                {
                    let url = format!("wss://{}:{port}/ws", ip.to_ip_addr());
                    let item = Discovered { name, url };
                    if !out.contains(&item) {
                        out.push(item);
                    }
                }
            }
            Ok(Ok(_)) => {}
            _ => break,
        }
    }
    let _ = d.shutdown();
    Ok(out)
}

/// Copies envelopes both ways between the LAN relay and an internet relay so
/// remote members and LAN members share one room.
pub fn start_bridge(
    lan: &mut LanRoom,
    room: RoomId,
    chain: CertChain,
    identity: Arc<DeviceIdentity>,
    remote: RelayAddr,
) {
    let local = lan.addr.clone();
    lan.bridge = Some(tokio::spawn(async move {
        loop {
            if let Err(e) = bridge_once(room, &chain, &identity, &local, &remote).await {
                debug!(error = %e, "bridge interrupted");
            }
            tokio::time::sleep(Duration::from_secs(3)).await;
        }
    }));
}

async fn bridge_once(
    room: RoomId,
    chain: &CertChain,
    id: &DeviceIdentity,
    a: &RelayAddr,
    b: &RelayAddr,
) -> anyhow::Result<()> {
    let ca = RelayClient::connect_with(&a.url, id, a.tls()).await?;
    let cb = RelayClient::connect_with(&b.url, id, b.tls()).await?;
    let (sa, mut ra, ta) = ca.into_parts();
    let (sb, mut rb, tb) = cb.into_parts();
    for s in [&sa, &sb] {
        s.send(ClientFrame::Subscribe { room, chain: Some(Box::new(chain.clone())), after: None }).await?;
    }
    let mut seen = Seen::default();
    let res = loop {
        tokio::select! {
            f = ra.recv() => match f {
                Some(RelayFrame::Deliver { envelope, .. }) => if seen.insert(&envelope) { sb.send(ClientFrame::Forward { envelope }).await? },
                Some(_) => {}
                None => break Ok(()),
            },
            f = rb.recv() => match f {
                Some(RelayFrame::Deliver { envelope, .. }) => if seen.insert(&envelope) { sa.send(ClientFrame::Forward { envelope }).await? },
                Some(_) => {}
                None => break Ok(()),
            },
        }
    };
    ta.abort();
    tb.abort();
    res
}

/// Remembers recently bridged envelopes so they are not echoed back.
#[derive(Default)]
struct Seen {
    set: HashSet<[u8; 16]>,
    order: VecDeque<[u8; 16]>,
}

impl Seen {
    fn insert(&mut self, env: &[u8]) -> bool {
        let h = blake3::hash(env);
        let mut k = [0u8; 16];
        k.copy_from_slice(&h.as_bytes()[..16]);
        if !self.set.insert(k) {
            return false;
        }
        self.order.push_back(k);
        if self.order.len() > 8192
            && let Some(old) = self.order.pop_front()
        {
            self.set.remove(&old);
        }
        true
    }
}
