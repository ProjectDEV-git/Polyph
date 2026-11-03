//! The relay's routing state: rooms, replay buffers, join codes and join
//! sessions. All locks are synchronous and never held across an await.

use std::collections::{HashMap, VecDeque};
use std::net::IpAddr;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{Duration, Instant};

use bytes::Bytes;
use parking_lot::Mutex;
use polyph_proto::envelope::Envelope;
use polyph_proto::identity::{ctx, verify};
use polyph_proto::ids::{DeviceId, RendezvousId, RoomId, Sig};
use polyph_proto::relay_wire::{
    ClientFrame, CodeClosed, CodeKind, ErrorCode, MAX_SNAPSHOT, RelayFrame, end_room_message,
};
use tokio::sync::{Notify, mpsc};
use tracing::{debug, info, warn};

use crate::config::{Policy, RelayConfig};
use crate::limiter::KeyedLimiter;
use crate::metrics::{Metrics, add, inc};
use crate::store::{RoomRecord, Store};

/// Largest single PAKE/join message forwarded between joiner and host.
pub const MAX_PAKE_MESSAGE: usize = 64 * 1024;
/// Messages allowed per join session in each direction combined.
pub const MAX_PAKE_MESSAGES: u32 = 64;
/// Join sessions older than this are dropped (covers waiting for approval).
pub const SESSION_TTL: Duration = Duration::from_secs(15 * 60);
/// Outgoing queue per connection; a client that falls this far behind is dropped.
pub const OUTGOING_QUEUE: usize = 4096;

pub fn unix_now() -> i64 {
    polyph_proto::now()
}

/// One live client connection.
#[derive(Debug)]
pub struct Conn {
    pub id: u64,
    pub ip: IpAddr,
    pub device: Mutex<Option<DeviceId>>,
    tx: mpsc::Sender<Bytes>,
    pub kill: Notify,
    dead: AtomicBool,
}

impl Conn {
    pub fn new(id: u64, ip: IpAddr) -> (Arc<Self>, mpsc::Receiver<Bytes>) {
        let (tx, rx) = mpsc::channel(OUTGOING_QUEUE);
        (Arc::new(Self { id, ip, device: Mutex::new(None), tx, kill: Notify::new(), dead: AtomicBool::new(false) }), rx)
    }

    pub fn device(&self) -> Option<DeviceId> {
        *self.device.lock()
    }

    /// Queues an encoded frame. A full queue means the client cannot keep up;
    /// it is disconnected rather than letting memory grow.
    fn push(&self, bytes: Bytes, metrics: &Metrics) {
        if self.dead.load(Ordering::Relaxed) {
            return;
        }
        if self.tx.try_send(bytes).is_err() {
            self.dead.store(true, Ordering::Relaxed);
            inc(&metrics.slow_consumers_total);
            self.kill.notify_one();
        }
    }
}

#[derive(Debug)]
struct Buffered {
    cursor: u64,
    at: Instant,
    frame: Bytes,
}

#[derive(Debug)]
struct Sub {
    conn: Arc<Conn>,
    writer: bool,
}

#[derive(Debug, Default)]
struct Room {
    subs: HashMap<u64, Sub>,
    buffer: VecDeque<Buffered>,
    buffer_bytes: usize,
    /// Last cursor assigned; 0 means none yet.
    last_cursor: u64,
}

#[derive(Debug)]
struct Code {
    host: Arc<Conn>,
    host_device: DeviceId,
    kind: CodeKind,
    expires_at: i64,
    uses: u32,
    failures: u32,
    locked_until: Option<i64>,
}

#[derive(Debug)]
struct Session {
    rendezvous: RendezvousId,
    joiner: Arc<Conn>,
    host: Arc<Conn>,
    started: Instant,
    messages: u32,
    resolved: bool,
}

#[derive(Debug)]
struct Inner {
    rooms: HashMap<RoomId, Room>,
    /// Sum of all rooms' `buffer_bytes`.
    buffered: usize,
    codes: HashMap<RendezvousId, Code>,
    sessions: HashMap<u64, Session>,
    ip_connections: KeyedLimiter<IpAddr>,
    ip_joins: KeyedLimiter<IpAddr>,
}

/// Optional tap on raw inbound frames, used by tests to prove that no
/// plaintext ever reaches the relay.
pub type Capture = Arc<Mutex<Vec<Vec<u8>>>>;

