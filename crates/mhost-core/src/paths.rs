use std::path::PathBuf;

#[derive(Clone)]
pub struct MhostPaths {
    root: PathBuf,
}

impl MhostPaths {
    pub fn new() -> Self {
        let root = dirs::home_dir()
            .expect("Could not determine home directory")
            .join(".mhost");
        Self { root }
    }

    pub fn with_root(root: PathBuf) -> Self {
        Self { root }
    }

    pub fn root(&self) -> &PathBuf {
        &self.root
    }
    pub fn db(&self) -> PathBuf {
        self.root.join("mhost.db")
    }
    pub fn pid_file(&self) -> PathBuf {
        self.root.join("mhostd.pid")
    }
    pub fn socket(&self) -> PathBuf {
        self.root.join("mhostd.sock")
    }
    pub fn logs_dir(&self) -> PathBuf {
        self.root.join("logs")
    }
    pub fn pids_dir(&self) -> PathBuf {
        self.root.join("pids")
    }

    pub fn process_out_log(&self, name: &str, instance: u32) -> PathBuf {
        self.logs_dir().join(format!("{name}-{instance}-out.log"))
    }

    pub fn process_err_log(&self, name: &str, instance: u32) -> PathBuf {
        self.logs_dir().join(format!("{name}-{instance}-err.log"))
    }

    pub fn process_pid(&self, name: &str, instance: u32) -> PathBuf {
        self.pids_dir().join(format!("{name}-{instance}.pid"))
    }

    pub fn daemon_log(&self) -> PathBuf {
        self.logs_dir().join("daemon.log")
    }
    pub fn dump_file(&self) -> PathBuf {
        self.root.join("dump.json")
    }
    pub fn notify_config(&self) -> PathBuf {
        self.root.join("notify.json")
    }
    pub fn ai_config(&self) -> PathBuf {
        self.root.join("ai.json")
    }
    pub fn fleet_config(&self) -> PathBuf {
        self.root.join("fleet.json")
    }
    pub fn bot_config(&self) -> PathBuf {
        self.root.join("bot.json")
    }
    pub fn api_tokens(&self) -> PathBuf {
        self.root.join("api_tokens.json")
    }
    pub fn webhooks_config(&self) -> PathBuf {
        self.root.join("webhooks.json")
    }
    pub fn webhook_failures(&self) -> PathBuf {
        self.root.join("webhook_failures.json")
    }

    pub fn cloud_credentials(&self) -> PathBuf {
        self.root.join("cloud-credentials.json")
    }

    pub fn cloud_state(&self) -> PathBuf {
        self.root.join("cloud-state.toml")
    }

    pub fn cloud_backups(&self) -> PathBuf {
        self.root.join("cloud-backups")
    }

    pub fn cloud_cost_cache(&self) -> PathBuf {
        self.root.join("cloud-cost-cache.json")
    }

    pub fn ensure_dirs(&self) -> std::io::Result<()> {
        std::fs::create_dir_all(&self.root)?;
        std::fs::create_dir_all(self.logs_dir())?;
        std::fs::create_dir_all(self.pids_dir())?;
        Ok(())
    }
}

impl Default for MhostPaths {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_paths_with_custom_root() {
        let paths = MhostPaths::with_root(PathBuf::from("/tmp/mhost-test"));
        assert_eq!(paths.root(), &PathBuf::from("/tmp/mhost-test"));
        assert_eq!(paths.db(), PathBuf::from("/tmp/mhost-test/mhost.db"));
        assert_eq!(
            paths.pid_file(),
            PathBuf::from("/tmp/mhost-test/mhostd.pid")
        );
        assert_eq!(paths.socket(), PathBuf::from("/tmp/mhost-test/mhostd.sock"));
    }

    #[test]
    fn test_process_log_paths() {
        let paths = MhostPaths::with_root(PathBuf::from("/tmp/mhost-test"));
        assert_eq!(
            paths.process_out_log("api", 0),
            PathBuf::from("/tmp/mhost-test/logs/api-0-out.log")
        );
        assert_eq!(
            paths.process_err_log("api", 2),
            PathBuf::from("/tmp/mhost-test/logs/api-2-err.log")
        );
    }

    #[test]
    fn test_process_pid_path() {
        let paths = MhostPaths::with_root(PathBuf::from("/tmp/mhost-test"));
        assert_eq!(
            paths.process_pid("worker", 1),
            PathBuf::from("/tmp/mhost-test/pids/worker-1.pid")
        );
    }

