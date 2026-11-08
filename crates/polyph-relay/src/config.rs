//! Relay configuration: environment variables, then an optional TOML file.

use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::Deserialize;

/// Plan limits the relay enforces. `None` means unlimited, which is the
/// default for self-hosted relays. The hosted relay loads these per plan
/// from the cloud service; a plain config file can set one global plan.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct Limits {
    /// Distinct members (people) per room.
    pub max_members: Option<usize>,
    /// Days to keep encrypted room snapshots after the last write.
    pub retention_days: Option<u32>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(default)]
pub struct RelayConfig {
    pub bind: SocketAddr,
    /// SQLite database path. `:memory:` keeps everything in RAM.
    pub database: String,
    /// Serve Prometheus metrics at `/metrics`. Off by default.
    pub metrics: bool,
    pub limits: Limits,
    /// How long envelopes stay in a room's replay buffer.
    pub buffer_secs: u64,
    pub buffer_max_envelopes: usize,
    pub buffer_max_bytes: usize,
    /// Budget for all rooms' buffers together. When exceeded, the room being
    /// written to sheds its oldest envelopes first.
    pub buffer_total_max_bytes: usize,
    /// Snapshots kept per room (latest per device, newest first).
    pub snapshots_per_room: usize,
    /// Connections allowed per IP per minute.
    pub connections_per_ip_per_min: u32,
    /// Join-code attempts allowed per IP per ten minutes.
    pub join_attempts_per_ip: u32,
    /// Frames a connection may send per second (burst equals twice this).
    pub frames_per_sec: u32,
    /// Failed attempts before a code dies (one-time) or locks (reusable).
    pub code_max_failures: u32,
    pub code_lock_secs: u64,
    /// Seconds a joiner has to prove the code; a join still unconfirmed by the
    /// host after this ends and counts as a failed attempt.
    pub join_handshake_secs: u64,
    /// Longest a host may keep a code alive.
    pub code_max_lifetime_secs: u64,
    /// Public URL clients should use, reported by `/healthz`.
    pub public_url: Option<String>,
    /// TLS certificate and key (PEM). Usually Caddy terminates TLS instead.
    pub tls_cert: Option<PathBuf>,
    pub tls_key: Option<PathBuf>,
    /// A header that holds the client's address, set by a proxy every
    /// connection must pass through (`fly-client-ip` on Fly.io). Unset, only
    /// `X-Forwarded-For` from a proxy on the same machine is trusted.
    pub client_ip_header: Option<String>,
}

impl Default for RelayConfig {
    fn default() -> Self {
        Self {
            bind: SocketAddr::from(([127, 0, 0, 1], 8787)),
            database: "polyph-relay.db".into(),
            metrics: false,
            limits: Limits::default(),
            buffer_secs: 600,
            buffer_max_envelopes: 5_000,
            buffer_max_bytes: 2 * 1024 * 1024,
            buffer_total_max_bytes: 64 * 1024 * 1024,
            snapshots_per_room: 10,
            connections_per_ip_per_min: 120,
            join_attempts_per_ip: 20,
            frames_per_sec: 200,
            code_max_failures: 5,
            code_lock_secs: 600,
            join_handshake_secs: 120,
            code_max_lifetime_secs: 30 * 24 * 3600,
            public_url: None,
            tls_cert: None,
            tls_key: None,
            client_ip_header: None,
        }
    }
}

impl RelayConfig {
    /// In-memory configuration for tests and embedded (LAN) relays.
    pub fn ephemeral() -> Self {
        Self { bind: SocketAddr::from(([127, 0, 0, 1], 0)), database: ":memory:".into(), ..Self::default() }
    }

    pub fn buffer_age(&self) -> Duration {
        Duration::from_secs(self.buffer_secs)
    }

    /// Loads a TOML file (if given), then applies `POLYPH_RELAY_*` variables.
    pub fn load(path: Option<&Path>) -> anyhow::Result<Self> {
        let mut cfg = match path {
            Some(p) => {
                let text =
                    std::fs::read_to_string(p).map_err(|e| anyhow::anyhow!("cannot read {}: {e}", p.display()))?;
                toml::from_str(&text).map_err(|e| anyhow::anyhow!("invalid relay config {}: {e}", p.display()))?
            }
            None => Self::default(),
        };
        let var = |k: &str| std::env::var(format!("POLYPH_RELAY_{k}")).ok();
        if let Some(v) = var("BIND") {
            cfg.bind = v.parse().map_err(|e| anyhow::anyhow!("POLYPH_RELAY_BIND: {e}"))?;
        }
        if let Some(v) = var("DATABASE") {
            cfg.database = v;
        }
        if let Some(v) = var("METRICS") {
            cfg.metrics = matches!(v.as_str(), "1" | "true" | "yes");
        }
        if let Some(v) = var("MAX_MEMBERS") {
            cfg.limits.max_members = Some(v.parse().map_err(|e| anyhow::anyhow!("POLYPH_RELAY_MAX_MEMBERS: {e}"))?);
        }
        if let Some(v) = var("RETENTION_DAYS") {
            cfg.limits.retention_days =
                Some(v.parse().map_err(|e| anyhow::anyhow!("POLYPH_RELAY_RETENTION_DAYS: {e}"))?);
        }
        if let Some(v) = var("CONNECTIONS_PER_IP_PER_MIN") {
            cfg.connections_per_ip_per_min =
                v.parse().map_err(|e| anyhow::anyhow!("POLYPH_RELAY_CONNECTIONS_PER_IP_PER_MIN: {e}"))?;
        }
        if let Some(v) = var("PUBLIC_URL") {
            cfg.public_url = Some(v);
        }
        if let Some(v) = var("TLS_CERT") {
            cfg.tls_cert = Some(v.into());
        }
        if let Some(v) = var("TLS_KEY") {
            cfg.tls_key = Some(v.into());
        }
        if let Some(v) = var("CLIENT_IP_HEADER").filter(|v| !v.is_empty()) {
            cfg.client_ip_header = Some(v);
        }
        Ok(cfg)
    }
}

/// Plan enforcement hook. The open-source relay uses [`StaticPolicy`]; the
/// proprietary cloud build supplies a per-account implementation.
pub trait Policy: Send + Sync + std::fmt::Debug {
    fn limits_for_room(&self, owner: Option<&polyph_proto::ids::DeviceId>) -> Limits;
}

#[derive(Debug, Clone, Default)]
pub struct StaticPolicy(pub Limits);

impl Policy for StaticPolicy {
    fn limits_for_room(&self, _owner: Option<&polyph_proto::ids::DeviceId>) -> Limits {
        self.0.clone()
    }
}
