use crate::config::ServerConfig;
use crate::ssh::SshExecutor;
use std::path::Path;

pub struct RemoteHost {
    pub name: String,
    pub ssh: SshExecutor,
}

impl RemoteHost {
    pub fn new(name: &str, config: &ServerConfig) -> Self {
        Self {
            name: name.into(),
            ssh: SshExecutor::from_server_config(config),
        }
    }

    pub async fn list_processes(&self) -> Result<String, String> {
        self.ssh.exec_mhost(&["list"]).await
    }

    pub async fn start(&self, target: &str) -> Result<String, String> {
        self.ssh.exec_mhost(&["start", target]).await
    }

    pub async fn stop(&self, target: &str) -> Result<String, String> {
        self.ssh.exec_mhost(&["stop", target]).await
    }

    pub async fn restart(&self, target: &str) -> Result<String, String> {
        self.ssh.exec_mhost(&["restart", target]).await
    }

    pub async fn scale(&self, name: &str, instances: u32) -> Result<String, String> {
        self.ssh
            .exec_mhost(&["scale", name, &instances.to_string()])
            .await
    }

    pub async fn deploy_config(&self, local_config: &Path) -> Result<String, String> {
        self.ssh
            .upload(local_config, "/tmp/mhost-deploy.toml")
            .await?;
        self.ssh
            .exec_mhost(&["start", "/tmp/mhost-deploy.toml"])
            .await
    }

    pub async fn stream_logs(&self, process_name: &str) -> Result<String, String> {
        let cmd = format!(
            "tail -50 ~/.mhost/logs/{}-0-out.log 2>/dev/null || echo 'No logs found'",
            process_name
        );
        self.ssh.exec(&cmd).await.map(|o| o.stdout)
    }

    pub async fn get_status(&self) -> ServerStatus {
        let reachable = self.ssh.is_reachable().await;
        if !reachable {
            return ServerStatus {
                name: self.name.clone(),
                online: false,
                mhost_installed: false,
                process_count: 0,
                cpu: None,
                memory: None,
            };
        }

        let mhost_installed = self.ssh.is_mhost_installed().await;
        let process_count = if mhost_installed {
            self.ssh
                .exec_mhost(&["list"])
                .await
                .map(|out| out.lines().filter(|l| l.contains("online")).count() as u32)
                .unwrap_or(0)
        } else {
            0
        };

        let cpu = self
            .ssh
            .exec("top -bn1 | head -3 | tail -1 | awk '{print $2}' 2>/dev/null || echo 0")
            .await
            .ok()
            .and_then(|o| o.stdout.trim().parse::<f32>().ok());

        let memory = self
            .ssh
            .exec("free -m 2>/dev/null | awk '/Mem:/{print $3}' || sysctl -n hw.memsize 2>/dev/null | awk '{print $1/1048576}'")
            .await
            .ok()
            .and_then(|o| o.stdout.trim().parse::<f64>().ok());

        ServerStatus {
            name: self.name.clone(),
            online: true,
            mhost_installed,
            process_count,
            cpu,
            memory,
        }
    }
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct ServerStatus {
    pub name: String,
    pub online: bool,
    pub mhost_installed: bool,
    pub process_count: u32,
    pub cpu: Option<f32>,
    pub memory: Option<f64>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{AuthMethod, ServerConfig};

    fn make_server_config(host: &str) -> ServerConfig {
        ServerConfig {
            host: host.to_string(),
            port: 22,
            user: "ubuntu".to_string(),
            auth: AuthMethod::Key,
            key_path: Some("~/.ssh/id_rsa".to_string()),
            tags: vec!["web".to_string()],
            provider: Some("aws".to_string()),
            instance_id: Some("i-abc123".to_string()),
            region: Some("us-east-1".to_string()),
        }
    }

    #[test]
    fn test_remote_host_builds_from_config() {
        let cfg = make_server_config("10.0.0.1");
        let host = RemoteHost::new("web1", &cfg);
        assert_eq!(host.name, "web1");
        assert_eq!(host.ssh.host, "10.0.0.1");
        assert_eq!(host.ssh.user, "ubuntu");
        assert_eq!(host.ssh.port, 22);
    }

    #[test]
    fn test_server_status_offline_defaults() {
        let status = ServerStatus {
            name: "web1".to_string(),
            online: false,
            mhost_installed: false,
            process_count: 0,
            cpu: None,
            memory: None,
        };
        assert!(!status.online);
        assert!(!status.mhost_installed);
        assert_eq!(status.process_count, 0);
        assert!(status.cpu.is_none());
        assert!(status.memory.is_none());
    }

    #[test]
    fn test_server_status_online_with_metrics() {
        let status = ServerStatus {
            name: "db1".to_string(),
            online: true,
            mhost_installed: true,
            process_count: 3,
            cpu: Some(12.5),
            memory: Some(2048.0),
        };
        assert!(status.online);
        assert!(status.mhost_installed);
        assert_eq!(status.process_count, 3);
        assert_eq!(status.cpu, Some(12.5));
        assert_eq!(status.memory, Some(2048.0));
    }

    #[test]
    fn test_server_status_serializes_to_json() {
        let status = ServerStatus {
            name: "web1".to_string(),
            online: true,
            mhost_installed: true,
            process_count: 2,
            cpu: Some(5.0),
            memory: Some(512.0),
        };
        let json = serde_json::to_string(&status).expect("serialization failed");
        assert!(json.contains("\"online\":true"));
        assert!(json.contains("\"process_count\":2"));
    }

    #[test]
    fn test_remote_host_name_stored() {
        let cfg = make_server_config("192.168.1.100");
        let host = RemoteHost::new("my-server", &cfg);
        assert_eq!(host.name, "my-server");
    }
}
