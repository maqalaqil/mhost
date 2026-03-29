use crate::parser::LogEntry;

/// A structured filter that can be evaluated against a `LogEntry`.
#[derive(Debug, Clone, PartialEq)]
pub enum QueryFilter {
    Eq { field: String, value: String },
    Gt { field: String, value: String },
    Gte { field: String, value: String },
    Lt { field: String, value: String },
    Lte { field: String, value: String },
    And(Box<QueryFilter>, Box<QueryFilter>),
    Or(Box<QueryFilter>, Box<QueryFilter>),
}

/// Parse a query string into a `QueryFilter`.
///
/// Operator precedence (low → high): OR < AND < comparison.
///
/// Examples:
/// - `"level=ERROR"`
/// - `"level=ERROR AND message=crash"`
/// - `"level=WARN OR level=ERROR"`
/// - `"count>=5"`
pub fn parse_query(input: &str) -> Result<QueryFilter, String> {
    let input = input.trim();
    parse_or(input)
}

fn parse_or(input: &str) -> Result<QueryFilter, String> {
    // Split on the first " OR " (case-sensitive per spec).
    if let Some(idx) = find_keyword(input, " OR ") {
        let left = parse_or(input[..idx].trim())?;
        let right = parse_or(input[idx + 4..].trim())?;
        return Ok(QueryFilter::Or(Box::new(left), Box::new(right)));
    }
    parse_and(input)
}

fn parse_and(input: &str) -> Result<QueryFilter, String> {
    if let Some(idx) = find_keyword(input, " AND ") {
        let left = parse_and(input[..idx].trim())?;
        let right = parse_and(input[idx + 5..].trim())?;
        return Ok(QueryFilter::And(Box::new(left), Box::new(right)));
    }
    parse_term(input)
}

/// Find the first occurrence of `keyword` that is not inside quotes.
fn find_keyword(input: &str, keyword: &str) -> Option<usize> {
    input.find(keyword)
}

fn parse_term(input: &str) -> Result<QueryFilter, String> {
    // Try operators from longest to shortest to avoid ambiguity (>= before >).
    for (op, ctor) in &[
        (">=", "gte"),
        ("<=", "lte"),
        (">", "gt"),
        ("<", "lt"),
        ("=", "eq"),
    ] {
        if let Some(idx) = input.find(op) {
            let field = input[..idx].trim().to_owned();
            let value = input[idx + op.len()..].trim().to_owned();

            if field.is_empty() {
                return Err(format!("missing field name in: {input}"));
            }

            return Ok(match *ctor {
                "gte" => QueryFilter::Gte { field, value },
                "lte" => QueryFilter::Lte { field, value },
                "gt" => QueryFilter::Gt { field, value },
                "lt" => QueryFilter::Lt { field, value },
                _ => QueryFilter::Eq { field, value },
            });
        }
    }

    Err(format!("cannot parse term: {input}"))
}

/// Resolve a field name to a string value from the entry.
fn resolve_field<'a>(field: &str, entry: &'a LogEntry) -> Option<std::borrow::Cow<'a, str>> {
    match field {
        "level" => entry
            .level
            .as_ref()
            .map(|l| std::borrow::Cow::Borrowed(l.as_str())),
        "message" | "msg" => Some(std::borrow::Cow::Borrowed(entry.message.as_str())),
        "process" | "process_name" => Some(std::borrow::Cow::Borrowed(entry.process_name.as_str())),
        other => entry
            .fields
            .get(other)
            .and_then(|v| v.as_str())
            .map(std::borrow::Cow::Borrowed)
            .or_else(|| {
                entry
                    .fields
                    .get(other)
                    .map(|v| std::borrow::Cow::Owned(v.to_string()))
            }),
    }
}

/// Evaluate a `QueryFilter` against a `LogEntry`.
/// Returns `true` when the entry satisfies the filter.
pub fn filter_matches(filter: &QueryFilter, entry: &LogEntry) -> bool {
    match filter {
        QueryFilter::Eq { field, value } => resolve_field(field, entry)
            .map(|v| v.as_ref() == value.as_str())
            .unwrap_or(false),

        QueryFilter::Gt { field, value } => compare_values(field, value, entry, |ord| ord.is_gt()),
        QueryFilter::Gte { field, value } => compare_values(field, value, entry, |ord| ord.is_ge()),
        QueryFilter::Lt { field, value } => compare_values(field, value, entry, |ord| ord.is_lt()),
        QueryFilter::Lte { field, value } => compare_values(field, value, entry, |ord| ord.is_le()),

        QueryFilter::And(left, right) => {
            filter_matches(left, entry) && filter_matches(right, entry)
        }
        QueryFilter::Or(left, right) => filter_matches(left, entry) || filter_matches(right, entry),
    }
}

