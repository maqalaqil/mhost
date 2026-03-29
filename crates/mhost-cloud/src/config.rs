use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct FleetConfig {
    #[serde(default)]
    pub servers: HashMap<String, ServerConfig>,
    #[serde(default)]
    pub groups: HashMap<String, Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ServerConfig {
    pub host: String,
    #[serde(default = "default_port")]
    pub port: u16,
    #[serde(default = "default_user")]
    pub user: String,
    #[serde(default)]
    pub auth: AuthMethod,
    #[serde(default)]
    pub key_path: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub provider: Option<String>,
    #[serde(default)]
    pub instance_id: Option<String>,
    #[serde(default)]
    pub region: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum AuthMethod {
    #[default]
    Key,
    KeyPassphrase,
    Password,
    Agent,
}

fn default_port() -> u16 {
    22
}

fn default_user() -> String {
    "root".into()
}

impl FleetConfig {
    /// Load a fleet config from a JSON file at `path`.
    pub fn load(path: &Path) -> Result<Self, String> {
        let content = std::fs::read_to_string(path)
            .map_err(|e| format!("Failed to read fleet config '{}': {e}", path.display()))?;
        serde_json::from_str(&content)
            .map_err(|e| format!("Failed to parse fleet config '{}': {e}", path.display()))
    }

    /// Persist the fleet config as JSON to `path`.
    pub fn save(&self, path: &Path) -> Result<(), String> {
        let json = serde_json::to_string_pretty(self)
            .map_err(|e| format!("Failed to serialize fleet config: {e}"))?;
        std::fs::write(path, json)
            .map_err(|e| format!("Failed to write fleet config '{}': {e}", path.display()))
    }

    /// Insert or replace a server entry by name. Returns a new `FleetConfig`.
    pub fn add_server(&mut self, name: &str, config: ServerConfig) {
        self.servers.insert(name.to_string(), config);
    }

    /// Remove a server by name. Returns `true` if the server existed.
    pub fn remove_server(&mut self, name: &str) -> bool {
        self.servers.remove(name).is_some()
    }

    /// Look up a server by name.
    pub fn get_server(&self, name: &str) -> Option<&ServerConfig> {
        self.servers.get(name)
    }

    /// Return the list of server names belonging to a group.
    pub fn servers_in_group(&self, group: &str) -> Vec<&str> {
        self.groups
            .get(group)
            .map(|names| names.iter().map(String::as_str).collect())
            .unwrap_or_default()
    }

    /// Return all (name, config) pairs whose tags include `tag`.
    pub fn servers_by_tag<'a>(&'a self, tag: &str) -> Vec<(&'a str, &'a ServerConfig)> {
        self.servers
            .iter()
            .filter(|(_, cfg)| cfg.tags.iter().any(|t| t == tag))
            .map(|(name, cfg)| (name.as_str(), cfg))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;

    fn sample_server(host: &str) -> ServerConfig {
        ServerConfig {
            host: host.to_string(),
            port: 22,
            user: "ubuntu".to_string(),
            auth: AuthMethod::Key,
            key_path: Some("~/.ssh/id_rsa".to_string()),
            tags: vec!["web".to_string(), "prod".to_string()],
            provider: Some("aws".to_string()),
            instance_id: Some("i-abc123".to_string()),
            region: Some("us-east-1".to_string()),
        }
    }

    #[test]
    fn test_default_fleet_config_is_empty() {
        let cfg = FleetConfig::default();
        assert!(cfg.servers.is_empty());
        assert!(cfg.groups.is_empty());
    }

    #[test]
    fn test_add_and_get_server() {
        let mut cfg = FleetConfig::default();
        let srv = sample_server("10.0.0.1");
        cfg.add_server("web1", srv);
        let found = cfg.get_server("web1").expect("server not found");
        assert_eq!(found.host, "10.0.0.1");
        assert_eq!(found.user, "ubuntu");
    }

    #[test]
    fn test_get_server_missing_returns_none() {
        let cfg = FleetConfig::default();
        assert!(cfg.get_server("missing").is_none());
    }

    #[test]
    fn test_remove_server_existing() {
        let mut cfg = FleetConfig::default();
        cfg.add_server("web1", sample_server("10.0.0.1"));
        assert!(cfg.remove_server("web1"));
        assert!(cfg.get_server("web1").is_none());
    }

    #[test]
    fn test_remove_server_missing_returns_false() {
        let mut cfg = FleetConfig::default();
        assert!(!cfg.remove_server("nope"));
    }

    #[test]
    fn test_load_save_roundtrip() {
        let mut cfg = FleetConfig::default();
        cfg.add_server("web1", sample_server("192.168.1.1"));
        cfg.groups.insert("production".to_string(), vec!["web1".to_string()]);

        let file = NamedTempFile::new().unwrap();
        cfg.save(file.path()).expect("save failed");

        let loaded = FleetConfig::load(file.path()).expect("load failed");
        assert_eq!(loaded.servers.len(), 1);
        let srv = loaded.get_server("web1").unwrap();
        assert_eq!(srv.host, "192.168.1.1");
        assert_eq!(srv.port, 22);
        assert_eq!(loaded.groups["production"], vec!["web1".to_string()]);
    }

    #[test]
    fn test_servers_in_group() {
        let mut cfg = FleetConfig::default();
        cfg.add_server("web1", sample_server("1.1.1.1"));
        cfg.add_server("web2", sample_server("1.1.1.2"));
        cfg.groups.insert(
            "frontend".to_string(),
            vec!["web1".to_string(), "web2".to_string()],
        );
        let mut members = cfg.servers_in_group("frontend");
        members.sort();
        assert_eq!(members, vec!["web1", "web2"]);
    }

    #[test]
    fn test_servers_in_group_unknown_returns_empty() {
        let cfg = FleetConfig::default();
        assert!(cfg.servers_in_group("ghost").is_empty());
    }

    #[test]
    fn test_servers_by_tag() {
        let mut cfg = FleetConfig::default();
        cfg.add_server("web1", sample_server("1.1.1.1")); // tags: web, prod
        let mut db_srv = sample_server("1.1.1.2");
        db_srv.tags = vec!["db".to_string(), "prod".to_string()];
        cfg.add_server("db1", db_srv);

        let web_servers = cfg.servers_by_tag("web");
        assert_eq!(web_servers.len(), 1);
        assert_eq!(web_servers[0].0, "web1");

        let prod_servers = cfg.servers_by_tag("prod");
        assert_eq!(prod_servers.len(), 2);
    }

    #[test]
    fn test_servers_by_tag_no_match() {
        let cfg = FleetConfig::default();
        assert!(cfg.servers_by_tag("staging").is_empty());
    }

    #[test]
    fn test_default_values_on_deserialize() {
        let json = r#"{ "servers": { "s1": { "host": "10.0.0.1" } }, "groups": {} }"#;
        let cfg: FleetConfig = serde_json::from_str(json).unwrap();
        let srv = cfg.get_server("s1").unwrap();
        assert_eq!(srv.port, 22);
        assert_eq!(srv.user, "root");
        assert_eq!(srv.auth, AuthMethod::Key);
        assert!(srv.key_path.is_none());
        assert!(srv.tags.is_empty());
    }
}
