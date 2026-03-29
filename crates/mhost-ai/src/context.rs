use mhost_core::process::{ProcessConfig, ProcessInfo};
use serde::Serialize;

/// All relevant data about a managed process gathered for LLM analysis.
#[derive(Debug, Clone, Serialize)]
pub struct ProcessContext {
    pub name: String,
    pub config: ProcessConfig,
    pub status: String,
    pub pid: Option<u32>,
    pub restarts: u32,
    pub uptime: String,
    pub exit_code: Option<i32>,
    /// Last 50 lines from the process stdout/stderr log file.
    pub recent_logs: Vec<String>,
    /// Last 20 error-level lines from the log file.
    pub error_logs: Vec<String>,
    /// Recent events pulled from the event history database.
    pub events: Vec<String>,
    pub cpu_percent: Option<f32>,
    pub memory_mb: Option<f64>,
}

impl ProcessContext {
    /// Build a [`ProcessContext`] from a [`ProcessInfo`] plus pre-loaded log
    /// and event data.
    ///
    /// `logs` should be the last 50 lines of the process log.
    /// `error_logs` should be the last 20 error-level lines.
    /// `events` should be recent human-readable event strings.
    pub fn from_process_info(
        info: &ProcessInfo,
        logs: Vec<String>,
        error_logs: Vec<String>,
        events: Vec<String>,
    ) -> Self {
        Self {
            name: info.config.name.clone(),
            config: info.config.clone(),
            status: info.status.to_string(),
            pid: info.pid,
            restarts: info.restart_count,
            uptime: info.format_uptime(),
            exit_code: info.exit_code,
            recent_logs: logs,
            error_logs,
            events,
            cpu_percent: info.cpu_percent,
            memory_mb: info.memory_bytes.map(|b| b as f64 / 1_048_576.0),
        }
    }