#[derive(Debug)]
pub struct Hub {
    pub cfg: RelayConfig,
    pub policy: Arc<dyn Policy>,
    pub store: Store,
    pub metrics: Metrics,
    pub capture: Option<Capture>,
    next_conn: AtomicU64,
    inner: Mutex<Inner>,
}

fn encode(f: &RelayFrame) -> Bytes {
    // Encoding our own enum into a Vec cannot fail; fall back to an empty
    // frame (which clients reject) rather than panicking if it ever did.
    Bytes::from(f.encode().unwrap_or_default())
}

fn error(code: ErrorCode, detail: &str) -> RelayFrame {
    RelayFrame::Error { code, detail: detail.to_string() }
}

impl Hub {
    pub fn new(cfg: RelayConfig, policy: Arc<dyn Policy>, store: Store, capture: Option<Capture>) -> Arc<Self> {
        let inner = Inner {
            rooms: HashMap::new(),
            buffered: 0,
            codes: HashMap::new(),
            sessions: HashMap::new(),
            ip_connections: KeyedLimiter::per_window(cfg.connections_per_ip_per_min, 60),
            ip_joins: KeyedLimiter::per_window(cfg.join_attempts_per_ip, 600),
        };
        Arc::new(Self {
            cfg,
            policy,
            store,
            metrics: Metrics::default(),
            capture,
            next_conn: AtomicU64::new(1),
            inner: Mutex::new(inner),
        })
    }

    /// Admits a new connection from `ip`, or refuses it when the IP is over its limit.
    pub fn connect(&self, ip: IpAddr) -> Option<(Arc<Conn>, mpsc::Receiver<Bytes>)> {
        if !self.inner.lock().ip_connections.take(&ip) {
            inc(&self.metrics.rate_limited_total);
            return None;
        }
        inc(&self.metrics.connections_total);
        inc(&self.metrics.connections_open);
        Some(Conn::new(self.next_conn.fetch_add(1, Ordering::Relaxed), ip))
    }

    pub fn send(&self, conn: &Conn, f: &RelayFrame) {
        conn.push(encode(f), &self.metrics);
    }

    pub fn stats(&self) -> (usize, usize, usize) {
        let g = self.inner.lock();
        (g.rooms.len(), g.rooms.values().map(|r| r.buffer_bytes).sum(), g.codes.len())
    }

    /// Handles one authenticated frame.
    pub async fn handle(&self, conn: &Arc<Conn>, device: DeviceId, frame: ClientFrame) {
        match frame {
            ClientFrame::Hello { .. } | ClientFrame::Auth { .. } => {
                self.send(conn, &error(ErrorCode::BadFrame, "already authenticated"));
            }
            ClientFrame::Ping => self.send(conn, &RelayFrame::Pong),
            ClientFrame::RegisterRoom { genesis } => match polyph_proto::cert::open_genesis(&genesis) {
                Ok((g, room)) if g.owner.sign == device => {
                    let rec = RoomRecord { owner: device, quick: g.quick };
                    if let Err(e) = self.store.register_room(&room, &rec, unix_now()).await {
                        warn!(error = %e, "register room failed");
                        self.send(conn, &error(ErrorCode::Internal, "storage error"));
                    }
                }
                _ => self.send(conn, &error(ErrorCode::Forbidden, "genesis must be signed by this device")),
            },
            ClientFrame::Subscribe { room, chain, after } => {
                self.subscribe(conn, device, room, chain.map(|c| *c), after).await
            }
            ClientFrame::Unsubscribe { room } => {
                let mut g = self.inner.lock();
                if let Some(r) = g.rooms.get_mut(&room) {
                    r.subs.remove(&conn.id);
                }
            }
            ClientFrame::Publish { envelope } => self.publish(conn, device, envelope),
            ClientFrame::PutSnapshot { room, epoch, blob } => {
                if blob.len() > MAX_SNAPSHOT {
                    self.send(conn, &error(ErrorCode::TooLarge, "snapshot too large"));
                    return;
                }
                if !self.is_subscribed(conn, &room, true) {
                    self.send(conn, &error(ErrorCode::NotMember, "subscribe as a member before storing snapshots"));
                    return;
                }
                if let Err(e) =
                    self.store.put_snapshot(&room, &device, epoch, &blob, unix_now(), self.cfg.snapshots_per_room).await
                {
                    warn!(error = %e, "snapshot store failed");
                    self.send(conn, &error(ErrorCode::Internal, "storage error"));
                }
            }
            ClientFrame::GetSnapshots { room } => {
                if !self.is_subscribed(conn, &room, false) {
                    self.send(conn, &error(ErrorCode::NotMember, "subscribe before fetching snapshots"));
                    return;
                }
                match self.store.snapshots(&room).await {
                    Ok(items) => self.send(conn, &RelayFrame::Snapshots { room, items }),
                    Err(e) => {
                        warn!(error = %e, "snapshot fetch failed");
                        self.send(conn, &error(ErrorCode::Internal, "storage error"));
                    }
                }
            }
            ClientFrame::EndRoom { room, at, sig } => self.end_room(conn, room, at, sig).await,
            ClientFrame::HostCode { rendezvous, kind, expires_at } => {
                self.host_code(conn, device, rendezvous, kind, expires_at)
            }
            ClientFrame::RevokeCode { rendezvous } => {
                let mut g = self.inner.lock();
                if g.codes.get(&rendezvous).is_some_and(|c| c.host_device == device) {
                    self.close_code(&mut g, &rendezvous, CodeClosed::Revoked);
                }
            }
            ClientFrame::JoinCode { rendezvous } => self.join_code(conn, rendezvous),
            ClientFrame::Pake { session, data } => self.pake(conn, session, data),
            ClientFrame::PakeResult { session, ok } => self.pake_result(conn, session, ok),
        }
    }

