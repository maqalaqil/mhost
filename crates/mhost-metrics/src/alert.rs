use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Operator
// ---------------------------------------------------------------------------

/// Comparison operator used in an `AlertCondition`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Operator {
    Gt,
    Lt,
    Gte,
    Lte,
}

impl Operator {
    fn apply(&self, lhs: f64, rhs: f64) -> bool {
        match self {
            Operator::Gt => lhs > rhs,
            Operator::Lt => lhs < rhs,
            Operator::Gte => lhs >= rhs,
            Operator::Lte => lhs <= rhs,
        }
    }
}

// ---------------------------------------------------------------------------
// AlertCondition
// ---------------------------------------------------------------------------

/// A condition that must hold continuously for `duration_ms` milliseconds.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlertCondition {
    /// Metric name, e.g. `"memory"` or `"cpu_percent"`.
    pub metric: String,
    pub operator: Operator,
    /// Threshold value in canonical units (bytes for memory, etc.).
    pub threshold: f64,
    /// How long the condition must be continuously true before firing (ms).
    pub duration_ms: u64,
}

// ---------------------------------------------------------------------------
// AlertRule
// ---------------------------------------------------------------------------

/// A full alert rule combining a condition, process selector, notification
/// targets and optional remediation action.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlertRule {
    pub name: String,
    /// Glob pattern matched against process names, e.g. `"api*"`.
    pub process_glob: String,
    pub condition: AlertCondition,
    /// Notification channel IDs (e.g. `"slack"`, `"email"`).
    pub notify: Vec<String>,
    /// Optional remediation action — currently `"restart"` is supported.
    pub action: Option<String>,
    /// Minimum milliseconds between successive firings of this rule.
    pub cooldown_ms: u64,
}

// ---------------------------------------------------------------------------
// parse_condition
// ---------------------------------------------------------------------------

/// Parse a human-readable condition string into an `AlertCondition`.
///
/// Supported syntax:
/// ```text
/// <metric> <op> <threshold>[MB|GB|KB] [for <duration>[s|m|h]]
/// ```
///
/// Examples:
/// * `"memory > 450MB for 5m"`
/// * `"cpu_percent > 90 for 30s"`
/// * `"memory < 100MB"`
pub fn parse_condition(input: &str) -> Result<AlertCondition, String> {
    let tokens: Vec<&str> = input.split_whitespace().collect();
    if tokens.len() < 3 {
        return Err(format!(
            "condition must have at least 3 tokens: '<metric> <op> <threshold>', got: {input:?}"
        ));
    }

    let metric = tokens[0].to_string();

    let operator = match tokens[1] {
        ">" => Operator::Gt,
        "<" => Operator::Lt,
        ">=" => Operator::Gte,
        "<=" => Operator::Lte,
        other => return Err(format!("unknown operator: {other:?}")),
    };

    let threshold = parse_threshold(tokens[2])?;

    // Optional: "for <duration>"
    let duration_ms = if tokens.len() >= 5 && tokens[3].eq_ignore_ascii_case("for") {
        parse_duration_ms(tokens[4])?
    } else {
        0
    };

    Ok(AlertCondition {
        metric,
        operator,
        threshold,
        duration_ms,
    })
}

// ---------------------------------------------------------------------------
// Helpers — threshold parsing
// ---------------------------------------------------------------------------

fn parse_threshold(raw: &str) -> Result<f64, String> {
    if raw.ends_with("GB") || raw.ends_with("gb") {
        let num = raw[..raw.len() - 2]
            .parse::<f64>()
            .map_err(|e| format!("invalid GB value {raw:?}: {e}"))?;
        Ok(num * 1_073_741_824.0)
    } else if raw.ends_with("MB") || raw.ends_with("mb") {
        let num = raw[..raw.len() - 2]
            .parse::<f64>()
            .map_err(|e| format!("invalid MB value {raw:?}: {e}"))?;
        Ok(num * 1_048_576.0)
    } else if raw.ends_with("KB") || raw.ends_with("kb") {
        let num = raw[..raw.len() - 2]
            .parse::<f64>()
            .map_err(|e| format!("invalid KB value {raw:?}: {e}"))?;
        Ok(num * 1_024.0)
    } else {
        raw.parse::<f64>()
            .map_err(|e| format!("invalid threshold {raw:?}: {e}"))
    }
}

// ---------------------------------------------------------------------------
// Helpers — duration parsing
// ---------------------------------------------------------------------------

fn parse_duration_ms(raw: &str) -> Result<u64, String> {
    if raw.ends_with('h') {
        let n = raw[..raw.len() - 1]
            .parse::<u64>()
            .map_err(|e| format!("invalid hours {raw:?}: {e}"))?;
        Ok(n * 3_600_000)
    } else if raw.ends_with('m') {
        let n = raw[..raw.len() - 1]
            .parse::<u64>()
            .map_err(|e| format!("invalid minutes {raw:?}: {e}"))?;
        Ok(n * 60_000)
    } else if raw.ends_with('s') {
        let n = raw[..raw.len() - 1]
            .parse::<u64>()
            .map_err(|e| format!("invalid seconds {raw:?}: {e}"))?;
        Ok(n * 1_000)
    } else {
        raw.parse::<u64>()
            .map_err(|e| format!("invalid duration ms {raw:?}: {e}"))
    }
}

// ---------------------------------------------------------------------------
// evaluate
// ---------------------------------------------------------------------------

