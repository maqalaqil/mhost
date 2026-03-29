use crate::ecosystem::EcosystemConfig;

// ---------------------------------------------------------------------------
// ValidationError
// ---------------------------------------------------------------------------

/// A single validation failure with a field name and human-readable message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidationError {
    pub field: String,
    pub message: String,
}

impl ValidationError {
    /// Construct a new `ValidationError`.
    pub fn new(field: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            field: field.into(),
            message: message.into(),
        }
    }
}

// ---------------------------------------------------------------------------
// validate_config
// ---------------------------------------------------------------------------

/// Validate an [`EcosystemConfig`] and return a list of [`ValidationError`]s.
///
/// Rules currently checked:
/// - Each process must have a non-empty `command`.
/// - Each process must have `instances >= 1`.
pub fn validate_config(config: &EcosystemConfig) -> Vec<ValidationError> {
    let mut errors = Vec::new();

    for (name, raw) in &config.process {
        if raw.command.trim().is_empty() {
            errors.push(ValidationError::new(
                format!("process.{name}.command"),
                "command must not be empty".to_string(),
            ));
        }

        if raw.instances == 0 {
            errors.push(ValidationError::new(
                format!("process.{name}.instances"),
                "instances must be >= 1".to_string(),
            ));
        }
    }

    errors
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // 1. Valid config produces no errors.
    #[test]
    fn test_valid_config() {
        let toml = r#"
[process.api]
command = "node"
instances = 2
"#;
        let cfg = EcosystemConfig::from_str(toml, "toml").expect("parse");
        let errors = validate_config(&cfg);
        assert!(errors.is_empty(), "expected no errors, got: {errors:?}");
    }

    // 2. Empty command triggers a validation error.
    #[test]
    fn test_empty_command() {
        let toml = r#"
[process.broken]
command = ""
"#;
        let cfg = EcosystemConfig::from_str(toml, "toml").expect("parse");
        let errors = validate_config(&cfg);
        assert_eq!(errors.len(), 1);
        assert!(errors[0].field.contains("command"));
        assert!(errors[0].message.contains("empty"));
    }

    // 3. Zero instances triggers a validation error.
    #[test]
    fn test_zero_instances_error() {
        let json = r#"
{
  "process": {
    "svc": {
      "command": "node",
      "instances": 0
    }
  }
}
"#;
        let cfg = EcosystemConfig::from_str(json, "json").expect("parse");
        let errors = validate_config(&cfg);
        assert_eq!(errors.len(), 1);
        assert!(errors[0].field.contains("instances"));
        assert!(errors[0].message.contains("1"));
    }

    // 4. Multiple errors — both empty command and zero instances.
    #[test]
    fn test_multiple_errors_collected() {
        let toml = r#"
[process.bad1]
command = ""
instances = 1

[process.bad2]
command = "ok"
instances = 0

[process.both_bad]
command = "   "
instances = 0
"#;
        let cfg = EcosystemConfig::from_str(toml, "toml").expect("parse");
        let errors = validate_config(&cfg);
        // bad1 → 1 error (empty command)
        // bad2 → 1 error (zero instances)
        // both_bad → 2 errors (whitespace command + zero instances)
        assert!(
            errors.len() >= 4,
            "expected at least 4 errors, got {}: {:?}",
            errors.len(),
            errors
        );
    }

    // 5. ValidationError constructor and fields.
    #[test]
    fn test_validation_error_new() {
        let err = ValidationError::new("process.api.command", "command must not be empty");
        assert_eq!(err.field, "process.api.command");
        assert_eq!(err.message, "command must not be empty");
    }

    // 6. ValidationError equality.
    #[test]
    fn test_validation_error_equality() {
        let a = ValidationError::new("field", "message");
        let b = ValidationError::new("field", "message");
        let c = ValidationError::new("other", "message");
        assert_eq!(a, b);
        assert_ne!(a, c);
    }
}
