use std::collections::HashMap;
use std::time::{Duration, Instant};

/// Throttle prevents flooding notification channels by enforcing minimum
/// time windows between sends per channel.
pub struct Throttle {
    /// Maps channel name to the last time a notification was sent.
    windows: HashMap<String, Instant>,
    /// Default minimum window between sends for a channel.
    pub default_window: Duration,
}

impl Throttle {
    pub fn new(default_window: Duration) -> Self {
        Self {
            windows: HashMap::new(),
            default_window,
        }
    }

    /// Returns `true` if a notification should be sent for the channel.
    /// Returns `false` if the channel is within its throttle window.
    /// When returning `true`, records the current time as the last send.
    pub fn should_send(&mut self, channel: &str, window: Duration) -> bool {
        let now = Instant::now();

        let should = match self.windows.get(channel) {
            None => true,
            Some(last_sent) => now.duration_since(*last_sent) >= window,
        };

        if should {
            self.windows.insert(channel.to_string(), now);
        }

        should
    }

    /// Convenience: use the default window for this channel.
    pub fn should_send_default(&mut self, channel: &str) -> bool {
        let window = self.default_window;
        self.should_send(channel, window)
    }

    /// Reset the throttle state for a channel (e.g., after an escalation).
    pub fn reset(&mut self, channel: &str) {
        self.windows.remove(channel);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_first_send_always_allowed() {
        let mut throttle = Throttle::new(Duration::from_secs(60));
        assert!(throttle.should_send("telegram", Duration::from_secs(60)));
    }

    #[test]
    fn test_second_send_within_window_is_suppressed() {
        let mut throttle = Throttle::new(Duration::from_secs(60));
        // First send allowed
        assert!(throttle.should_send("telegram", Duration::from_secs(60)));
        // Immediate second send should be suppressed
        assert!(!throttle.should_send("telegram", Duration::from_secs(60)));
    }

    #[test]
    fn test_send_after_zero_window_is_always_allowed() {
        let mut throttle = Throttle::new(Duration::from_secs(0));
        assert!(throttle.should_send("slack", Duration::from_secs(0)));
        // With zero duration, should immediately allow again
        assert!(throttle.should_send("slack", Duration::from_secs(0)));
    }

    #[test]
    fn test_different_channels_throttled_independently() {
        let mut throttle = Throttle::new(Duration::from_secs(60));
        assert!(throttle.should_send("telegram", Duration::from_secs(60)));
        assert!(throttle.should_send("slack", Duration::from_secs(60)));
        // Both should now be suppressed
        assert!(!throttle.should_send("telegram", Duration::from_secs(60)));
        assert!(!throttle.should_send("slack", Duration::from_secs(60)));
    }

    #[test]
    fn test_reset_allows_immediate_resend() {
        let mut throttle = Throttle::new(Duration::from_secs(60));
        assert!(throttle.should_send("discord", Duration::from_secs(60)));
        assert!(!throttle.should_send("discord", Duration::from_secs(60)));
        throttle.reset("discord");
        assert!(throttle.should_send("discord", Duration::from_secs(60)));
    }

    #[test]
    fn test_send_allowed_after_window_expires() {
        let mut throttle = Throttle::new(Duration::from_millis(10));
        assert!(throttle.should_send("email", Duration::from_millis(10)));
        // Wait for window to expire
        std::thread::sleep(Duration::from_millis(15));
        assert!(throttle.should_send("email", Duration::from_millis(10)));
    }
}
