use std::collections::HashMap;
use std::path::Path;

use serde::{Deserialize, Serialize};
use thiserror::Error;

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
            health_config: None,
        }
    }
}

// ---------------------------------------------------------------------------
// EcosystemConfig
// ---------------------------------------------------------------------------

/// Top-level ecosystem configuration, holding one or more named processes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EcosystemConfig {
    pub process: HashMap<String, RawProcessConfig>,
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
}
