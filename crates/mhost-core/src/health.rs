use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// HealthCheckKind
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum HealthCheckKind {
    Http {
        url: String,
        #[serde(default = "default_http_status")]
        expected_status: u16,
    },
    Tcp {
        host: String,
        port: u16,
    },
    Script {
        command: String,
    },
}

fn default_http_status() -> u16 {
    200
}

// ---------------------------------------------------------------------------
// HealthConfig
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HealthConfig {
    pub kind: HealthCheckKind,
    #[serde(default = "default_interval_ms")]
    pub interval_ms: u64,
    #[serde(default = "default_timeout_ms")]
    pub timeout_ms: u64,
    #[serde(default = "default_retries")]
    pub retries: u32,
}

fn default_interval_ms() -> u64 {
    10_000
}

fn default_timeout_ms() -> u64 {
    3_000
}

fn default_retries() -> u32 {
    3
}

// ---------------------------------------------------------------------------
// HealthStatus
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum HealthStatus {
    #[default]
    Unknown,
    Healthy,
    Unhealthy,
    Disabled,
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_health_status_is_unknown() {
        let status = HealthStatus::default();
        assert_eq!(status, HealthStatus::Unknown);
    }

    #[test]
    fn test_health_config_defaults() {
        let config: HealthConfig = serde_json::from_str(
            r#"{"kind": {"kind": "http", "url": "http://localhost:8080/health"}}"#,
        )
        .expect("deserialize with defaults");

        assert_eq!(config.interval_ms, 10_000);
        assert_eq!(config.timeout_ms, 3_000);
        assert_eq!(config.retries, 3);

        match &config.kind {
            HealthCheckKind::Http {
                url,
                expected_status,
            } => {
                assert_eq!(url, "http://localhost:8080/health");
                assert_eq!(*expected_status, 200);
            }
            other => panic!("unexpected kind: {:?}", other),
        }
    }

    #[test]
    fn test_health_config_serialization_roundtrip() {
        let original = HealthConfig {
            kind: HealthCheckKind::Http {
                url: "http://example.com/health".to_string(),
                expected_status: 200,
            },
            interval_ms: 5_000,
            timeout_ms: 1_000,
            retries: 2,
        };

        let json = serde_json::to_string(&original).expect("serialize");
        let decoded: HealthConfig = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(original, decoded);
    }

    #[test]
    fn test_tcp_health_check_roundtrip() {
        let original = HealthConfig {
            kind: HealthCheckKind::Tcp {
                host: "localhost".to_string(),
                port: 5432,
            },
            interval_ms: 15_000,
            timeout_ms: 2_000,
            retries: 5,
        };

        let json = serde_json::to_string(&original).expect("serialize");
        let decoded: HealthConfig = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(original, decoded);
    }

    #[test]
    fn test_script_health_check_roundtrip() {
        let original = HealthConfig {
            kind: HealthCheckKind::Script {
                command: "/usr/local/bin/check-db.sh".to_string(),
            },
            interval_ms: 30_000,
            timeout_ms: 10_000,
            retries: 1,
        };

        let json = serde_json::to_string(&original).expect("serialize");
        let decoded: HealthConfig = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(original, decoded);
    }

    // -- HealthConfig with all probe types via serde -------------------------

    #[test]
    fn test_health_config_all_probe_types_deserialize() {
        // HTTP probe
        let http_json = r#"{"kind":{"kind":"http","url":"http://localhost/health"}}"#;
        let http: HealthConfig = serde_json::from_str(http_json).expect("http");
        assert!(matches!(http.kind, HealthCheckKind::Http { .. }));

        // TCP probe
        let tcp_json = r#"{"kind":{"kind":"tcp","host":"127.0.0.1","port":5432}}"#;
        let tcp: HealthConfig = serde_json::from_str(tcp_json).expect("tcp");
        assert!(matches!(tcp.kind, HealthCheckKind::Tcp { .. }));

        // Script probe
        let script_json = r#"{"kind":{"kind":"script","command":"/bin/check.sh"}}"#;
        let script: HealthConfig = serde_json::from_str(script_json).expect("script");
        assert!(matches!(script.kind, HealthCheckKind::Script { .. }));
    }

    // -- HealthStatus display ------------------------------------------------

    #[test]
    fn test_health_status_all_variants_serialize() {
        // HealthStatus uses serde rename_all = "snake_case"
        let unknown = serde_json::to_string(&HealthStatus::Unknown).expect("serialize");
        let healthy = serde_json::to_string(&HealthStatus::Healthy).expect("serialize");
        let unhealthy = serde_json::to_string(&HealthStatus::Unhealthy).expect("serialize");
        let disabled = serde_json::to_string(&HealthStatus::Disabled).expect("serialize");

        assert_eq!(unknown, r#""unknown""#);
        assert_eq!(healthy, r#""healthy""#);
        assert_eq!(unhealthy, r#""unhealthy""#);
        assert_eq!(disabled, r#""disabled""#);
    }

    #[test]
    fn test_health_status_equality() {
        assert_eq!(HealthStatus::Healthy, HealthStatus::Healthy);
        assert_ne!(HealthStatus::Healthy, HealthStatus::Unhealthy);
        assert_ne!(HealthStatus::Unknown, HealthStatus::Disabled);
    }

    // -- HealthConfig clone + equality --------------------------------------

    #[test]
    fn test_health_config_clone_equality() {
        let original = HealthConfig {
            kind: HealthCheckKind::Tcp {
                host: "db.internal".to_string(),
                port: 3306,
            },
            interval_ms: 20_000,
            timeout_ms: 4_000,
            retries: 2,
        };
        let cloned = original.clone();
        assert_eq!(original, cloned);
    }
}
