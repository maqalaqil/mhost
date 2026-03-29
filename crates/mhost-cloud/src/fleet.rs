use crate::config::FleetConfig;
use crate::install::RemoteInstaller;
use crate::remote::{RemoteHost, ServerStatus};
use crate::ssh::SshExecutor;
use std::path::Path;

pub struct Fleet {
    pub config: FleetConfig,
}

impl Fleet {
    pub fn new(config: FleetConfig) -> Self {
        Self { config }
    }

    pub async fn status_all(&self) -> Vec<ServerStatus> {
        let mut results = Vec::new();
        for (name, server) in &self.config.servers {
            let host = RemoteHost::new(name, server);
            results.push(host.get_status().await);
        }
        results
    }

    pub async fn exec_all(&self, mhost_args: &[&str]) -> Vec<(String, Result<String, String>)> {
        let mut results = Vec::new();
        for (name, server) in &self.config.servers {
            let host = RemoteHost::new(name, server);
            let result = host.ssh.exec_mhost(mhost_args).await;
            results.push((name.clone(), result));
        }
        results
    }

    pub async fn sync_config(&self, config_path: &Path) -> Vec<(String, Result<String, String>)> {
        let mut results = Vec::new();
        for (name, server) in &self.config.servers {
            let host = RemoteHost::new(name, server);
            let result = host.deploy_config(config_path).await;
            results.push((name.clone(), result));
        }
        results
    }

    pub async fn install_all(&self) -> Vec<(String, Result<String, String>)> {
        let mut results = Vec::new();
        for (name, server) in &self.config.servers {
            let ssh = SshExecutor::from_server_config(server);
            let result = RemoteInstaller::install(&ssh).await;
            results.push((name.clone(), result));
        }
        results
    }

    pub async fn update_all(&self) -> Vec<(String, Result<String, String>)> {
        self.install_all().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{AuthMethod, ServerConfig};
    use std::collections::HashMap;

    fn make_server(host: &str) -> ServerConfig {
        ServerConfig {
            host: host.to_string(),
            port: 22,
            user: "ubuntu".to_string(),
            auth: AuthMethod::Key,
            key_path: None,
            tags: vec!["prod".to_string()],
            provider: Some("aws".to_string()),
            instance_id: Some("i-abc".to_string()),
            region: Some("us-east-1".to_string()),
        }
    }

    fn make_fleet_config(servers: Vec<(&str, &str)>) -> FleetConfig {
        let mut cfg = FleetConfig::default();
        for (name, host) in servers {
            cfg.add_server(name, make_server(host));
        }
        cfg
    }

    #[test]
    fn test_fleet_new_stores_config() {
        let cfg = make_fleet_config(vec![("web1", "10.0.0.1"), ("web2", "10.0.0.2")]);
        let fleet = Fleet::new(cfg);
        assert_eq!(fleet.config.servers.len(), 2);
        assert!(fleet.config.get_server("web1").is_some());
        assert!(fleet.config.get_server("web2").is_some());
    }

    #[test]
    fn test_fleet_empty_config() {
        let fleet = Fleet::new(FleetConfig::default());
        assert!(fleet.config.servers.is_empty());
    }

    #[test]
    fn test_fleet_config_server_lookup() {
        let cfg = make_fleet_config(vec![("db1", "10.0.1.1")]);
        let fleet = Fleet::new(cfg);
        let server = fleet.config.get_server("db1").expect("db1 should exist");
        assert_eq!(server.host, "10.0.1.1");
        assert_eq!(server.user, "ubuntu");
    }

    #[test]
    fn test_fleet_with_groups() {
        let mut cfg = make_fleet_config(vec![("web1", "10.0.0.1"), ("web2", "10.0.0.2")]);
        cfg.groups.insert(
            "frontend".to_string(),
            vec!["web1".to_string(), "web2".to_string()],
        );
        let fleet = Fleet::new(cfg);
        let members = fleet.config.servers_in_group("frontend");
        assert_eq!(members.len(), 2);
    }

    #[test]
    fn test_fleet_servers_by_tag() {
        let cfg = make_fleet_config(vec![("web1", "10.0.0.1"), ("web2", "10.0.0.2")]);
        let fleet = Fleet::new(cfg);
        let prod_servers = fleet.config.servers_by_tag("prod");
        assert_eq!(prod_servers.len(), 2);
    }

    #[test]
    fn test_fleet_config_via_hashmap() {
        let mut servers = HashMap::new();
        servers.insert("srv1".to_string(), make_server("1.2.3.4"));
        let cfg = FleetConfig {
            servers,
            groups: HashMap::new(),
        };
        let fleet = Fleet::new(cfg);
        assert_eq!(fleet.config.servers.len(), 1);
    }
}