/// Returns `true` when `condition` holds for every data point in `history`
/// that falls within the trailing `condition.duration_ms` window.
///
/// Evaluation rules:
/// * If `duration_ms == 0`, only the latest data point is checked.
/// * If there are no data points in the window, returns `false`.
/// * Every data point within the window must satisfy the condition (all-true).
pub fn evaluate(condition: &AlertCondition, history: &[(DateTime<Utc>, f64)]) -> bool {
    if history.is_empty() {
        return false;
    }

    if condition.duration_ms == 0 {
        // Only latest point.
        let (_, value) = history.last().unwrap();
        return condition.operator.apply(*value, condition.threshold);
    }

    let now = Utc::now();
    let window_start = now
        - chrono::Duration::milliseconds(condition.duration_ms as i64);

    let window: Vec<f64> = history
        .iter()
        .filter(|(ts, _)| *ts >= window_start)
        .map(|(_, v)| *v)
        .collect();

    if window.is_empty() {
        return false;
    }

    window
        .iter()
        .all(|&v| condition.operator.apply(v, condition.threshold))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    // ---- parse_condition ---------------------------------------------------

    #[test]
    fn parse_memory_gt_mb_for_minutes() {
        let c = parse_condition("memory > 450MB for 5m").expect("should parse");
        assert_eq!(c.metric, "memory");
        assert_eq!(c.operator, Operator::Gt);
        assert!((c.threshold - 450.0 * 1_048_576.0).abs() < 1.0);
        assert_eq!(c.duration_ms, 5 * 60_000);
    }

    #[test]
    fn parse_cpu_gt_no_duration() {
        let c = parse_condition("cpu_percent > 90").expect("should parse");
        assert_eq!(c.metric, "cpu_percent");
        assert_eq!(c.operator, Operator::Gt);
        assert!((c.threshold - 90.0).abs() < 1e-6);
        assert_eq!(c.duration_ms, 0);
    }

    #[test]
    fn parse_memory_lt_gb_for_seconds() {
        let c = parse_condition("memory < 2GB for 30s").expect("should parse");
        assert!((c.threshold - 2.0 * 1_073_741_824.0).abs() < 1.0);
        assert_eq!(c.duration_ms, 30_000);
    }

    #[test]
    fn parse_gte_operator() {
        let c = parse_condition("cpu_percent >= 50").expect("should parse");
        assert_eq!(c.operator, Operator::Gte);
    }

    #[test]
    fn parse_lte_operator() {
        let c = parse_condition("cpu_percent <= 10").expect("should parse");
        assert_eq!(c.operator, Operator::Lte);
    }

    #[test]
    fn parse_unknown_operator_returns_error() {
        let result = parse_condition("memory != 100MB");
        assert!(result.is_err());
    }

    #[test]
    fn parse_too_few_tokens_returns_error() {
        assert!(parse_condition("memory >").is_err());
    }

    // ---- evaluate ----------------------------------------------------------

    #[test]
    fn evaluate_no_duration_latest_point_above_threshold() {
        let cond = AlertCondition {
            metric: "cpu_percent".to_string(),
            operator: Operator::Gt,
            threshold: 80.0,
            duration_ms: 0,
        };
        let history = vec![(Utc::now(), 95.0)];
        assert!(evaluate(&cond, &history));
    }

    #[test]
    fn evaluate_no_duration_latest_point_below_threshold() {
        let cond = AlertCondition {
            metric: "cpu_percent".to_string(),
            operator: Operator::Gt,
            threshold: 80.0,
            duration_ms: 0,
        };
        let history = vec![(Utc::now(), 50.0)];
        assert!(!evaluate(&cond, &history));
    }

    #[test]
    fn evaluate_with_duration_all_points_in_window_satisfy() {
        let cond = AlertCondition {
            metric: "memory".to_string(),
            operator: Operator::Gt,
            threshold: 100.0,
            duration_ms: 60_000, // 1 minute window
        };
        // All three points within last 60 s and all above 100.
        let now = Utc::now();
        let history = vec![
            (now - chrono::Duration::seconds(50), 200.0),
            (now - chrono::Duration::seconds(30), 250.0),
            (now, 300.0),
        ];
        assert!(evaluate(&cond, &history));
    }

    #[test]
    fn evaluate_with_duration_one_point_fails() {
        let cond = AlertCondition {
            metric: "memory".to_string(),
            operator: Operator::Gt,
            threshold: 100.0,
            duration_ms: 60_000,
        };
        let now = Utc::now();
        let history = vec![
            (now - chrono::Duration::seconds(50), 200.0),
            (now - chrono::Duration::seconds(30), 50.0), // below threshold
            (now, 300.0),
        ];
        assert!(!evaluate(&cond, &history));
    }

    #[test]
    fn evaluate_empty_history_returns_false() {
        let cond = AlertCondition {
            metric: "cpu_percent".to_string(),
            operator: Operator::Gt,
            threshold: 50.0,
            duration_ms: 0,
        };
        assert!(!evaluate(&cond, &[]));
    }

    #[test]
    fn evaluate_no_points_in_window_returns_false() {
        let cond = AlertCondition {
            metric: "memory".to_string(),
            operator: Operator::Gt,
            threshold: 100.0,
            duration_ms: 60_000,
        };
        let old = Utc::now() - chrono::Duration::minutes(5);
        let history = vec![(old, 200.0)]; // outside window
        assert!(!evaluate(&cond, &history));
    }
}
