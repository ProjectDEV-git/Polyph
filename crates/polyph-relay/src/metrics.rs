//! Counters for the optional Prometheus endpoint. Nothing here is content.

use std::fmt::Write;
use std::sync::atomic::{AtomicU64, Ordering};

#[derive(Debug, Default)]
pub struct Metrics {
    pub connections_total: AtomicU64,
    pub connections_open: AtomicU64,
    pub envelopes_total: AtomicU64,
    pub envelope_bytes_total: AtomicU64,
    pub deliveries_total: AtomicU64,
    pub rejected_frames_total: AtomicU64,
    pub rate_limited_total: AtomicU64,
    pub join_attempts_total: AtomicU64,
    pub join_failures_total: AtomicU64,
    pub slow_consumers_total: AtomicU64,
}

pub fn inc(c: &AtomicU64) {
    c.fetch_add(1, Ordering::Relaxed);
}

pub fn add(c: &AtomicU64, n: u64) {
    c.fetch_add(n, Ordering::Relaxed);
}

impl Metrics {
    pub fn render(&self, rooms: usize, buffered_bytes: usize, codes: usize) -> String {
        let mut s = String::new();
        let mut c = |name: &str, kind: &str, v: u64| {
            let _ = writeln!(s, "# TYPE polyph_relay_{name} {kind}\npolyph_relay_{name} {v}");
        };
        let g = |a: &AtomicU64| a.load(Ordering::Relaxed);
        c("connections_total", "counter", g(&self.connections_total));
        c("connections_open", "gauge", g(&self.connections_open));
        c("envelopes_total", "counter", g(&self.envelopes_total));
        c("envelope_bytes_total", "counter", g(&self.envelope_bytes_total));
        c("deliveries_total", "counter", g(&self.deliveries_total));
        c("rejected_frames_total", "counter", g(&self.rejected_frames_total));
        c("rate_limited_total", "counter", g(&self.rate_limited_total));
        c("join_attempts_total", "counter", g(&self.join_attempts_total));
        c("join_failures_total", "counter", g(&self.join_failures_total));
        c("slow_consumers_total", "counter", g(&self.slow_consumers_total));
        c("rooms_active", "gauge", rooms as u64);
        c("buffered_bytes", "gauge", buffered_bytes as u64);
        c("codes_active", "gauge", codes as u64);
        s
    }
}
