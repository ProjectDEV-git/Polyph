//! Token buckets for rate limiting.

use std::collections::HashMap;
use std::hash::Hash;
use std::time::Instant;

#[derive(Debug, Clone)]
pub struct Bucket {
    tokens: f64,
    last: Instant,
    rate_per_sec: f64,
    burst: f64,
}

impl Bucket {
    pub fn new(rate_per_sec: f64, burst: f64) -> Self {
        Self { tokens: burst, last: Instant::now(), rate_per_sec, burst }
    }

    pub fn take(&mut self) -> bool {
        let now = Instant::now();
        let dt = now.duration_since(self.last).as_secs_f64();
        self.last = now;
        self.tokens = (self.tokens + dt * self.rate_per_sec).min(self.burst);
        if self.tokens >= 1.0 {
            self.tokens -= 1.0;
            true
        } else {
            false
        }
    }

    fn idle_full(&self) -> bool {
        let dt = Instant::now().duration_since(self.last).as_secs_f64();
        self.tokens + dt * self.rate_per_sec >= self.burst
    }
}

/// A bucket per key, e.g. per IP address.
#[derive(Debug)]
pub struct KeyedLimiter<K> {
    buckets: HashMap<K, Bucket>,
    rate_per_sec: f64,
    burst: f64,
}

impl<K: Hash + Eq + Clone> KeyedLimiter<K> {
    /// `count` events allowed per `window_secs`, all usable at once.
    pub fn per_window(count: u32, window_secs: u64) -> Self {
        Self {
            buckets: HashMap::new(),
            rate_per_sec: count as f64 / window_secs.max(1) as f64,
            burst: count.max(1) as f64,
        }
    }

    pub fn take(&mut self, key: &K) -> bool {
        let (r, b) = (self.rate_per_sec, self.burst);
        self.buckets.entry(key.clone()).or_insert_with(|| Bucket::new(r, b)).take()
    }

    /// Drops buckets that have refilled, bounding memory.
    pub fn prune(&mut self) {
        self.buckets.retain(|_, b| !b.idle_full());
    }

    pub fn len(&self) -> usize {
        self.buckets.len()
    }

    pub fn is_empty(&self) -> bool {
        self.buckets.is_empty()
    }
}