    fn is_subscribed(&self, conn: &Conn, room: &RoomId, writer: bool) -> bool {
        let g = self.inner.lock();
        g.rooms.get(room).and_then(|r| r.subs.get(&conn.id)).is_some_and(|s| s.writer || !writer)
    }

    async fn subscribe(
        &self,
        conn: &Arc<Conn>,
        device: DeviceId,
        room: RoomId,
        chain: Option<polyph_proto::cert::CertChain>,
        after: Option<u64>,
    ) {
        let mut writer = false;
        if let Some(chain) = chain {
            let verified = match chain.verify(unix_now(), &|_| false) {
                Ok(v) => v,
                Err(_) => {
                    self.send(conn, &error(ErrorCode::NotMember, "certificate chain did not verify"));
                    return;
                }
            };
            if verified.room != room || verified.device.sign != device {
                self.send(conn, &error(ErrorCode::NotMember, "certificate is for another room or device"));
                return;
            }
            let limits = self.policy.limits_for_room(Some(&verified.owner));
            if let Some(max) = limits.max_members {
                match self.store.admit_member(&room, &verified.member, max).await {
                    Ok(true) => {}
                    Ok(false) => {
                        self.send(conn, &error(ErrorCode::RoomFull, "this room has reached its plan's member limit"));
                        return;
                    }
                    Err(e) => {
                        warn!(error = %e, "member admission failed");
                        self.send(conn, &error(ErrorCode::Internal, "storage error"));
                        return;
                    }
                }
            }
            writer = true;
        }
        let mut g = self.inner.lock();
        let r = g.rooms.entry(room).or_default();
        r.subs.insert(conn.id, Sub { conn: conn.clone(), writer });
        let oldest = r.buffer.front().map(|b| b.cursor);
        let last = r.last_cursor;
        self.send(conn, &RelayFrame::Subscribed { room, writer, cursor: last });
        let gap = match after {
            // Cursor from a previous relay process, or older than the buffer.
            Some(a) if a > last => true,
            Some(a) => oldest.is_some_and(|o| a + 1 < o) || (oldest.is_none() && a < last),
            None => false,
        };
        if gap {
            self.send(conn, &RelayFrame::Gap { room });
        }
        let from = after.unwrap_or(0);
        for b in r.buffer.iter().filter(|b| b.cursor > from) {
            conn.push(b.frame.clone(), &self.metrics);
        }
        self.send(conn, &RelayFrame::CaughtUp { room, cursor: last });
        debug!(conn = conn.id, room = %room.short(), writer, "subscribed");
    }