    #[test]
    fn test_ensure_dirs() {
        let tmp = std::env::temp_dir().join("mhost-test-dirs");
        let _ = std::fs::remove_dir_all(&tmp);
        let paths = MhostPaths::with_root(tmp.clone());
        paths.ensure_dirs().unwrap();
        assert!(paths.root().exists());
        assert!(paths.logs_dir().exists());
        assert!(paths.pids_dir().exists());
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn test_default_root_is_home_mhost() {
        let paths = MhostPaths::new();
        let home = dirs::home_dir().unwrap();
        assert_eq!(paths.root(), &home.join(".mhost"));
    }

    #[test]
    fn test_daemon_log_path() {
        let paths = MhostPaths::with_root(PathBuf::from("/tmp/mhost-test"));
        assert_eq!(
            paths.daemon_log(),
            PathBuf::from("/tmp/mhost-test/logs/daemon.log")
        );
    }

    #[test]
    fn test_dump_file_path() {
        let paths = MhostPaths::with_root(PathBuf::from("/tmp/mhost-test"));
        assert_eq!(
            paths.dump_file(),
            PathBuf::from("/tmp/mhost-test/dump.json")
        );
    }

    #[test]
    fn test_notify_config_path() {
        let paths = MhostPaths::with_root(PathBuf::from("/tmp/mhost-test"));
        assert_eq!(
            paths.notify_config(),
            PathBuf::from("/tmp/mhost-test/notify.json")
        );
    }

    #[test]
    fn test_ai_config_path() {
        let paths = MhostPaths::with_root(PathBuf::from("/tmp/mhost-test"));
        assert_eq!(paths.ai_config(), PathBuf::from("/tmp/mhost-test/ai.json"));
    }

    #[test]
    fn test_bot_config_path() {
        let paths = MhostPaths::with_root(PathBuf::from("/tmp/mhost-test"));
        assert_eq!(
            paths.bot_config(),
            PathBuf::from("/tmp/mhost-test/bot.json")
        );
    }

    #[test]
    fn test_fleet_config_path() {
        let paths = MhostPaths::with_root(PathBuf::from("/tmp/mhost-test"));
        assert_eq!(
            paths.fleet_config(),
            PathBuf::from("/tmp/mhost-test/fleet.json")
        );
    }

    #[test]
    fn test_api_tokens_path() {
        let paths = MhostPaths::with_root(PathBuf::from("/tmp/mhost-test"));
        assert_eq!(
            paths.api_tokens(),
            PathBuf::from("/tmp/mhost-test/api_tokens.json")
        );
    }

    #[test]
    fn test_webhooks_config_path() {
        let paths = MhostPaths::with_root(PathBuf::from("/tmp/mhost-test"));
        assert_eq!(
            paths.webhooks_config(),
            PathBuf::from("/tmp/mhost-test/webhooks.json")
        );
    }

    #[test]
    fn test_webhook_failures_path() {
        let paths = MhostPaths::with_root(PathBuf::from("/tmp/mhost-test"));
        assert_eq!(
            paths.webhook_failures(),
            PathBuf::from("/tmp/mhost-test/webhook_failures.json")
        );
    }

    #[test]
    fn test_cloud_credentials_path() {
        let paths = MhostPaths::with_root(PathBuf::from("/tmp/mhost-test"));
        assert_eq!(
            paths.cloud_credentials(),
            PathBuf::from("/tmp/mhost-test/cloud-credentials.json")
        );
    }

    #[test]
    fn test_cloud_state_path() {
        let paths = MhostPaths::with_root(PathBuf::from("/tmp/mhost-test"));
        assert_eq!(
            paths.cloud_state(),
            PathBuf::from("/tmp/mhost-test/cloud-state.toml")
        );
    }

    #[test]
    fn test_cloud_backups_path() {
        let paths = MhostPaths::with_root(PathBuf::from("/tmp/mhost-test"));
        assert_eq!(
            paths.cloud_backups(),
            PathBuf::from("/tmp/mhost-test/cloud-backups")
        );
    }

    #[test]
    fn test_cloud_cost_cache_path() {
        let paths = MhostPaths::with_root(PathBuf::from("/tmp/mhost-test"));
        assert_eq!(
            paths.cloud_cost_cache(),
            PathBuf::from("/tmp/mhost-test/cloud-cost-cache.json")
        );
    }

    #[test]
    fn test_paths_clone() {
        let paths = MhostPaths::with_root(PathBuf::from("/tmp/mhost-test"));
        let cloned = paths.clone();
        assert_eq!(paths.root(), cloned.root());
        assert_eq!(paths.db(), cloned.db());
    }
}
