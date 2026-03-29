use std::collections::HashMap;
use std::path::Path;

use serde::{Deserialize, Serialize};
use thiserror::Error;

use mhost_core::group::GroupConfig;
use mhost_core::health::{HealthCheckKind, HealthConfig};
use mhost_core::process::ProcessConfig;

use crate::env_expand::{expand_env, expand_env_map};

// ---------------------------------------------------------------------------
// Error type
// ---------------------------------------------------------------------------

#[derive(Debug, Error)]
pub enum EcosystemError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("TOML parse error: {0}")]
    Toml(#[from] toml::de::Error),

    #[error("YAML parse error: {0}")]
    Yaml(#[from] serde_yaml::Error),

    #[error("JSON parse error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("Unsupported config format: {0}")]
    UnsupportedFormat(String),
}

// ---------------------------------------------------------------------------
// RawHealthConfig structs
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RawHttpHealthConfig {
    pub url: String,
    #[serde(default = "default_http_interval")]
    pub interval: String,
    #[serde(default = "default_http_timeout")]
    pub timeout: String,
    #[serde(default = "default_http_retries")]
    pub retries: u32,
    #[serde(default = "default_http_expected_status")]
    pub expected_status: u16,
}

fn default_http_interval() -> String {
    "10s".into()
}
fn default_http_timeout() -> String {
    "3s".into()
}
fn default_http_retries() -> u32 {
    3
}
fn default_http_expected_status() -> u16 {
    200
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RawTcpHealthConfig {
    #[serde(default = "default_localhost")]
    pub host: String,
    pub port: u16,
    #[serde(default = "default_tcp_interval")]
    pub interval: String,
    #[serde(default = "default_tcp_timeout")]
    pub timeout: String,
    #[serde(default = "default_tcp_retries")]
    pub retries: u32,
}

fn default_localhost() -> String {
    "127.0.0.1".into()
}
fn default_tcp_interval() -> String {
    "5s".into()
}
fn default_tcp_timeout() -> String {
    "2s".into()
}
fn default_tcp_retries() -> u32 {
    3
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RawScriptHealthConfig {
    pub command: String,
    #[serde(default = "default_script_interval")]
    pub interval: String,
    #[serde(default = "default_script_timeout")]
    pub timeout: String,
    #[serde(default = "default_script_retries")]
    pub retries: u32,
}

fn default_script_interval() -> String {
    "15s".into()
}
fn default_script_timeout() -> String {
    "5s".into()
}
fn default_script_retries() -> u32 {
    1
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RawHealthConfig {
    pub http: Option<RawHttpHealthConfig>,
    pub tcp: Option<RawTcpHealthConfig>,
    pub script: Option<RawScriptHealthConfig>,
}

impl RawHealthConfig {
    /// Convert to a [`HealthConfig`]. Priority: http > tcp > script.
    /// Returns `None` if none of the sub-configs are present.
    pub fn to_health_config(&self) -> Option<HealthConfig> {
        if let Some(h) = &self.http {
            let interval_ms = parse_duration_ms(&h.interval).unwrap_or(10_000);
            let timeout_ms = parse_duration_ms(&h.timeout).unwrap_or(3_000);
            return Some(HealthConfig {
                kind: HealthCheckKind::Http {
                    url: h.url.clone(),
                    expected_status: h.expected_status,
                },
                interval_ms,
                timeout_ms,
                retries: h.retries,
            });
        }

        if let Some(t) = &self.tcp {
            let interval_ms = parse_duration_ms(&t.interval).unwrap_or(5_000);
            let timeout_ms = parse_duration_ms(&t.timeout).unwrap_or(2_000);
            return Some(HealthConfig {
                kind: HealthCheckKind::Tcp {
                    host: t.host.clone(),
                    port: t.port,
                },
                interval_ms,
                timeout_ms,
                retries: t.retries,
            });
        }

        if let Some(s) = &self.script {
            let interval_ms = parse_duration_ms(&s.interval).unwrap_or(15_000);
            let timeout_ms = parse_duration_ms(&s.timeout).unwrap_or(5_000);
            return Some(HealthConfig {
                kind: HealthCheckKind::Script {
                    command: s.command.clone(),
                },
                interval_ms,
                timeout_ms,
                retries: s.retries,
            });
        }

        None
    }
}

// ---------------------------------------------------------------------------
// RawGroupConfig
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RawGroupConfig {
    #[serde(default)]
    pub depends_on: Vec<String>,
    pub processes: Vec<String>,
}

// ---------------------------------------------------------------------------
// RawProcessConfig
// ---------------------------------------------------------------------------

/// Raw, deserialized process configuration before any env expansion or type
/// conversion.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RawProcessConfig {
    pub command: String,

    #[serde(default)]
    pub args: Vec<String>,

    pub cwd: Option<String>,

    #[serde(default)]
    pub env: HashMap<String, String>,

    #[serde(default = "default_instances")]
    pub instances: u32,

    pub max_memory: Option<String>,

    #[serde(default = "default_max_restarts")]
    pub max_restarts: u32,

    #[serde(default = "default_min_uptime")]
    pub min_uptime: String,

    #[serde(default = "default_restart_delay")]
    pub restart_delay: String,

    #[serde(default = "default_grace_period")]
    pub grace_period: String,

    pub cron_restart: Option<String>,

    pub interpreter: Option<String>,

    #[serde(default)]
    pub health: Option<RawHealthConfig>,
}

fn default_instances() -> u32 {
    1
}
fn default_max_restarts() -> u32 {
    15
}
fn default_min_uptime() -> String {
    "1s".to_string()
}
fn default_restart_delay() -> String {
    "100ms".to_string()
}
fn default_grace_period() -> String {
    "5s".to_string()
}

impl RawProcessConfig {
    /// Convert to a [`ProcessConfig`], expanding env vars in string fields.
    pub fn to_process_config(&self, name: &str) -> ProcessConfig {
        let expanded_env = expand_env_map(&self.env);

        ProcessConfig {
            name: name.to_string(),
            command: expand_env(&self.command),
            args: self.args.iter().map(|a| expand_env(a)).collect(),
            cwd: self.cwd.as_deref().map(expand_env),
            env: expanded_env,
            instances: self.instances,
            max_memory_mb: self.max_memory.as_deref().and_then(parse_memory_mb),
            max_restarts: self.max_restarts,
            min_uptime_ms: parse_duration_ms(&self.min_uptime).unwrap_or(1000),
            restart_delay_ms: parse_duration_ms(&self.restart_delay).unwrap_or(100),
            grace_period_ms: parse_duration_ms(&self.grace_period).unwrap_or(5000),
            cron_restart: self.cron_restart.clone(),
            interpreter: self.interpreter.clone(),
            health_config: self.health.as_ref().and_then(|h| h.to_health_config()),
        }
    }
}

// ---------------------------------------------------------------------------
// NotificationChannelConfig
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum NotificationChannelConfig {
    #[serde(rename = "telegram")]
    Telegram {
        bot_token: String,
        chat_id: String,
        #[serde(default)]
        events: Vec<String>,
        #[serde(default = "default_throttle")]
        throttle: String,
    },
    #[serde(rename = "slack")]
    Slack {
        webhook: String,
        #[serde(default)]
        events: Vec<String>,
        #[serde(default = "default_throttle")]
        throttle: String,
    },
    #[serde(rename = "discord")]
    Discord {
        webhook: String,
        #[serde(default)]
        events: Vec<String>,
    },
    #[serde(rename = "webhook")]
    Webhook {
        url: String,
        #[serde(default)]
        headers: HashMap<String, String>,
        #[serde(default)]
        hmac_secret: Option<String>,
        #[serde(default)]
        events: Vec<String>,
    },
    #[serde(rename = "email")]
    Email {
        smtp_host: String,
        smtp_port: u16,
        from: String,
        to: Vec<String>,
        #[serde(default)]
        events: Vec<String>,
    },
    #[serde(rename = "pagerduty")]
    PagerDuty {
        routing_key: String,
        #[serde(default)]
        events: Vec<String>,
    },
    #[serde(rename = "teams")]
    Teams {
        webhook: String,
        #[serde(default)]
        events: Vec<String>,
    },
    #[serde(rename = "ntfy")]
    Ntfy {
        url: String,
        topic: String,
        #[serde(default)]
        events: Vec<String>,
    },
}

fn default_throttle() -> String {
    "60s".into()
}

// ---------------------------------------------------------------------------
// LogSinkConfig
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum LogSinkConfig {
    #[serde(rename = "gelf")]
    Gelf {
        host: String,
        port: u16,
        #[serde(default = "default_udp")]
        transport: String,
        #[serde(default = "default_star")]
        processes: String,
    },
    #[serde(rename = "loki")]
    Loki {
        url: String,
        #[serde(default)]
        labels: HashMap<String, String>,
        #[serde(default = "default_star")]
        processes: String,
    },
    #[serde(rename = "elasticsearch")]
    Elasticsearch {
        url: String,
        index: String,
        #[serde(default = "default_star")]
        processes: String,
    },
    #[serde(rename = "syslog")]
    Syslog {
        host: String,
        port: u16,
        #[serde(default = "default_udp")]
        transport: String,
        #[serde(default = "default_star")]
        processes: String,
    },
}

fn default_udp() -> String {
    "udp".into()
}
fn default_star() -> String {
    "*".into()
}

// ---------------------------------------------------------------------------
// LogsConfig
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LogsConfig {
    #[serde(default)]
    pub sinks: HashMap<String, LogSinkConfig>,
}

// ---------------------------------------------------------------------------
// AlertRuleConfig
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlertRuleConfig {
    pub process: String,
    /// e.g. "memory > 450MB for 5m"
    pub condition: String,
    #[serde(default)]
    pub notify: Vec<String>,
    pub action: Option<String>,
    #[serde(default = "default_cooldown")]
    pub cooldown: String,
}

fn default_cooldown() -> String {
    "10m".into()
}

// ---------------------------------------------------------------------------
// EscalationConfig
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EscalationConfig {
    pub chain: Vec<String>,
    pub escalate_after: String,
}

// ---------------------------------------------------------------------------
// DeployEnvConfig
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeployEnvConfig {
    pub repo: String,
    pub branch: String,
    pub path: String,
    #[serde(default)]
    pub pre_deploy: Vec<String>,
    #[serde(default)]
    pub post_deploy: Vec<String>,
}

// ---------------------------------------------------------------------------
// RemoteApiConfig
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoteApiConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_remote_listen")]
    pub listen: String,
    #[serde(default)]
    pub cert: String,
    #[serde(default)]
    pub key: String,
    #[serde(default)]
    pub ca: String,
}

fn default_remote_listen() -> String {
    "0.0.0.0:9615".into()
}

// ---------------------------------------------------------------------------
// EcosystemConfig
// ---------------------------------------------------------------------------

/// Top-level ecosystem configuration, holding one or more named processes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EcosystemConfig {
    pub process: HashMap<String, RawProcessConfig>,
    #[serde(default)]
    pub groups: HashMap<String, RawGroupConfig>,
    #[serde(default)]
    pub notifications: HashMap<String, NotificationChannelConfig>,
    #[serde(default)]
    pub alerts: HashMap<String, AlertRuleConfig>,
    #[serde(default)]
    pub escalation: Option<EscalationConfig>,
    /// Log sinks go under [logs.sinks.*]
    #[serde(default)]
    pub logs: Option<LogsConfig>,
    #[serde(default)]
    pub deploy: HashMap<String, DeployEnvConfig>,
    #[serde(default)]
    pub remote: Option<RemoteApiConfig>,
}

impl EcosystemConfig {
    /// Read and parse a config file. The format is inferred from the file
    /// extension: `.toml`, `.yaml`/`.yml`, or `.json`.
    pub fn from_file(path: &Path) -> Result<Self, EcosystemError> {
        let content = std::fs::read_to_string(path)?;
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_lowercase();
        Self::from_str(&content, &ext)
    }

    /// Parse a config from a string with an explicit `format` hint.
    ///
    /// Supported formats: `"toml"`, `"yaml"`, `"yml"`, `"json"`.
    pub fn from_str(content: &str, format: &str) -> Result<Self, EcosystemError> {
        match format.to_lowercase().as_str() {
            "toml" => Ok(toml::from_str(content)?),
            "yaml" | "yml" => Ok(serde_yaml::from_str(content)?),
            "json" => Ok(serde_json::from_str(content)?),
            other => Err(EcosystemError::UnsupportedFormat(other.to_string())),
        }
    }

    /// Convert every named process entry to a `Vec<ProcessConfig>`.
    pub fn to_process_configs(&self) -> Vec<ProcessConfig> {
        let mut configs: Vec<ProcessConfig> = self
            .process
            .iter()
            .map(|(name, raw)| raw.to_process_config(name))
            .collect();
        // Deterministic ordering by name so callers can rely on it.
        configs.sort_by(|a, b| a.name.cmp(&b.name));
        configs
    }

    /// Convert raw group entries to a `HashMap<String, GroupConfig>`.
    pub fn to_group_configs(&self) -> HashMap<String, GroupConfig> {
        self.groups
            .iter()
            .map(|(name, raw)| {
                let group = GroupConfig {
                    depends_on: raw.depends_on.clone(),
                    processes: raw.processes.clone(),
                };
                (name.clone(), group)
            })
            .collect()
    }
}

// ---------------------------------------------------------------------------
// parse_memory_mb
// ---------------------------------------------------------------------------

/// Parse a human-readable memory string to megabytes.
///
/// Accepted formats (case-insensitive):
/// - `"512MB"` → 512
/// - `"1GB"` → 1024
/// - `"1024KB"` → 1
/// - `"256"` → 256 (bare number assumed MB)
pub fn parse_memory_mb(input: &str) -> Option<u64> {
    let trimmed = input.trim();
    let upper = trimmed.to_uppercase();

    let (num_str, multiplier): (&str, u64) = if let Some(s) = upper.strip_suffix("GB") {
        (s, 1024)
    } else if let Some(s) = upper.strip_suffix("MB") {
        (s, 1)
    } else if let Some(s) = upper.strip_suffix("KB") {
        // KB → round down to MB (integer division)
        let kb: u64 = s.trim().parse().ok()?;
        return Some(kb / 1024);
    } else {
        // Bare number — assumed to be MB
        (trimmed, 1)
    };

    let num: u64 = num_str.trim().parse().ok()?;
    Some(num * multiplier)
}

// ---------------------------------------------------------------------------
// parse_duration_ms
// ---------------------------------------------------------------------------

/// Parse a human-readable duration string to milliseconds.
///
/// Accepted formats (case-insensitive):
/// - `"5s"` → 5000
/// - `"100ms"` → 100
/// - `"1m"` → 60000
/// - `"1h"` → 3600000
/// - `"500"` → 500 (bare number assumed ms)
pub fn parse_duration_ms(input: &str) -> Option<u64> {
    let trimmed = input.trim();
    let lower = trimmed.to_lowercase();

    // Order matters: check "ms" before "m" to avoid false positive.
    let (num_str, multiplier): (&str, u64) = if let Some(s) = lower.strip_suffix("ms") {
        (s, 1)
    } else if let Some(s) = lower.strip_suffix("h") {
        (s, 3_600_000)
    } else if let Some(s) = lower.strip_suffix("m") {
        (s, 60_000)
    } else if let Some(s) = lower.strip_suffix("s") {
        (s, 1_000)
    } else {
        // Bare number — assumed milliseconds
        (trimmed, 1)
    };

    let num: u64 = num_str.trim().parse().ok()?;
    Some(num * multiplier)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // 1. parse_memory_mb
    #[test]
    fn test_parse_memory_mb() {
        assert_eq!(parse_memory_mb("512MB"), Some(512));
        assert_eq!(parse_memory_mb("1GB"), Some(1024));
        assert_eq!(parse_memory_mb("2048KB"), Some(2));
        assert_eq!(parse_memory_mb("256"), Some(256));
        assert_eq!(parse_memory_mb(""), None);
    }

    // 2. parse_duration_ms
    #[test]
    fn test_parse_duration_ms() {
        assert_eq!(parse_duration_ms("5s"), Some(5000));
        assert_eq!(parse_duration_ms("100ms"), Some(100));
        assert_eq!(parse_duration_ms("1m"), Some(60_000));
        assert_eq!(parse_duration_ms("1h"), Some(3_600_000));
        assert_eq!(parse_duration_ms("500"), Some(500));
        assert_eq!(parse_duration_ms(""), None);
    }

    // 3. TOML parsing
    #[test]
    fn test_toml_parsing() {
        let toml = r#"
[process.api]
command = "node"
args = ["server.js"]
instances = 2
"#;
        let cfg = EcosystemConfig::from_str(toml, "toml").expect("parse toml");
        assert!(cfg.process.contains_key("api"));
        assert_eq!(cfg.process["api"].command, "node");
        assert_eq!(cfg.process["api"].instances, 2);
    }

    // 4. YAML parsing
    #[test]
    fn test_yaml_parsing() {
        let yaml = r#"
process:
  worker:
    command: python
    args:
      - worker.py
    instances: 3
"#;
        let cfg = EcosystemConfig::from_str(yaml, "yaml").expect("parse yaml");
        assert!(cfg.process.contains_key("worker"));
        assert_eq!(cfg.process["worker"].command, "python");
        assert_eq!(cfg.process["worker"].instances, 3);
    }

    // 5. JSON parsing
    #[test]
    fn test_json_parsing() {
        let json = r#"
{
  "process": {
    "web": {
      "command": "nginx",
      "instances": 1
    }
  }
}
"#;
        let cfg = EcosystemConfig::from_str(json, "json").expect("parse json");
        assert!(cfg.process.contains_key("web"));
        assert_eq!(cfg.process["web"].command, "nginx");
    }

    // 6. to_process_configs
    #[test]
    fn test_to_process_configs() {
        let toml = r#"
[process.alpha]
command = "alpha-bin"

[process.beta]
command = "beta-bin"
instances = 4
"#;
        let cfg = EcosystemConfig::from_str(toml, "toml").expect("parse");
        let configs = cfg.to_process_configs();
        assert_eq!(configs.len(), 2);
        // Sorted by name: alpha before beta
        assert_eq!(configs[0].name, "alpha");
        assert_eq!(configs[1].name, "beta");
        assert_eq!(configs[1].instances, 4);
    }

    // 7. Env expansion in config values
    #[test]
    fn test_env_expansion_in_config() {
        std::env::set_var("MHOST_ECO_TEST", "expanded");
        let toml = r#"
[process.svc]
command = "${MHOST_ECO_TEST}-server"
"#;
        let cfg = EcosystemConfig::from_str(toml, "toml").expect("parse");
        let configs = cfg.to_process_configs();
        assert_eq!(configs[0].command, "expanded-server");
        std::env::remove_var("MHOST_ECO_TEST");
    }

    // 8. Unsupported format returns an error
    #[test]
    fn test_unsupported_format_error() {
        let result = EcosystemConfig::from_str("{}", "xml");
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, EcosystemError::UnsupportedFormat(_)));
    }

    // 9. Default values applied when fields are absent
    #[test]
    fn test_default_values() {
        let toml = r#"
[process.minimal]
command = "true"
"#;
        let cfg = EcosystemConfig::from_str(toml, "toml").expect("parse");
        let configs = cfg.to_process_configs();
        let pc = &configs[0];
        assert_eq!(pc.instances, 1);
        assert_eq!(pc.max_restarts, 15);
        assert_eq!(pc.min_uptime_ms, 1000);
        assert_eq!(pc.restart_delay_ms, 100);
        assert_eq!(pc.grace_period_ms, 5000);
        assert!(pc.max_memory_mb.is_none());
        assert!(pc.cron_restart.is_none());
        assert!(pc.interpreter.is_none());
    }

    // 10. Health HTTP config parsing
    #[test]
    fn test_health_http_parsing() {
        let toml = r#"
[process.api]
command = "node"
args = ["server.js"]

[process.api.health.http]
url = "http://localhost:8080/health"
interval = "15s"
timeout = "2s"
retries = 5
expected_status = 200
"#;
        let cfg = EcosystemConfig::from_str(toml, "toml").expect("parse toml with health");
        let configs = cfg.to_process_configs();
        let pc = &configs[0];
        let health = pc.health_config.as_ref().expect("health config present");

        assert_eq!(health.interval_ms, 15_000);
        assert_eq!(health.timeout_ms, 2_000);
        assert_eq!(health.retries, 5);

        match &health.kind {
            mhost_core::health::HealthCheckKind::Http {
                url,
                expected_status,
            } => {
                assert_eq!(url, "http://localhost:8080/health");
                assert_eq!(*expected_status, 200);
            }
            other => panic!("expected Http kind, got: {:?}", other),
        }
    }

    // 12. Notification channel and log sink config parsing
    #[test]
    fn test_notification_and_log_sink_config_parsing() {
        let toml = r#"
[process.api]
command = "node"

[notifications.telegram]
type = "telegram"
bot_token = "abc123"
chat_id = "-100999"
events = ["crash", "restart"]

[notifications.slack]
type = "slack"
webhook = "https://hooks.slack.com/T/B/xxx"

[logs.sinks.graylog]
type = "gelf"
host = "logs.example.com"
port = 12201
"#;
        let cfg = EcosystemConfig::from_str(toml, "toml")
            .expect("parse toml with notifications and sinks");

        let tg = cfg.notifications.get("telegram").expect("telegram channel");
        match tg {
            NotificationChannelConfig::Telegram {
                bot_token,
                chat_id,
                events,
                throttle,
            } => {
                assert_eq!(bot_token, "abc123");
                assert_eq!(chat_id, "-100999");
                assert_eq!(events, &["crash", "restart"]);
                assert_eq!(throttle, "60s");
            }
            other => panic!("expected Telegram, got: {:?}", other),
        }

        let sl = cfg.notifications.get("slack").expect("slack channel");
        match sl {
            NotificationChannelConfig::Slack {
                webhook,
                events,
                throttle,
            } => {
                assert!(webhook.contains("slack.com"));
                assert!(events.is_empty());
                assert_eq!(throttle, "60s");
            }
            other => panic!("expected Slack, got: {:?}", other),
        }

        let logs_cfg = cfg.logs.as_ref().expect("logs config present");
        let graylog = logs_cfg.sinks.get("graylog").expect("graylog sink");
        match graylog {
            LogSinkConfig::Gelf {
                host,
                port,
                transport,
                processes,
            } => {
                assert_eq!(host, "logs.example.com");
                assert_eq!(*port, 12201);
                assert_eq!(transport, "udp");
                assert_eq!(processes, "*");
            }
            other => panic!("expected Gelf, got: {:?}", other),
        }
    }

    // 13. Deploy and remote config parsing
    #[test]
    fn test_deploy_and_remote_config_parsing() {
        let toml = r#"
[process.api]
command = "node"

[deploy.production]
repo = "https://github.com/example/app"
branch = "main"
path = "/srv/app"
pre_deploy = ["npm install"]
post_deploy = ["pm2 reload all"]

[remote]
enabled = true
listen = "0.0.0.0:9615"
cert = "/etc/mhost/server.crt"
key = "/etc/mhost/server.key"
ca = "/etc/mhost/ca.crt"
"#;
        let cfg =
            EcosystemConfig::from_str(toml, "toml").expect("parse toml with deploy and remote");

        let prod = cfg.deploy.get("production").expect("production deploy");
        assert_eq!(prod.repo, "https://github.com/example/app");
        assert_eq!(prod.branch, "main");
        assert_eq!(prod.path, "/srv/app");
        assert_eq!(prod.pre_deploy, vec!["npm install".to_string()]);
        assert_eq!(prod.post_deploy, vec!["pm2 reload all".to_string()]);

        let remote = cfg.remote.as_ref().expect("remote config present");
        assert!(remote.enabled);
        assert_eq!(remote.listen, "0.0.0.0:9615");
        assert_eq!(remote.cert, "/etc/mhost/server.crt");
    }

    // 14. Remote config defaults
    #[test]
    fn test_remote_config_defaults() {
        let toml = r#"
[process.api]
command = "node"

[remote]
enabled = false
"#;
        let cfg = EcosystemConfig::from_str(toml, "toml").expect("parse");
        let remote = cfg.remote.as_ref().expect("remote config present");
        assert!(!remote.enabled);
        assert_eq!(remote.listen, "0.0.0.0:9615");
        assert_eq!(remote.cert, "");
        assert_eq!(remote.key, "");
        assert_eq!(remote.ca, "");
    }

    // 11. Group config parsing
    #[test]
    fn test_group_config_parsing() {
        let toml = r#"
[process.api]
command = "node"

[process.db]
command = "postgres"

[groups.backend]
depends_on = ["infra"]
processes = ["api"]

[groups.infra]
processes = ["db"]
"#;
        let cfg = EcosystemConfig::from_str(toml, "toml").expect("parse toml with groups");
        let groups = cfg.to_group_configs();

        assert_eq!(groups.len(), 2);

        let backend = groups.get("backend").expect("backend group");
        assert_eq!(backend.depends_on, vec!["infra".to_string()]);
        assert_eq!(backend.processes, vec!["api".to_string()]);

        let infra = groups.get("infra").expect("infra group");
        assert!(infra.depends_on.is_empty());
        assert_eq!(infra.processes, vec!["db".to_string()]);
    }

    // 15. Parse TOML with health.tcp config
    #[test]
    fn test_toml_health_tcp_parsing() {
        let toml = r#"
[process.db]
command = "postgres"

[process.db.health.tcp]
host = "127.0.0.1"
port = 5432
interval = "10s"
timeout = "3s"
retries = 2
"#;
        let cfg = EcosystemConfig::from_str(toml, "toml").expect("parse toml with tcp health");
        let configs = cfg.to_process_configs();
        let pc = &configs[0];
        let health = pc.health_config.as_ref().expect("health config present");

        assert_eq!(health.interval_ms, 10_000);
        assert_eq!(health.timeout_ms, 3_000);
        assert_eq!(health.retries, 2);

        match &health.kind {
            mhost_core::health::HealthCheckKind::Tcp { host, port } => {
                assert_eq!(host, "127.0.0.1");
                assert_eq!(*port, 5432);
            }
            other => panic!("expected Tcp kind, got: {:?}", other),
        }
    }

    // 16. Parse TOML with health.script config
    #[test]
    fn test_toml_health_script_parsing() {
        let toml = r#"
[process.worker]
command = "python3"
args = ["worker.py"]

[process.worker.health.script]
command = "/usr/local/bin/check-worker.sh"
interval = "30s"
timeout = "5s"
retries = 1
"#;
        let cfg = EcosystemConfig::from_str(toml, "toml").expect("parse toml with script health");
        let configs = cfg.to_process_configs();
        let pc = &configs[0];
        let health = pc.health_config.as_ref().expect("health config present");

        assert_eq!(health.interval_ms, 30_000);
        assert_eq!(health.timeout_ms, 5_000);
        assert_eq!(health.retries, 1);

        match &health.kind {
            mhost_core::health::HealthCheckKind::Script { command } => {
                assert_eq!(command, "/usr/local/bin/check-worker.sh");
            }
            other => panic!("expected Script kind, got: {:?}", other),
        }
    }

    // 17. Parse TOML with all notification types
    #[test]
    fn test_all_notification_types_parsing() {
        let toml = r#"
[process.api]
command = "node"

[notifications.disc]
type = "discord"
webhook = "https://discord.com/api/webhooks/xxx"

[notifications.wh]
type = "webhook"
url = "https://hooks.example.com/notify"

[notifications.email]
type = "email"
smtp_host = "smtp.example.com"
smtp_port = 587
from = "noreply@example.com"
to = ["ops@example.com"]

[notifications.pd]
type = "pagerduty"
routing_key = "my-routing-key"

[notifications.ms_teams]
type = "teams"
webhook = "https://outlook.office.com/webhook/xxx"

[notifications.ntfy_ch]
type = "ntfy"
url = "https://ntfy.sh"
topic = "mhost-alerts"
"#;
        let cfg = EcosystemConfig::from_str(toml, "toml").expect("parse all notification types");

        assert!(matches!(
            cfg.notifications.get("disc"),
            Some(NotificationChannelConfig::Discord { .. })
        ));
        assert!(matches!(
            cfg.notifications.get("wh"),
            Some(NotificationChannelConfig::Webhook { .. })
        ));
        assert!(matches!(
            cfg.notifications.get("email"),
            Some(NotificationChannelConfig::Email { .. })
        ));
        assert!(matches!(
            cfg.notifications.get("pd"),
            Some(NotificationChannelConfig::PagerDuty { .. })
        ));
        assert!(matches!(
            cfg.notifications.get("ms_teams"),
            Some(NotificationChannelConfig::Teams { .. })
        ));
        assert!(matches!(
            cfg.notifications.get("ntfy_ch"),
            Some(NotificationChannelConfig::Ntfy { .. })
        ));
    }

    // 18. Parse TOML with empty groups section
    #[test]
    fn test_parse_empty_groups() {
        let toml = r#"
[process.api]
command = "node"
"#;
        let cfg = EcosystemConfig::from_str(toml, "toml").expect("parse");
        let groups = cfg.to_group_configs();
        assert!(groups.is_empty());
    }

    // 19. Parse TOML with deploy config
    #[test]
    fn test_parse_deploy_config() {
        let toml = r#"
[process.api]
command = "node"

[deploy.staging]
repo = "https://github.com/example/app"
branch = "develop"
path = "/srv/staging"

[deploy.production]
repo = "https://github.com/example/app"
branch = "main"
path = "/srv/production"
pre_deploy = ["make build"]
post_deploy = ["systemctl reload nginx"]
"#;
        let cfg = EcosystemConfig::from_str(toml, "toml").expect("parse deploy config");

        assert_eq!(cfg.deploy.len(), 2);

        let staging = cfg.deploy.get("staging").expect("staging");
        assert_eq!(staging.branch, "develop");
        assert!(staging.pre_deploy.is_empty());
        assert!(staging.post_deploy.is_empty());

        let production = cfg.deploy.get("production").expect("production");
        assert_eq!(production.branch, "main");
        assert_eq!(production.pre_deploy, vec!["make build".to_string()]);
    }

    // 20. Parse TOML with remote config
    #[test]
    fn test_parse_remote_config_enabled() {
        let toml = r#"
[process.api]
command = "node"

[remote]
enabled = true
listen = "127.0.0.1:9615"
"#;
        let cfg = EcosystemConfig::from_str(toml, "toml").expect("parse remote config");
        let remote = cfg.remote.as_ref().expect("remote present");
        assert!(remote.enabled);
        assert_eq!(remote.listen, "127.0.0.1:9615");
    }
}
