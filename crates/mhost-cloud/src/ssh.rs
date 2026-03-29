use crate::config::ServerConfig;
use std::path::Path;
use tokio::process::Command;

#[derive(Debug, Clone)]
pub struct SshExecutor {
    pub host: String,
    pub port: u16,
    pub user: String,
    pub key_path: Option<String>,
}

#[derive(Debug, Clone)]
pub struct SshOutput {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
}

impl SshExecutor {
    /// Build an executor from a `ServerConfig`.
    pub fn from_server_config(config: &ServerConfig) -> Self {
        Self {
            host: config.host.clone(),
            port: config.port,
            user: config.user.clone(),
            key_path: config.key_path.clone(),
        }
    }

    /// Common SSH CLI arguments (without the destination).
    fn ssh_args(&self) -> Vec<String> {
        let mut args = vec![
            "-o".into(),
            "StrictHostKeyChecking=accept-new".into(),
            "-o".into(),
            "ConnectTimeout=10".into(),
            "-o".into(),
            "BatchMode=yes".into(),
            "-p".into(),
            self.port.to_string(),
        ];
        if let Some(ref key) = self.key_path {
            args.push("-i".into());
            args.push(shellexpand_home(key));
        }
        args
    }

    /// Execute a remote shell command and return its output.
    pub async fn exec(&self, command: &str) -> Result<SshOutput, String> {
        let target = format!("{}@{}", self.user, self.host);
        let mut args = self.ssh_args();
        args.push(target);
        args.push(command.into());

        let output = Command::new("ssh")
            .args(&args)
            .output()
            .await
            .map_err(|e| format!("SSH process spawn failed: {e}"))?;

        Ok(SshOutput {
            stdout: String::from_utf8_lossy(&output.stdout).to_string(),
            stderr: String::from_utf8_lossy(&output.stderr).to_string(),
            exit_code: output.status.code().unwrap_or(-1),
        })
    }

    /// Execute a remote `mhost` sub-command and return stdout on success.
    pub async fn exec_mhost(&self, args: &[&str]) -> Result<String, String> {
        let cmd = format!("mhost {}", args.join(" "));
        let output = self.exec(&cmd).await?;
        if output.exit_code != 0 {
            Err(format!(
                "Remote mhost failed (exit {}): {}",
                output.exit_code,
                output.stderr.trim()
            ))
        } else {
            Ok(output.stdout)
        }
    }

    /// Upload a local file to a remote destination via `scp`.
    pub async fn upload(&self, local: &Path, remote: &str) -> Result<(), String> {
        let target = format!("{}@{}:{}", self.user, self.host, remote);
        let mut args = vec![
            "-o".into(),
            "StrictHostKeyChecking=accept-new".into(),
            "-P".into(),
            self.port.to_string(),
        ];
        if let Some(ref key) = self.key_path {
            args.push("-i".into());
            args.push(shellexpand_home(key));
        }
        args.push(local.to_string_lossy().to_string());
        args.push(target);

        let output = Command::new("scp")
            .args(&args)
            .output()
            .await
            .map_err(|e| format!("SCP process spawn failed: {e}"))?;

        if output.status.success() {
            Ok(())
        } else {
            Err(format!(
                "SCP upload failed: {}",
                String::from_utf8_lossy(&output.stderr)
            ))
        }
    }

    /// Download a remote file to a local path via `scp`.
    pub async fn download(&self, remote: &str, local: &Path) -> Result<(), String> {
        let source = format!("{}@{}:{}", self.user, self.host, remote);
        let mut args = vec![
            "-o".into(),
            "StrictHostKeyChecking=accept-new".into(),
            "-P".into(),
            self.port.to_string(),
        ];
        if let Some(ref key) = self.key_path {
            args.push("-i".into());
            args.push(shellexpand_home(key));
        }
        args.push(source);
        args.push(local.to_string_lossy().to_string());

        let output = Command::new("scp")
            .args(&args)
            .output()
            .await
            .map_err(|e| format!("SCP process spawn failed: {e}"))?;

        if output.status.success() {
            Ok(())
        } else {
            Err(format!(
                "SCP download failed: {}",
                String::from_utf8_lossy(&output.stderr)
            ))
        }
    }