    fn publish(&self, conn: &Arc<Conn>, device: DeviceId, bytes: Vec<u8>) {
        let env = match Envelope::decode(&bytes) {
            Ok(e) => e,
            Err(_) => {
                inc(&self.metrics.rejected_frames_total);
                self.send(conn, &error(ErrorCode::BadFrame, "malformed envelope"));
                return;
            }
        };
        if env.sender != device {
            inc(&self.metrics.rejected_frames_total);
            self.send(conn, &error(ErrorCode::Forbidden, "envelope sender is not this device"));
            return;
        }
        let mut guard = self.inner.lock();
        let g = &mut *guard;
        let Some(r) = g.rooms.get_mut(&env.room) else {
            self.send(conn, &error(ErrorCode::NotMember, "subscribe as a member before publishing"));
            return;
        };
        if !r.subs.get(&conn.id).is_some_and(|s| s.writer) {
            inc(&self.metrics.rejected_frames_total);
            self.send(conn, &error(ErrorCode::NotMember, "subscribe as a member before publishing"));
            return;
        }
        r.last_cursor += 1;
        let cursor = r.last_cursor;
        let len = bytes.len();
        let frame = encode(&RelayFrame::Deliver { room: env.room, cursor, envelope: bytes });
        inc(&self.metrics.envelopes_total);
        add(&self.metrics.envelope_bytes_total, len as u64);
        for (id, s) in &r.subs {
            if *id != conn.id {
                s.conn.push(frame.clone(), &self.metrics);
                inc(&self.metrics.deliveries_total);
            }
        }
        let before = r.buffer_bytes;
        r.buffer_bytes += frame.len();
        r.buffer.push_back(Buffered { cursor, at: Instant::now(), frame });
        trim(r, &self.cfg);
        g.buffered = (g.buffered + r.buffer_bytes).saturating_sub(before);
        while g.buffered > self.cfg.buffer_total_max_bytes && r.buffer.len() > 1 {
            if let Some(b) = r.buffer.pop_front() {
                r.buffer_bytes = r.buffer_bytes.saturating_sub(b.frame.len());
                g.buffered = g.buffered.saturating_sub(b.frame.len());
            }
        }
    }

    async fn end_room(&self, conn: &Arc<Conn>, room: RoomId, at: i64, sig: Sig) {
        let rec = match self.store.room(&room).await {
            Ok(Some(r)) => r,
            Ok(None) => {
                self.send(conn, &error(ErrorCode::Forbidden, "room was never registered"));
                return;
            }
            Err(e) => {
                warn!(error = %e, "room lookup failed");
                self.send(conn, &error(ErrorCode::Internal, "storage error"));
                return;
            }
        };
        if (unix_now() - at).abs() > 300
            || verify(&rec.owner, ctx::ROOM_END, &end_room_message(&room, at), &sig).is_err()
        {
            self.send(conn, &error(ErrorCode::Forbidden, "only the room owner can end it"));
            return;
        }
        if let Err(e) = self.store.delete_room(&room).await {
            warn!(error = %e, "room delete failed");
        }
        let mut g = self.inner.lock();
        if let Some(r) = g.rooms.remove(&room) {
            g.buffered = g.buffered.saturating_sub(r.buffer_bytes);
            for s in r.subs.values() {
                self.send(&s.conn, &RelayFrame::RoomEnded { room });
            }
        }
        info!(room = %room.short(), "room ended");
    }

    fn host_code(&self, conn: &Arc<Conn>, device: DeviceId, rendezvous: RendezvousId, kind: CodeKind, expires_at: i64) {
        let now = unix_now();
        if expires_at <= now || expires_at > now + self.cfg.code_max_lifetime_secs as i64 {
            self.send(conn, &error(ErrorCode::BadFrame, "code expiry out of range"));
            return;
        }
        let mut g = self.inner.lock();
        if let Some(existing) = g.codes.get_mut(&rendezvous) {
            if existing.host_device != device {
                self.send(conn, &error(ErrorCode::CodeInUse, "pick another code"));
                return;
            }
            // Same device re-hosting after a reconnect keeps counters.
            existing.host = conn.clone();
            existing.kind = kind;
            existing.expires_at = expires_at;
        } else {
            g.codes.insert(
                rendezvous,
                Code {
                    host: conn.clone(),
                    host_device: device,
                    kind,
                    expires_at,
                    uses: 0,
                    failures: 0,
                    locked_until: None,
                },
            );
        }
        self.send(conn, &RelayFrame::CodeHosted { rendezvous });
    }

