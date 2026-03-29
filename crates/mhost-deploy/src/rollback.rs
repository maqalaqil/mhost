use crate::git::GitOps;
use crate::history::DeployHistory;
use crate::hooks::HookRunner;
use std::path::Path;
use std::time::Duration;

pub struct Rollback;

impl Rollback {
    pub async fn execute(
        env: &str,
        deploy_path: &Path,
        history: &DeployHistory,
        post_hooks: &[String],
    ) -> Result<String, String> {
        let prev = history
            .last_successful(env)
            .ok_or_else(|| format!("No successful deploy found for '{env}'"))?;

        GitOps::checkout(deploy_path, &prev.commit_hash)?;

        HookRunner::run(post_hooks, deploy_path, Duration::from_secs(300)).await?;

        history.record(env, &prev.commit_hash, "success", Some("rollback"));

        Ok(prev.commit_hash)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command as StdCommand;
    use tempfile::TempDir;

    fn init_git_repo_with_commit(dir: &Path) -> String {
        StdCommand::new("git")
            .args(["init"])
            .current_dir(dir)
            .output()
            .expect("git init");
        StdCommand::new("git")
            .args(["config", "user.email", "test@example.com"])
            .current_dir(dir)
            .output()
            .expect("git config email");
        StdCommand::new("git")
            .args(["config", "user.name", "Test User"])
            .current_dir(dir)
            .output()
            .expect("git config name");
        std::fs::write(dir.join("README.md"), "# test").expect("write file");
        StdCommand::new("git")
            .args(["add", "."])
            .current_dir(dir)
            .output()
            .expect("git add");
        StdCommand::new("git")
            .args(["commit", "-m", "initial commit"])
            .current_dir(dir)
            .output()
            .expect("git commit");
        GitOps::current_commit(dir).expect("get commit hash")
    }

    #[tokio::test]
    async fn rollback_fails_when_no_successful_deploys() {
        let tmp = TempDir::new().unwrap();
        init_git_repo_with_commit(tmp.path());
        let history = DeployHistory::in_memory().unwrap();

        let result = Rollback::execute("production", tmp.path(), &history, &[]).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("No successful deploy found"));
    }

    #[tokio::test]
    async fn rollback_succeeds_with_valid_history() {
        let tmp = TempDir::new().unwrap();
        let commit_hash = init_git_repo_with_commit(tmp.path());
        let history = DeployHistory::in_memory().unwrap();

        // Record a successful deploy with the current commit hash
        history.record("production", &commit_hash, "success", None);

        let result = Rollback::execute("production", tmp.path(), &history, &[]).await;
        assert!(result.is_ok(), "rollback should succeed: {result:?}");
        assert_eq!(result.unwrap(), commit_hash);

        // Verify rollback was recorded
        let records = history.list("production", 10);
        assert_eq!(records.len(), 2);
        assert_eq!(records[0].message.as_deref(), Some("rollback"));
    }
}
