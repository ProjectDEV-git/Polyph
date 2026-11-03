//! Time source. Tests drive a manual clock so elections and leases can be
//! exercised without waiting in real time.

use std::sync::Arc;
use std::sync::atomic::{AtomicI64, Ordering};

#[derive(Debug, Clone, Default)]
pub struct Clock {
    manual: Option<Arc<AtomicI64>>,
}

impl Clock {
    pub fn system() -> Self {
        Self { manual: None }
    }

    pub fn manual(start: i64) -> Self {
        Self { manual: Some(Arc::new(AtomicI64::new(start))) }
    }

    pub fn now(&self) -> i64 {
        match &self.manual {
            Some(t) => t.load(Ordering::SeqCst),
            None => polyph_proto::now(),
        }
    }

    pub fn advance(&self, secs: i64) {
        if let Some(t) = &self.manual {
            t.fetch_add(secs, Ordering::SeqCst);
        }
    }
}
