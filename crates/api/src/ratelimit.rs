//! In-process token bucket, keyed per API key.
//!
//! Good enough for a single node; swap the map for Redis when the API runs on
//! more than one instance.

use dashmap::DashMap;
use std::time::{Duration, Instant};

struct Bucket {
    tokens: f64,
    last: Instant,
}

pub struct RateLimiter {
    buckets: DashMap<String, Bucket>,
}

impl Default for RateLimiter {
    fn default() -> Self {
        Self::new()
    }
}

impl RateLimiter {
    pub fn new() -> Self {
        Self {
            buckets: DashMap::new(),
        }
    }

    /// Returns `Ok(())` if a token was available, else seconds until one is.
    pub fn check(&self, key: &str, per_minute: i32) -> Result<(), u64> {
        let capacity = per_minute.max(1) as f64;
        let refill_per_sec = capacity / 60.0;
        let now = Instant::now();

        let mut entry = self.buckets.entry(key.to_string()).or_insert(Bucket {
            tokens: capacity,
            last: now,
        });

        let elapsed = now.duration_since(entry.last).as_secs_f64();
        entry.tokens = (entry.tokens + elapsed * refill_per_sec).min(capacity);
        entry.last = now;

        if entry.tokens >= 1.0 {
            entry.tokens -= 1.0;
            Ok(())
        } else {
            let need = (1.0 - entry.tokens) / refill_per_sec;
            Err(need.ceil().max(1.0) as u64)
        }
    }

    /// Drops buckets untouched for an hour, so the map can't grow forever.
    pub fn sweep(&self) {
        let cutoff = Duration::from_secs(3600);
        let now = Instant::now();
        self.buckets
            .retain(|_, b| now.duration_since(b.last) < cutoff);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allows_up_to_capacity_then_blocks() {
        let rl = RateLimiter::new();
        for _ in 0..60 {
            assert!(rl.check("k", 60).is_ok());
        }
        assert!(rl.check("k", 60).is_err());
    }

    #[test]
    fn separate_keys_dont_share() {
        let rl = RateLimiter::new();
        assert!(rl.check("a", 1).is_ok());
        assert!(rl.check("b", 1).is_ok());
        assert!(rl.check("a", 1).is_err());
    }
}