    /// Check whether the remote host is reachable via SSH.
    pub async fn is_reachable(&self) -> bool {
        self.exec("echo ok")
            .await
            .map(|o| o.exit_code == 0)
            .unwrap_or(false)
    }

    /// Check whether `mhost` is installed on the remote host.
    pub async fn is_mhost_installed(&self) -> bool {
        self.exec("mhost --version")
            .await
            .map(|o| o.exit_code == 0)
            .unwrap_or(false)
    }
}

/// Expand a leading `~/` to the user's home directory.
pub fn shellexpand_home(path: &str) -> String {
    if let Some(rest) = path.strip_prefix("~/") {
        if let Some(home) = dirs::home_dir() {
            return format!("{}/{}", home.display(), rest);
        }
    }
    path.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{AuthMethod, ServerConfig};

    fn make_config(host: &str, key: Option<&str>) -> ServerConfig {
        ServerConfig {
            host: host.to_string(),
            port: 2222,
            user: "deploy".to_string(),
            auth: AuthMethod::Key,
            key_path: key.map(str::to_string),
            tags: vec![],
            provider: None,
            instance_id: None,
            region: None,
        }
    }

    #[test]
    fn test_from_server_config() {
        let cfg = make_config("192.168.1.1", Some("~/.ssh/deploy_key"));
        let exec = SshExecutor::from_server_config(&cfg);
        assert_eq!(exec.host, "192.168.1.1");
        assert_eq!(exec.port, 2222);
        assert_eq!(exec.user, "deploy");
        assert_eq!(exec.key_path.as_deref(), Some("~/.ssh/deploy_key"));
    }

    #[test]
    fn test_ssh_args_without_key() {
        let cfg = make_config("10.0.0.1", None);
        let exec = SshExecutor::from_server_config(&cfg);
        let args = exec.ssh_args();
        assert!(args.contains(&"StrictHostKeyChecking=accept-new".to_string()));
        assert!(args.contains(&"ConnectTimeout=10".to_string()));
        assert!(args.contains(&"BatchMode=yes".to_string()));
        assert!(args.contains(&"2222".to_string()));
        assert!(!args.contains(&"-i".to_string()));
    }

    #[test]
    fn test_ssh_args_with_key() {
        let cfg = make_config("10.0.0.1", Some("/home/user/.ssh/id_ed25519"));
        let exec = SshExecutor::from_server_config(&cfg);
        let args = exec.ssh_args();
        let i_pos = args
            .iter()
            .position(|a| a == "-i")
            .expect("-i flag missing");
        assert_eq!(args[i_pos + 1], "/home/user/.ssh/id_ed25519");
    }

    #[test]
    fn test_ssh_args_with_tilde_key() {
        let cfg = make_config("10.0.0.1", Some("~/.ssh/mykey"));
        let exec = SshExecutor::from_server_config(&cfg);
        let args = exec.ssh_args();
        let i_pos = args
            .iter()
            .position(|a| a == "-i")
            .expect("-i flag missing");
        // Should be expanded, not literally "~/.ssh/mykey"
        assert!(!args[i_pos + 1].starts_with("~/"));
    }

    #[test]
    fn test_shellexpand_home_tilde() {
        let expanded = shellexpand_home("~/.ssh/id_rsa");
        assert!(
            !expanded.starts_with("~/"),
            "tilde should be expanded: {expanded}"
        );
        assert!(
            expanded.ends_with("/.ssh/id_rsa"),
            "suffix should remain: {expanded}"
        );
    }

    #[test]
    fn test_shellexpand_home_no_tilde() {
        let path = "/absolute/path/key";
        assert_eq!(shellexpand_home(path), path);
    }

    #[test]
    fn test_shellexpand_home_tilde_only() {
        // "~" alone should not be expanded (only "~/...")
        let path = "~";
        assert_eq!(shellexpand_home(path), path);
    }
}
