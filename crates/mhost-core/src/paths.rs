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

    pub fn root(&self) -> &PathBuf { &self.root }
    pub fn db(&self) -> PathBuf { self.root.join("mhost.db") }
    pub fn pid_file(&self) -> PathBuf { self.root.join("mhostd.pid") }
    pub fn socket(&self) -> PathBuf { self.root.join("mhostd.sock") }
    pub fn logs_dir(&self) -> PathBuf { self.root.join("logs") }
    pub fn pids_dir(&self) -> PathBuf { self.root.join("pids") }

    pub fn process_out_log(&self, name: &str, instance: u32) -> PathBuf {
        self.logs_dir().join(format!("{name}-{instance}-out.log"))
    }

    pub fn process_err_log(&self, name: &str, instance: u32) -> PathBuf {
        self.logs_dir().join(format!("{name}-{instance}-err.log"))
    }

    pub fn process_pid(&self, name: &str, instance: u32) -> PathBuf {
        self.pids_dir().join(format!("{name}-{instance}.pid"))
    }

    pub fn daemon_log(&self) -> PathBuf { self.logs_dir().join("daemon.log") }
    pub fn dump_file(&self) -> PathBuf { self.root.join("dump.json") }

    pub fn ensure_dirs(&self) -> std::io::Result<()> {
        std::fs::create_dir_all(&self.root)?;
        std::fs::create_dir_all(self.logs_dir())?;
        std::fs::create_dir_all(self.pids_dir())?;
        Ok(())
    }
}

impl Default for MhostPaths {
    fn default() -> Self { Self::new() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_paths_with_custom_root() {
        let paths = MhostPaths::with_root(PathBuf::from("/tmp/mhost-test"));
        assert_eq!(paths.root(), &PathBuf::from("/tmp/mhost-test"));
        assert_eq!(paths.db(), PathBuf::from("/tmp/mhost-test/mhost.db"));
        assert_eq!(paths.pid_file(), PathBuf::from("/tmp/mhost-test/mhostd.pid"));
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
}
