use std::collections::HashMap;
use std::time::Instant;

/// Per-user sliding-window rate limiter.
///
/// Tracks command timestamps per user over a one-minute window.
pub struct RateLimiter {
    /// user_id → list of timestamps within the current window
    limits: HashMap<i64, Vec<Instant>>,
    max_per_minute: u32,
}

impl RateLimiter {
    /// Create a new limiter that allows at most `max_per_minute` commands per
    /// user over a 60-second rolling window.
    pub fn new(max_per_minute: u32) -> Self {
        Self {
            limits: HashMap::new(),
            max_per_minute,
        }
    }

    /// Check whether `user_id` is within their rate limit.
    ///
    /// Returns `true` (and records the request) if the user still has budget.
    /// Returns `false` and does **not** record the request if the limit is
    /// already reached.
    pub fn check(&mut self, user_id: i64) -> bool {
        let now = Instant::now();
        let timestamps = self.limits.entry(user_id).or_default();

        // Evict timestamps older than 60 s
        timestamps.retain(|t| now.duration_since(*t).as_secs() < 60);

        if timestamps.len() >= self.max_per_minute as usize {
            return false;
        }

        timestamps.push(now);
        true
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_allows_requests_within_limit() {
        let mut limiter = RateLimiter::new(5);
        for _ in 0..5 {
            assert!(limiter.check(1), "should allow within limit");
        }
    }

    #[test]
    fn test_blocks_when_limit_exceeded() {
        let mut limiter = RateLimiter::new(3);
        for _ in 0..3 {
            limiter.check(1);
        }
        assert!(!limiter.check(1), "4th request should be blocked");
    }

    #[test]
    fn test_different_users_tracked_independently() {
        let mut limiter = RateLimiter::new(2);
        // Exhaust user 1
        limiter.check(1);
        limiter.check(1);
        assert!(!limiter.check(1), "user 1 should be blocked");

        // User 2 is unaffected
        assert!(limiter.check(2), "user 2 should still be allowed");
    }

    #[test]
    fn test_zero_limit_blocks_immediately() {
        let mut limiter = RateLimiter::new(0);
        assert!(!limiter.check(1), "zero limit should block on first request");
    }

    #[test]
    fn test_single_request_within_single_limit() {
        let mut limiter = RateLimiter::new(1);
        assert!(limiter.check(1));
        assert!(!limiter.check(1));
    }

    /// Manually inject old timestamps to simulate the 60-second window expiry.
    #[test]
    fn test_window_expiry_allows_requests_again() {
        let mut limiter = RateLimiter::new(2);
        let old = Instant::now() - std::time::Duration::from_secs(61);

        // Insert two old timestamps directly (simulating past requests)
        limiter.limits.insert(42, vec![old, old]);

        // Both are outside the 60-s window, so the slot should be free
        assert!(limiter.check(42), "old timestamps should be evicted, allowing new request");
    }
}
