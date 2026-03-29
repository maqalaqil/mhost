use std::collections::HashMap;
use std::time::{Duration, Instant};

use chrono::{DateTime, Utc};
use tracing::{debug, info};

use crate::alert::{evaluate, AlertRule};

// ---------------------------------------------------------------------------
// RemediationAction
// ---------------------------------------------------------------------------

/// The action to take when an alert rule fires.
#[derive(Debug, Clone, PartialEq)]
pub enum RemediationAction {
    /// Restart the offending process.
    Restart,
    /// Send notifications to the listed channels.
    Notify(Vec<String>),
}

// ---------------------------------------------------------------------------
// RemediationEngine
// ---------------------------------------------------------------------------

/// Evaluates alert rules and enforces per-rule cooldowns to prevent
/// remediation storms.
pub struct RemediationEngine {
    /// Maps rule name → last time the rule fired.
    cooldowns: HashMap<String, Instant>,
}

impl RemediationEngine {
    /// Create a new, empty engine.
    pub fn new() -> Self {
        Self {
            cooldowns: HashMap::new(),
        }
    }

    /// Evaluate `rule` against `current_value` and `history`.
    ///
    /// Returns `Some(action)` when:
    /// 1. The alert condition fires (based on history), and
    /// 2. The rule's cooldown has expired (or has never fired).
    ///
    /// Returns `None` when the condition is not met or the cooldown is active.
    pub fn check_rule(
        &mut self,
        rule: &AlertRule,
        _current_value: f64,
        history: &[(DateTime<Utc>, f64)],
    ) -> Option<RemediationAction> {
        let fires = evaluate(&rule.condition, history);
        if !fires {
            debug!(rule = %rule.name, "condition not met");
            return None;
        }

        // Check cooldown.
        let cooldown = Duration::from_millis(rule.cooldown_ms);
        if let Some(&last_fired) = self.cooldowns.get(&rule.name) {
            if last_fired.elapsed() < cooldown {
                debug!(
                    rule = %rule.name,
                    remaining_ms = (cooldown - last_fired.elapsed()).as_millis(),
                    "cooldown active — suppressing action"
                );
                return None;
            }
        }

        // Record firing time (immutable-style: replace entry).
        self.cooldowns.insert(rule.name.clone(), Instant::now());
        info!(rule = %rule.name, "alert rule fired");

        // Determine action: prefer explicit action over notify-only.
        let action = match rule.action.as_deref() {
            Some("restart") => RemediationAction::Restart,
            _ => RemediationAction::Notify(rule.notify.clone()),
        };

        Some(action)
    }
}

impl Default for RemediationEngine {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::alert::{AlertCondition, AlertRule, Operator};
    use chrono::Utc;

    fn make_rule(cooldown_ms: u64, action: Option<&str>) -> AlertRule {
        AlertRule {
            name: "high-memory".to_string(),
            process_glob: "api*".to_string(),
            condition: AlertCondition {
                metric: "memory".to_string(),
                operator: Operator::Gt,
                threshold: 100.0,
                duration_ms: 0,
            },
            notify: vec!["slack".to_string()],
            action: action.map(String::from),
            cooldown_ms,
        }
    }

    fn above_history() -> Vec<(DateTime<Utc>, f64)> {
        vec![(Utc::now(), 200.0)] // above threshold
    }

    fn below_history() -> Vec<(DateTime<Utc>, f64)> {
        vec![(Utc::now(), 50.0)] // below threshold
    }

    // ---- basic firing -------------------------------------------------------

    #[test]
    fn fires_when_condition_met_and_no_cooldown() {
        let mut engine = RemediationEngine::new();
        let rule = make_rule(0, None);
        let action = engine.check_rule(&rule, 200.0, &above_history());
        assert_eq!(
            action,
            Some(RemediationAction::Notify(vec!["slack".to_string()]))
        );
    }

    #[test]
    fn does_not_fire_when_condition_not_met() {
        let mut engine = RemediationEngine::new();
        let rule = make_rule(0, None);
        let action = engine.check_rule(&rule, 50.0, &below_history());
        assert!(action.is_none());
    }

    #[test]
    fn restart_action_returned_when_configured() {
        let mut engine = RemediationEngine::new();
        let rule = make_rule(0, Some("restart"));
        let action = engine.check_rule(&rule, 200.0, &above_history());
        assert_eq!(action, Some(RemediationAction::Restart));
    }

    // ---- cooldown -----------------------------------------------------------

    #[test]
    fn cooldown_prevents_second_immediate_trigger() {
        let mut engine = RemediationEngine::new();
        // Very long cooldown (5 minutes) — second call should be suppressed.
        let rule = make_rule(300_000, None);

        let first = engine.check_rule(&rule, 200.0, &above_history());
        assert!(first.is_some(), "first call should fire");

        let second = engine.check_rule(&rule, 200.0, &above_history());
        assert!(
            second.is_none(),
            "second call within cooldown should be suppressed"
        );
    }

    #[test]
    fn zero_cooldown_allows_repeated_triggers() {
        let mut engine = RemediationEngine::new();
        let rule = make_rule(0, None);

        let first = engine.check_rule(&rule, 200.0, &above_history());
        let second = engine.check_rule(&rule, 200.0, &above_history());
        assert!(first.is_some());
        assert!(
            second.is_some(),
            "zero cooldown should allow immediate re-trigger"
        );
    }

    #[test]
    fn different_rules_have_independent_cooldowns() {
        let mut engine = RemediationEngine::new();

        let mut rule_a = make_rule(300_000, None);
        rule_a.name = "rule-a".to_string();

        let mut rule_b = make_rule(300_000, None);
        rule_b.name = "rule-b".to_string();

        engine.check_rule(&rule_a, 200.0, &above_history());

        // rule-b has never fired, so it should go through.
        let result = engine.check_rule(&rule_b, 200.0, &above_history());
        assert!(
            result.is_some(),
            "rule-b should fire independently of rule-a"
        );
    }

    #[test]
    fn no_history_does_not_fire() {
        let mut engine = RemediationEngine::new();
        let rule = make_rule(0, None);
        let action = engine.check_rule(&rule, 200.0, &[]);
        assert!(action.is_none());
    }
}