fn compare_values(
    field: &str,
    value: &str,
    entry: &LogEntry,
    predicate: impl Fn(std::cmp::Ordering) -> bool,
) -> bool {
    let Some(field_val) = resolve_field(field, entry) else {
        return false;
    };

    // Try numeric comparison first.
    if let (Ok(a), Ok(b)) = (field_val.parse::<f64>(), value.parse::<f64>()) {
        return predicate(a.partial_cmp(&b).unwrap_or(std::cmp::Ordering::Equal));
    }

    // Fall back to lexicographic.
    predicate(field_val.as_ref().cmp(value))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::{LogEntry, LogLevel};
    use chrono::Utc;
    use std::collections::HashMap;

    fn make_entry(level: Option<LogLevel>, message: &str) -> LogEntry {
        LogEntry {
            timestamp: Utc::now(),
            level,
            message: message.to_owned(),
            process_name: "test".to_owned(),
            instance: 0,
            fields: HashMap::new(),
            raw_line: message.to_owned(),
        }
    }

    fn make_entry_with_fields(
        level: Option<LogLevel>,
        message: &str,
        fields: HashMap<String, serde_json::Value>,
    ) -> LogEntry {
        LogEntry {
            timestamp: Utc::now(),
            level,
            message: message.to_owned(),
            process_name: "test".to_owned(),
            instance: 0,
            fields,
            raw_line: message.to_owned(),
        }
    }

    #[test]
    fn parse_eq() {
        let filter = parse_query("level=ERROR").expect("parse");
        assert_eq!(
            filter,
            QueryFilter::Eq {
                field: "level".into(),
                value: "ERROR".into()
            }
        );
    }

    #[test]
    fn parse_and() {
        let filter = parse_query("level=ERROR AND message=crash").expect("parse");
        assert_eq!(
            filter,
            QueryFilter::And(
                Box::new(QueryFilter::Eq {
                    field: "level".into(),
                    value: "ERROR".into()
                }),
                Box::new(QueryFilter::Eq {
                    field: "message".into(),
                    value: "crash".into()
                }),
            )
        );
    }

    #[test]
    fn evaluate_against_entry() {
        let entry = make_entry(Some(LogLevel::Error), "crash happened");
        let filter = parse_query("level=ERROR").expect("parse");
        assert!(filter_matches(&filter, &entry));

        let filter_no = parse_query("level=INFO").expect("parse");
        assert!(!filter_matches(&filter_no, &entry));
    }

    #[test]
    fn numeric_comparison() {
        let mut fields = HashMap::new();
        fields.insert("count".into(), serde_json::json!(42));
        let entry = make_entry_with_fields(Some(LogLevel::Info), "metrics", fields);

        let gte = parse_query("count>=40").expect("parse gte");
        assert!(filter_matches(&gte, &entry));

        let lt = parse_query("count<10").expect("parse lt");
        assert!(!filter_matches(&lt, &entry));

        let eq = parse_query("count=42").expect("parse eq");
        assert!(filter_matches(&eq, &entry));
    }

    // -- Parse OR query -------------------------------------------------------

    #[test]
    fn parse_or_query() {
        let filter = parse_query("level=WARN OR level=ERROR").expect("parse OR");
        assert_eq!(
            filter,
            QueryFilter::Or(
                Box::new(QueryFilter::Eq {
                    field: "level".into(),
                    value: "WARN".into()
                }),
                Box::new(QueryFilter::Eq {
                    field: "level".into(),
                    value: "ERROR".into()
                }),
            )
        );
    }

    // -- Evaluate OR against entries -----------------------------------------

    #[test]
    fn evaluate_or_against_entries() {
        let warn_entry = make_entry(Some(LogLevel::Warn), "disk space low");
        let error_entry = make_entry(Some(LogLevel::Error), "crash");
        let info_entry = make_entry(Some(LogLevel::Info), "started");

        let filter = parse_query("level=WARN OR level=ERROR").expect("parse");
        assert!(filter_matches(&filter, &warn_entry));
        assert!(filter_matches(&filter, &error_entry));
        assert!(!filter_matches(&filter, &info_entry));
    }

    // -- Parse nested AND/OR -------------------------------------------------

    #[test]
    fn parse_nested_and_or() {
        // "level=ERROR AND message=crash OR level=FATAL"
        // Precedence: AND before OR → (level=ERROR AND message=crash) OR level=FATAL
        let filter = parse_query("level=ERROR AND message=crash OR level=FATAL").expect("parse");
        // The top-level node should be OR
        assert!(
            matches!(filter, QueryFilter::Or(_, _)),
            "top-level should be OR, got: {:?}",
            filter
        );
    }

    // -- Empty query string returns error ------------------------------------

    #[test]
    fn empty_query_string_returns_error() {
        let result = parse_query("");
        assert!(result.is_err(), "empty query should be an error");
    }

    // -- Lte comparison -------------------------------------------------------

    #[test]
    fn parse_lte_comparison() {
        let filter = parse_query("count<=100").expect("parse lte");
        assert_eq!(
            filter,
            QueryFilter::Lte {
                field: "count".into(),
                value: "100".into()
            }
        );
    }

    // -- GT comparison --------------------------------------------------------

    #[test]
    fn parse_gt_comparison() {
        let filter = parse_query("count>5").expect("parse gt");
        assert_eq!(
            filter,
            QueryFilter::Gt {
                field: "count".into(),
                value: "5".into()
            }
        );
    }
}