    /// Render the context as a structured text block suitable for inclusion in
    /// an LLM prompt.
    pub fn to_prompt_text(&self) -> String {
        let mut text = String::new();

        text.push_str(&format!("## Process: {}\n", self.name));
        text.push_str(&format!("Status: {}\n", self.status));
        text.push_str(&format!("PID: {:?}\n", self.pid));
        text.push_str(&format!("Restarts: {}\n", self.restarts));
        text.push_str(&format!("Uptime: {}\n", self.uptime));
        text.push_str(&format!(
            "Command: {} {}\n",
            self.config.command,
            self.config.args.join(" ")
        ));

        if let Some(exit) = self.exit_code {
            text.push_str(&format!("Exit Code: {exit}\n"));
        }
        if let Some(cpu) = self.cpu_percent {
            text.push_str(&format!("CPU: {cpu:.1}%\n"));
        }
        if let Some(mem) = self.memory_mb {
            text.push_str(&format!("Memory: {mem:.1} MB\n"));
        }

        if !self.recent_logs.is_empty() {
            text.push_str("\n### Recent Logs (last 50 lines)\n```\n");
            for line in &self.recent_logs {
                text.push_str(line);
                text.push('\n');
            }
            text.push_str("```\n");
        }

        if !self.error_logs.is_empty() {
            text.push_str("\n### Error Logs\n```\n");
            for line in &self.error_logs {
                text.push_str(line);
                text.push('\n');
            }
            text.push_str("```\n");
        }

        if !self.events.is_empty() {
            text.push_str("\n### Event History\n");
            for event in &self.events {
                text.push_str(&format!("- {event}\n"));
            }
        }

        text
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use mhost_core::process::{ProcessConfig, ProcessInfo, ProcessStatus};

    fn make_info_online() -> ProcessInfo {
        let config = ProcessConfig {
            name: "api-server".to_string(),
            command: "node".to_string(),
            args: vec!["server.js".to_string(), "--port=3000".to_string()],
            ..Default::default()
        };
        let mut info = ProcessInfo::new(config, 0);
        // Manually set fields that would be set by the daemon at runtime.
        info.status = ProcessStatus::Errored;
        info.pid = Some(12345);
        info.restart_count = 3;
        info.exit_code = Some(1);
        info.cpu_percent = Some(45.5);
        info.memory_bytes = Some(256 * 1_048_576); // 256 MB
        info
    }

    // -----------------------------------------------------------------------
    // from_process_info builds all fields correctly
    // -----------------------------------------------------------------------

    #[test]
    fn test_from_process_info_basic_fields() {
        let info = make_info_online();
        let ctx = ProcessContext::from_process_info(
            &info,
            vec!["log line 1".into(), "log line 2".into()],
            vec!["ERROR: connection refused".into()],
            vec!["process started".into(), "process crashed".into()],
        );

        assert_eq!(ctx.name, "api-server");
        assert_eq!(ctx.status, "errored");
        assert_eq!(ctx.pid, Some(12345));
        assert_eq!(ctx.restarts, 3);
        assert_eq!(ctx.exit_code, Some(1));
        assert_eq!(ctx.cpu_percent, Some(45.5));
        assert!(ctx.memory_mb.is_some());
        // 256 MB in bytes → 256.0 MB
        assert!((ctx.memory_mb.unwrap() - 256.0).abs() < 0.01);
        assert_eq!(ctx.recent_logs, vec!["log line 1", "log line 2"]);
        assert_eq!(ctx.error_logs, vec!["ERROR: connection refused"]);
        assert_eq!(ctx.events, vec!["process started", "process crashed"]);
    }

    #[test]
    fn test_from_process_info_config_cloned() {
        let info = make_info_online();
        let ctx = ProcessContext::from_process_info(&info, vec![], vec![], vec![]);

        assert_eq!(ctx.config.command, "node");
        assert_eq!(ctx.config.args, vec!["server.js", "--port=3000"]);
    }

    #[test]
    fn test_from_process_info_no_pid_no_memory() {
        let config = ProcessConfig {
            name: "worker".to_string(),
            command: "python".to_string(),
            args: vec!["worker.py".to_string()],
            ..Default::default()
        };
        let info = ProcessInfo::new(config, 0);
        let ctx = ProcessContext::from_process_info(&info, vec![], vec![], vec![]);

        assert!(ctx.pid.is_none());
        assert!(ctx.memory_mb.is_none());
        assert!(ctx.cpu_percent.is_none());
        assert!(ctx.exit_code.is_none());
    }

    // -----------------------------------------------------------------------
    // to_prompt_text renders all populated sections
    // -----------------------------------------------------------------------

    #[test]
    fn test_to_prompt_text_includes_header() {
        let info = make_info_online();
        let ctx = ProcessContext::from_process_info(&info, vec![], vec![], vec![]);
        let text = ctx.to_prompt_text();

        assert!(text.contains("## Process: api-server"));
        assert!(text.contains("Status: errored"));
        assert!(text.contains("PID: Some(12345)"));
        assert!(text.contains("Restarts: 3"));
        assert!(text.contains("Command: node server.js --port=3000"));
        assert!(text.contains("Exit Code: 1"));
        assert!(text.contains("CPU: 45.5%"));
        assert!(text.contains("Memory: 256.0 MB"));
    }

    #[test]
    fn test_to_prompt_text_includes_recent_logs_section() {
        let info = make_info_online();
        let logs = vec!["INFO startup".into(), "WARN slow query".into()];
        let ctx = ProcessContext::from_process_info(&info, logs, vec![], vec![]);
        let text = ctx.to_prompt_text();

        assert!(text.contains("### Recent Logs (last 50 lines)"));
        assert!(text.contains("INFO startup"));
        assert!(text.contains("WARN slow query"));
    }

    #[test]
    fn test_to_prompt_text_includes_error_logs_section() {
        let info = make_info_online();
        let error_logs = vec!["ERROR: ECONNREFUSED 127.0.0.1:5432".into()];
        let ctx = ProcessContext::from_process_info(&info, vec![], error_logs, vec![]);
        let text = ctx.to_prompt_text();

        assert!(text.contains("### Error Logs"));
        assert!(text.contains("ERROR: ECONNREFUSED 127.0.0.1:5432"));
    }

    #[test]
    fn test_to_prompt_text_includes_event_history_section() {
        let info = make_info_online();
        let events = vec![
            "2026-03-28T10:00:00Z process started".into(),
            "2026-03-28T10:05:00Z process exited with code 1".into(),
        ];
        let ctx = ProcessContext::from_process_info(&info, vec![], vec![], events);
        let text = ctx.to_prompt_text();

        assert!(text.contains("### Event History"));
        assert!(text.contains("- 2026-03-28T10:00:00Z process started"));
        assert!(text.contains("- 2026-03-28T10:05:00Z process exited with code 1"));
    }

    // -----------------------------------------------------------------------
    // to_prompt_text handles empty optional sections gracefully
    // -----------------------------------------------------------------------

    #[test]
    fn test_to_prompt_text_empty_logs_omit_sections() {
        let info = make_info_online();
        let ctx = ProcessContext::from_process_info(&info, vec![], vec![], vec![]);
        let text = ctx.to_prompt_text();

        assert!(!text.contains("### Recent Logs"));
        assert!(!text.contains("### Error Logs"));
        assert!(!text.contains("### Event History"));
    }

    #[test]
    fn test_to_prompt_text_no_exit_code_omits_line() {
        let config = ProcessConfig {
            name: "healthy-proc".to_string(),
            command: "./bin/serve".to_string(),
            args: vec![],
            ..Default::default()
        };
        let info = ProcessInfo::new(config, 0);
        let ctx = ProcessContext::from_process_info(&info, vec![], vec![], vec![]);
        let text = ctx.to_prompt_text();

        assert!(!text.contains("Exit Code:"));
        assert!(!text.contains("CPU:"));
        assert!(!text.contains("Memory:"));
    }
}
