use chrono::{DateTime, Utc};
use std::collections::HashMap;

/// The type of notification event that occurred.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum EventType {
    Crash,
    Restart,
    Oom,
    Deploy,
    HealthFail,
    Recovered,
}

impl std::fmt::Display for EventType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EventType::Crash => write!(f, "Crash"),
            EventType::Restart => write!(f, "Restart"),
            EventType::Oom => write!(f, "OOM"),
            EventType::Deploy => write!(f, "Deploy"),
            EventType::HealthFail => write!(f, "HealthFail"),
            EventType::Recovered => write!(f, "Recovered"),
        }
    }
}

/// Severity level of a notification event.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Severity {
    Info,
    Warning,
    Critical,
}

impl std::fmt::Display for Severity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Severity::Info => write!(f, "INFO"),
            Severity::Warning => write!(f, "WARNING"),
            Severity::Critical => write!(f, "CRITICAL"),
        }
    }
}

/// A notification event emitted by the mhost process manager.
#[derive(Debug, Clone)]
pub struct NotifyEvent {
    pub event_type: EventType,
    pub process_name: String,
    pub message: String,
    pub severity: Severity,
    pub timestamp: DateTime<Utc>,
    pub metadata: HashMap<String, String>,
}

impl NotifyEvent {
    /// Create a new notify event with current timestamp.
    pub fn new(
        event_type: EventType,
        process_name: impl Into<String>,
        message: impl Into<String>,
        severity: Severity,
    ) -> Self {
        Self {
            event_type,
            process_name: process_name.into(),
            message: message.into(),
            severity,
            timestamp: Utc::now(),
            metadata: HashMap::new(),
        }
    }

    /// Add metadata to the event (builder pattern, returns new instance).
    pub fn with_metadata(self, key: impl Into<String>, value: impl Into<String>) -> Self {
        let mut metadata = self.metadata.clone();
        metadata.insert(key.into(), value.into());
        Self { metadata, ..self }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_event_type_display() {
        assert_eq!(EventType::Crash.to_string(), "Crash");
        assert_eq!(EventType::Oom.to_string(), "OOM");
        assert_eq!(EventType::Recovered.to_string(), "Recovered");
    }

    #[test]
    fn test_severity_display() {
        assert_eq!(Severity::Info.to_string(), "INFO");
        assert_eq!(Severity::Warning.to_string(), "WARNING");
        assert_eq!(Severity::Critical.to_string(), "CRITICAL");
    }

    #[test]
    fn test_notify_event_new() {
        let event = NotifyEvent::new(
            EventType::Crash,
            "my-service",
            "Process crashed with exit code 1",
            Severity::Critical,
        );
        assert_eq!(event.process_name, "my-service");
        assert_eq!(event.severity, Severity::Critical);
        assert!(event.metadata.is_empty());
    }

    #[test]
    fn test_notify_event_with_metadata_is_immutable() {
        let event = NotifyEvent::new(EventType::Deploy, "api", "Deployed v2", Severity::Info);
        let enriched = event.with_metadata("version", "2.0.0");
        // original event is consumed but new one has metadata
        assert_eq!(enriched.metadata.get("version"), Some(&"2.0.0".to_string()));
    }

    // -- All EventType variants construct correctly -------------------------

    #[test]
    fn test_all_event_type_variants() {
        let types = [
            EventType::Crash,
            EventType::Restart,
            EventType::Oom,
            EventType::Deploy,
            EventType::HealthFail,
            EventType::Recovered,
        ];

        for event_type in types {
            let event = NotifyEvent::new(
                event_type.clone(),
                "svc",
                "test message",
                Severity::Info,
            );
            assert_eq!(event.event_type, event_type);
            assert_eq!(event.process_name, "svc");
        }
    }

    // -- All EventType display strings -------------------------------------

    #[test]
    fn test_all_event_type_display_strings() {
        assert_eq!(EventType::Crash.to_string(), "Crash");
        assert_eq!(EventType::Restart.to_string(), "Restart");
        assert_eq!(EventType::Oom.to_string(), "OOM");
        assert_eq!(EventType::Deploy.to_string(), "Deploy");
        assert_eq!(EventType::HealthFail.to_string(), "HealthFail");
        assert_eq!(EventType::Recovered.to_string(), "Recovered");
    }

    // -- Severity ordering (via Display) ------------------------------------

    #[test]
    fn test_severity_ordering_by_display() {
        // We can't compare Severity with <, but we can verify they have distinct
        // display strings and are ordered logically by their known labels.
        let info = Severity::Info.to_string();
        let warning = Severity::Warning.to_string();
        let critical = Severity::Critical.to_string();

        assert_ne!(info, warning);
        assert_ne!(warning, critical);
        assert_ne!(info, critical);

        // Verify expected ordering semantics (Critical > Warning > Info
        // by lexicographic sort of their display strings is not guaranteed,
        // but we can verify explicit values).
        assert_eq!(info, "INFO");
        assert_eq!(warning, "WARNING");
        assert_eq!(critical, "CRITICAL");
    }

    // -- Severity equality -------------------------------------------------

    #[test]
    fn test_severity_equality() {
        assert_eq!(Severity::Info, Severity::Info);
        assert_eq!(Severity::Critical, Severity::Critical);
        assert_ne!(Severity::Info, Severity::Warning);
        assert_ne!(Severity::Warning, Severity::Critical);
    }

    // -- NotifyEvent clone -------------------------------------------------

    #[test]
    fn test_notify_event_clone() {
        let event = NotifyEvent::new(EventType::Crash, "svc", "crash", Severity::Critical);
        let cloned = event.clone();
        assert_eq!(event.process_name, cloned.process_name);
        assert_eq!(event.event_type, cloned.event_type);
        assert_eq!(event.severity, cloned.severity);
    }

    // -- with_metadata chaining -------------------------------------------

    #[test]
    fn test_with_metadata_chaining() {
        let event = NotifyEvent::new(EventType::Deploy, "api", "Deployed", Severity::Info)
            .with_metadata("version", "3.0.0")
            .with_metadata("environment", "production");

        assert_eq!(event.metadata.get("version"), Some(&"3.0.0".to_string()));
        assert_eq!(event.metadata.get("environment"), Some(&"production".to_string()));
    }
}
