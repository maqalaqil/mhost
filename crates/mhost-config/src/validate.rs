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
                format!("process.{}.command", name),
                "command must not be empty".to_string(),
            ));
        }

        if raw.instances == 0 {
            errors.push(ValidationError::new(
                format!("process.{}.instances", name),
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
        assert!(errors.is_empty(), "expected no errors, got: {:?}", errors);
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
}