    fn join_code(&self, conn: &Arc<Conn>, rendezvous: RendezvousId) {
        let now = unix_now();
        let mut g = self.inner.lock();
        if !g.ip_joins.take(&conn.ip) {
            inc(&self.metrics.rate_limited_total);
            self.send(
                conn,
                &error(ErrorCode::RateLimited, "too many join attempts from this address; wait and try again"),
            );
            return;
        }
        inc(&self.metrics.join_attempts_total);
        let host = match g.codes.get(&rendezvous) {
            Some(c) if c.expires_at > now => {
                if c.locked_until.is_some_and(|u| u > now) {
                    self.send(conn, &error(ErrorCode::CodeLocked, "this code is locked after failed attempts"));
                    return;
                }
                c.host.clone()
            }
            _ => {
                self.send(conn, &error(ErrorCode::CodeUnknown, "no room is waiting for this code"));
                return;
            }
        };
        let mut session = rand::random::<u64>();
        while session == 0 || g.sessions.contains_key(&session) {
            session = rand::random::<u64>();
        }
        g.sessions.insert(
            session,
            Session {
                rendezvous,
                joiner: conn.clone(),
                host: host.clone(),
                started: Instant::now(),
                messages: 0,
                resolved: false,
            },
        );
        self.send(&host, &RelayFrame::JoinerArrived { rendezvous, session });
        self.send(conn, &RelayFrame::JoinStarted { session });
    }

    fn pake(&self, conn: &Arc<Conn>, session: u64, data: Vec<u8>) {
        if data.len() > MAX_PAKE_MESSAGE {
            self.send(conn, &error(ErrorCode::TooLarge, "join message too large"));
            return;
        }
        let mut g = self.inner.lock();
        let Some(s) = g.sessions.get_mut(&session) else {
            self.send(conn, &RelayFrame::PakeEnded { session, ok: false });
            return;
        };
        s.messages += 1;
        if s.messages > MAX_PAKE_MESSAGES {
            self.send(conn, &error(ErrorCode::RateLimited, "too many join messages"));
            return;
        }
        let to = if conn.id == s.joiner.id {
            s.host.clone()
        } else if conn.id == s.host.id {
            s.joiner.clone()
        } else {
            self.send(conn, &error(ErrorCode::Forbidden, "not part of this join"));
            return;
        };
        self.send(&to, &RelayFrame::Pake { session, data });
    }

    fn pake_result(&self, conn: &Arc<Conn>, session: u64, ok: bool) {
        let mut g = self.inner.lock();
        let Some(s) = g.sessions.get_mut(&session) else { return };
        if s.host.id != conn.id || s.resolved {
            return;
        }
        s.resolved = true;
        let rendezvous = s.rendezvous;
        let joiner = s.joiner.clone();
        if ok {
            // The session stays open so join traffic (knock, approval) can continue.
            if let Some(c) = g.codes.get_mut(&rendezvous) {
                c.uses += 1;
                let done = match c.kind {
                    CodeKind::OneTime => true,
                    CodeKind::Reusable { max_uses } => max_uses.is_some_and(|m| c.uses >= m),
                };
                if done {
                    let host = c.host.clone();
                    g.codes.remove(&rendezvous);
                    self.send(&host, &RelayFrame::CodeClosed { rendezvous, reason: CodeClosed::Used });
                }
            }
        } else {
            g.sessions.remove(&session);
            self.send(&joiner, &RelayFrame::PakeEnded { session, ok: false });
            self.record_failure(&mut g, &rendezvous);
        }
    }

    fn record_failure(&self, g: &mut Inner, rendezvous: &RendezvousId) {
        inc(&self.metrics.join_failures_total);
        let max = self.cfg.code_max_failures;
        let Some(c) = g.codes.get_mut(rendezvous) else { return };
        c.failures += 1;
        if c.failures < max {
            return;
        }
        match c.kind {
            CodeKind::OneTime => {
                info!("one-time code closed after repeated failures");
                self.close_code(g, rendezvous, CodeClosed::Failed);
            }
            CodeKind::Reusable { .. } => {
                let until = unix_now() + self.cfg.code_lock_secs as i64;
                c.locked_until = Some(until);
                c.failures = 0;
                let host = c.host.clone();
                info!("reusable code locked after repeated failures");
                self.send(&host, &RelayFrame::CodeLocked { rendezvous: *rendezvous, until });
                // End sessions still in flight on the locked code.
                let ids: Vec<u64> = g
                    .sessions
                    .iter()
                    .filter(|(_, s)| s.rendezvous == *rendezvous && !s.resolved)
                    .map(|(id, _)| *id)
                    .collect();
                for id in ids {
                    if let Some(s) = g.sessions.remove(&id) {
                        self.send(&s.joiner, &RelayFrame::PakeEnded { session: id, ok: false });
                    }
                }
            }
        }
    }

