use std::collections::HashMap;
use std::time::{Duration, Instant};

/// Per-token sliding window rate limiter.
///
/// Each key (typically a token ID) gets an independent bucket of timestamps.
/// `check` returns `true` when the caller is still within the allowed request
/// rate and `false` once the limit has been reached for the current window.
pub struct RateLimiter {
    max_per_window: u32,
    window: Duration,
    buckets: HashMap<String, Vec<Instant>>,
}

impl RateLimiter {
    /// Creates a new rate limiter that allows `max_per_window` requests per
    /// sliding `window` duration.
    pub fn new(max_per_window: u32, window: Duration) -> Self {
        Self {
            max_per_window,
            window,
            buckets: HashMap::new(),
        }
    }

    /// Returns `true` if the key is under the rate limit, `false` if exceeded.
    ///
    /// Expired entries are pruned on every call. If the key is still under the
    /// limit after pruning, the current timestamp is recorded and `true` is
    /// returned. Otherwise the timestamp is **not** recorded and `false` is
    /// returned so that rejected requests do not consume quota.
    pub fn check(&mut self, key: &str) -> bool {
        let now = Instant::now();
        let cutoff = now - self.window;

        let bucket = self.buckets.entry(key.to_string()).or_default();

        // Remove timestamps older than the window.
        bucket.retain(|ts| *ts > cutoff);

        if (bucket.len() as u32) < self.max_per_window {
            bucket.push(now);
            true
        } else {
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_allows_within_limit() {
        let mut limiter = RateLimiter::new(3, Duration::from_secs(60));

        assert!(limiter.check("tok_a"));
        assert!(limiter.check("tok_a"));
        assert!(limiter.check("tok_a"));
    }

    #[test]
    fn test_blocks_over_limit() {
        let mut limiter = RateLimiter::new(2, Duration::from_secs(60));

        assert!(limiter.check("tok_b"));
        assert!(limiter.check("tok_b"));
        assert!(!limiter.check("tok_b"));
    }

    #[test]
    fn test_separate_keys() {
        let mut limiter = RateLimiter::new(1, Duration::from_secs(60));

        assert!(limiter.check("tok_x"));
        assert!(!limiter.check("tok_x")); // x is exhausted

        // Different key should still be allowed.
        assert!(limiter.check("tok_y"));
        assert!(!limiter.check("tok_y"));
    }
}
