use std::path::Path;
use std::time::Duration;
use tokio::process::Command;

pub struct HookRunner;

impl HookRunner {
    pub async fn run(commands: &[String], cwd: &Path, timeout: Duration) -> Result<(), String> {
        for cmd in commands {
            tracing::info!(command = %cmd, "Running deploy hook");
            let parts: Vec<&str> = cmd.split_whitespace().collect();
            if parts.is_empty() {
                continue;
            }

            let result = tokio::time::timeout(
                timeout,
                Command::new(parts[0])
                    .args(&parts[1..])
                    .current_dir(cwd)
                    .status(),
            )
            .await;

            match result {
                Ok(Ok(status)) if status.success() => {}
                Ok(Ok(status)) => {
                    return Err(format!(
                        "Hook '{}' failed with exit code {:?}",
                        cmd,
                        status.code()
                    ))
                }
                Ok(Err(e)) => return Err(format!("Hook '{}' failed to execute: {}", cmd, e)),
                Err(_) => return Err(format!("Hook '{}' timed out after {:?}", cmd, timeout)),
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn echo_hook_succeeds() {
        let tmp = TempDir::new().unwrap();
        let cmds = vec!["echo hello".to_string()];
        let result = HookRunner::run(&cmds, tmp.path(), Duration::from_secs(10)).await;
        assert!(result.is_ok(), "echo hello should succeed: {:?}", result);
    }

    #[tokio::test]
    async fn false_hook_fails() {
        let tmp = TempDir::new().unwrap();
        let cmds = vec!["false".to_string()];
        let result = HookRunner::run(&cmds, tmp.path(), Duration::from_secs(10)).await;
        assert!(result.is_err(), "false should fail");
        assert!(result.unwrap_err().contains("exit code"));
    }

    #[tokio::test]
    async fn empty_commands_succeeds() {
        let tmp = TempDir::new().unwrap();
        let cmds: Vec<String> = vec![];
        let result = HookRunner::run(&cmds, tmp.path(), Duration::from_secs(10)).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn multiple_hooks_all_succeed() {
        let tmp = TempDir::new().unwrap();
        let cmds = vec!["echo step1".to_string(), "echo step2".to_string()];
        let result = HookRunner::run(&cmds, tmp.path(), Duration::from_secs(10)).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn hooks_stop_on_first_failure() {
        let tmp = TempDir::new().unwrap();
        // "false" will fail, next echo should never run
        let cmds = vec!["false".to_string(), "echo should_not_run".to_string()];
        let result = HookRunner::run(&cmds, tmp.path(), Duration::from_secs(10)).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn middle_command_failure_stops_chain() {
        let tmp = TempDir::new().unwrap();
        // echo ok — false (fails) — echo never_reached
        let cmds = vec![
            "echo step_one".to_string(),
            "false".to_string(),
            "echo step_three".to_string(),
        ];
        let result = HookRunner::run(&cmds, tmp.path(), Duration::from_secs(10)).await;
        assert!(result.is_err(), "middle failure should abort the chain");
        let err = result.unwrap_err();
        assert!(
            err.contains("exit code"),
            "error should mention exit code, got: {err}"
        );
    }

    #[tokio::test]
    async fn hook_timeout_returns_error() {
        let tmp = TempDir::new().unwrap();
        // sleep for 10 seconds but timeout is 100 ms
        let cmds = vec!["sleep 10".to_string()];
        let result = HookRunner::run(&cmds, tmp.path(), Duration::from_millis(100)).await;
        assert!(result.is_err(), "timed-out hook should return an error");
        let err = result.unwrap_err();
        assert!(
            err.contains("timed out"),
            "error should mention timeout, got: {err}"
        );
    }

    #[tokio::test]
    async fn nonexistent_command_returns_error() {
        let tmp = TempDir::new().unwrap();
        let cmds = vec!["__nonexistent_binary_xyz__".to_string()];
        let result = HookRunner::run(&cmds, tmp.path(), Duration::from_secs(5)).await;
        assert!(result.is_err(), "nonexistent binary should return an error");
    }

    #[tokio::test]
    async fn whitespace_only_command_is_skipped() {
        let tmp = TempDir::new().unwrap();
        // A command that is empty after split_whitespace should be skipped.
        let cmds = vec!["   ".to_string(), "echo ok".to_string()];
        let result = HookRunner::run(&cmds, tmp.path(), Duration::from_secs(5)).await;
        assert!(result.is_ok(), "whitespace-only command should be skipped");
    }
}