    fn close_code(&self, g: &mut Inner, rendezvous: &RendezvousId, reason: CodeClosed) {
        if let Some(c) = g.codes.remove(rendezvous) {
            self.send(&c.host, &RelayFrame::CodeClosed { rendezvous: *rendezvous, reason });
        }
        let ids: Vec<u64> =
            g.sessions.iter().filter(|(_, s)| s.rendezvous == *rendezvous && !s.resolved).map(|(id, _)| *id).collect();
        for id in ids {
            if let Some(s) = g.sessions.remove(&id) {
                self.send(&s.joiner, &RelayFrame::PakeEnded { session: id, ok: false });
            }
        }
    }

    /// Cleans up everything a closed connection owned.
    pub fn disconnect(&self, conn: &Arc<Conn>) {
        self.metrics.connections_open.fetch_sub(1, Ordering::Relaxed);
        let mut g = self.inner.lock();
        for r in g.rooms.values_mut() {
            r.subs.remove(&conn.id);
        }
        // Codes hosted on this connection close; the daemon re-hosts on reconnect.
        let hosted: Vec<RendezvousId> = g.codes.iter().filter(|(_, c)| c.host.id == conn.id).map(|(r, _)| *r).collect();
        for r in hosted {
            self.close_code(&mut g, &r, CodeClosed::HostLeft);
        }
        let mine: Vec<u64> = g
            .sessions
            .iter()
            .filter(|(_, s)| s.joiner.id == conn.id || s.host.id == conn.id)
            .map(|(id, _)| *id)
            .collect();
        for id in mine {
            if let Some(s) = g.sessions.remove(&id) {
                if s.joiner.id == conn.id {
                    self.send(&s.host, &RelayFrame::PakeEnded { session: id, ok: false });
                    // Abandoning a join before the host confirmed it counts as a
                    // failed attempt, so disconnecting cannot dodge the limit.
                    if !s.resolved {
                        self.record_failure(&mut g, &s.rendezvous);
                    }
                } else {
                    self.send(&s.joiner, &RelayFrame::PakeEnded { session: id, ok: false });
                }
            }
        }
    }

    /// Periodic maintenance: expire codes and sessions, trim buffers, drop idle rooms.
    pub fn sweep(&self) {
        let now = unix_now();
        let mut g = self.inner.lock();
        let expired: Vec<RendezvousId> = g.codes.iter().filter(|(_, c)| c.expires_at <= now).map(|(r, _)| *r).collect();
        for r in expired {
            self.close_code(&mut g, &r, CodeClosed::Expired);
        }
        let stale: Vec<u64> =
            g.sessions.iter().filter(|(_, s)| s.started.elapsed() > SESSION_TTL).map(|(id, _)| *id).collect();
        for id in stale {
            if let Some(s) = g.sessions.remove(&id) {
                self.send(&s.joiner, &RelayFrame::PakeEnded { session: id, ok: false });
                self.send(&s.host, &RelayFrame::PakeEnded { session: id, ok: false });
            }
        }
        for r in g.rooms.values_mut() {
            trim(r, &self.cfg);
        }
        g.rooms.retain(|_, r| !r.subs.is_empty() || !r.buffer.is_empty());
        g.buffered = g.rooms.values().map(|r| r.buffer_bytes).sum();
        g.ip_connections.prune();
        g.ip_joins.prune();
    }

    /// Deletes snapshots older than the configured retention.
    pub async fn expire_snapshots(&self) {
        if let Some(days) = self.policy.limits_for_room(None).retention_days {
            let before = unix_now() - days as i64 * 86_400;
            match self.store.expire_snapshots(before).await {
                Ok(n) if n > 0 => info!(count = n, "expired snapshots"),
                Ok(_) => {}
                Err(e) => warn!(error = %e, "snapshot expiry failed"),
            }
        }
    }

    /// Test helper: whether a room's buffer still exists in memory.
    pub fn room_buffer_len(&self, room: &RoomId) -> usize {
        self.inner.lock().rooms.get(room).map(|r| r.buffer.len()).unwrap_or(0)
    }
}

fn trim(r: &mut Room, cfg: &RelayConfig) {
    let max_age = cfg.buffer_age();
    while let Some(front) = r.buffer.front() {
        let too_old = front.at.elapsed() > max_age;
        let too_many = r.buffer.len() > cfg.buffer_max_envelopes;
        let too_big = r.buffer_bytes > cfg.buffer_max_bytes;
        if !(too_old || too_many || too_big) {
            break;
        }
        if let Some(b) = r.buffer.pop_front() {
            r.buffer_bytes = r.buffer_bytes.saturating_sub(b.frame.len());
        }
    }
}
