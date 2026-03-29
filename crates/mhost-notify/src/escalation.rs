use std::time::Duration;

/// An escalation chain defines an ordered list of notification channels
/// and the duration to wait before escalating to the next channel.
#[derive(Debug, Clone)]
pub struct EscalationChain {
    /// Channel names in escalation order (first = primary, last = final escalation).
    pub chain: Vec<String>,
    /// Duration to wait before escalating to the next channel.
    pub escalate_after: Duration,
}

impl EscalationChain {
    pub fn new(chain: Vec<String>, escalate_after: Duration) -> Self {
        Self { chain, escalate_after }
    }

    /// Returns the next channel to escalate to after `current_channel`,
    /// or `None` if `current_channel` is the last in the chain.
    pub fn next_channel(&self, current_channel: &str) -> Option<&str> {
        let pos = self.chain.iter().position(|c| c == current_channel)?;
        self.chain.get(pos + 1).map(String::as_str)
    }

    /// Returns the first channel in the chain, or `None` if empty.
    pub fn first_channel(&self) -> Option<&str> {
        self.chain.first().map(String::as_str)
    }

    /// Returns `true` if `channel` is in this escalation chain.
    pub fn contains(&self, channel: &str) -> bool {
        self.chain.iter().any(|c| c == channel)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_chain() -> EscalationChain {
        EscalationChain::new(
            vec![
                "slack".to_string(),
                "pagerduty".to_string(),
                "email".to_string(),
            ],
            Duration::from_secs(300),
        )
    }

    #[test]
    fn test_first_channel_returns_first() {
        let chain = make_chain();
        assert_eq!(chain.first_channel(), Some("slack"));
    }

    #[test]
    fn test_next_channel_from_first() {
        let chain = make_chain();
        assert_eq!(chain.next_channel("slack"), Some("pagerduty"));
    }

    #[test]
    fn test_next_channel_from_middle() {
        let chain = make_chain();
        assert_eq!(chain.next_channel("pagerduty"), Some("email"));
    }

    #[test]
    fn test_next_channel_from_last_returns_none() {
        let chain = make_chain();
        assert_eq!(chain.next_channel("email"), None);
    }

    #[test]
    fn test_next_channel_unknown_returns_none() {
        let chain = make_chain();
        assert_eq!(chain.next_channel("discord"), None);
    }

    #[test]
    fn test_contains_known_channel() {
        let chain = make_chain();
        assert!(chain.contains("pagerduty"));
    }

    #[test]
    fn test_contains_unknown_channel() {
        let chain = make_chain();
        assert!(!chain.contains("telegram"));
    }

    #[test]
    fn test_empty_chain_first_channel_is_none() {
        let chain = EscalationChain::new(vec![], Duration::from_secs(60));
        assert_eq!(chain.first_channel(), None);
    }

    // -- Empty chain: next_channel on anything returns None ----------------

    #[test]
    fn test_empty_chain_next_channel_returns_none() {
        let chain = EscalationChain::new(vec![], Duration::from_secs(60));
        assert_eq!(chain.next_channel("slack"), None);
    }

    // -- Empty chain: contains always returns false ------------------------

    #[test]
    fn test_empty_chain_contains_returns_false() {
        let chain = EscalationChain::new(vec![], Duration::from_secs(60));
        assert!(!chain.contains("slack"));
        assert!(!chain.contains(""));
    }

    // -- Single-item chain: next is None, first is the sole item -----------

    #[test]
    fn test_single_item_chain() {
        let chain = EscalationChain::new(vec!["pagerduty".to_string()], Duration::from_secs(600));
        assert_eq!(chain.first_channel(), Some("pagerduty"));
        assert_eq!(chain.next_channel("pagerduty"), None);
        assert!(chain.contains("pagerduty"));
        assert!(!chain.contains("slack"));
    }

    // -- Clone preserves escalate_after ------------------------------------

    #[test]
    fn test_chain_clone_preserves_data() {
        let original = make_chain();
        let cloned = original.clone();
        assert_eq!(original.escalate_after, cloned.escalate_after);
        assert_eq!(original.chain, cloned.chain);
    }
}
