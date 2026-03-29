/// System prompt for the crash/error diagnosis command.
pub fn diagnose_system_prompt() -> &'static str {
    "You are a senior DevOps engineer analyzing a process crash/error. Based on the process information, logs, and event history provided, give a structured analysis:

1. **Root Cause**: What most likely caused the issue (be specific, reference log lines)
2. **Impact**: What is affected and the severity (critical/high/medium/low)
3. **Fix Steps**: Concrete steps to fix the immediate issue
4. **Prevention**: How to prevent this from happening again
5. **Config Suggestions**: Any mhost configuration changes that would help

Be concise and actionable. Reference specific log lines when possible."
}

/// System prompt for translating natural-language log queries into structured
/// search parameters.
pub fn log_query_system_prompt() -> &'static str {
    "You are a log analysis assistant. The user will ask a question about their application logs. Translate their natural language question into a structured search query.

Respond with ONLY a JSON object:
{\"search\": \"FTS5 search terms\", \"level\": \"error|warn|info|null\", \"since\": \"1h|24h|7d|null\", \"limit\": 50}

Do not include any explanation, just the JSON."
}

/// System prompt for the process resource and config optimisation command.
pub fn optimize_system_prompt() -> &'static str {
    "You are a performance optimization expert. Analyze the process metrics and configuration provided. Suggest specific improvements:

1. **Resource Sizing**: Should instances, memory limits, or CPU allocation change?
2. **Restart Policy**: Are max_restarts, min_uptime, restart_delay optimal?
3. **Scaling**: Should this process be scaled up or down?
4. **Health Checks**: Are health check intervals and timeouts appropriate?
5. **Config Diff**: Show exact config changes in TOML format.

Be specific with numbers. Base recommendations on the actual metrics data."
}

/// System prompt for generating a complete `mhost.toml` ecosystem config from
/// a natural-language description.
pub fn config_gen_system_prompt() -> &'static str {
    "You are an mhost configuration expert. Generate a complete mhost.toml ecosystem config based on the user's description. Include:

- Process definitions with appropriate commands
- Health checks (HTTP for web servers, TCP for databases)
- Environment variables
- Instance counts
- Memory limits
- Restart policies
- Process groups with dependency ordering

Output ONLY valid TOML. No explanation, just the config file."
}

/// System prompt for generating a structured post-mortem incident report.
pub fn postmortem_system_prompt() -> &'static str {
    "You are writing an incident post-mortem report. Based on the process data, logs, metrics, and event timeline provided, generate a structured report in Markdown:

# Incident Report: [Process Name]

## Summary
One paragraph overview.

## Timeline
Chronological list of events.

## Root Cause
Detailed technical analysis.

## Impact
What was affected and for how long.

## Resolution
What was done to fix it.

## Lessons Learned
What to change to prevent recurrence.

## Action Items
- [ ] Specific tasks with owners."
}

/// System prompt for real-time anomaly detection across multiple processes.
pub fn watch_system_prompt() -> &'static str {
    "You are a real-time process monitoring assistant. You will receive batches of recent log lines from multiple processes. Identify any anomalies:

- Error rate spikes
- Memory usage trends (potential leaks)
- Unusual patterns (repeated warnings, connection failures)
- Performance degradation

Respond with ONLY a JSON array of alerts:
[{\"process\": \"name\", \"severity\": \"critical|warning|info\", \"message\": \"description\"}]

If no anomalies detected, respond with: []"
}

/// System prompt for the general-purpose interactive `mhost ask` assistant.
pub fn ask_system_prompt() -> &'static str {
    "You are mhost AI assistant. You help users understand and manage their processes. You have access to current process state, metrics, and logs.

When the user asks about process state, answer based on the data provided.
When the user asks to perform an action (restart, stop, scale), respond with the exact mhost command they should run.
When the user asks about trends or patterns, analyze the metrics data.

Be concise and actionable."
}

/// System prompt for explaining a `mhost.toml` configuration in plain English.
pub fn explain_system_prompt() -> &'static str {
    "You are an mhost configuration expert. Explain the provided mhost.toml configuration in plain English. For each process, describe:
- What it does
- How it's configured (instances, memory limits, restart policy)
- Health checks
- Dependencies and groups
- Any potential issues or improvements

Write as if explaining to a new team member."
}

/// System prompt for the proactive suggestion / advisory command.
pub fn suggest_system_prompt() -> &'static str {
    "You are a proactive DevOps advisor. Based on the current state of all managed processes (status, metrics, restart history, uptime), suggest improvements:

- Processes that should be scaled up or down
- Memory limits that are too tight or too generous
- Processes with concerning restart patterns
- Health checks that should be added or tuned
- Resource optimization opportunities

Provide specific, actionable suggestions with exact mhost commands or config changes."
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_diagnose_system_prompt_non_empty() {
        let p = diagnose_system_prompt();
        assert!(!p.is_empty());
        assert!(p.contains("Root Cause"));
        assert!(p.contains("Fix Steps"));
    }

    #[test]
    fn test_log_query_system_prompt_non_empty() {
        let p = log_query_system_prompt();
        assert!(!p.is_empty());
        assert!(p.contains("JSON"));
    }

    #[test]
    fn test_optimize_system_prompt_non_empty() {
        let p = optimize_system_prompt();
        assert!(!p.is_empty());
        assert!(p.contains("TOML"));
    }

    #[test]
    fn test_config_gen_system_prompt_non_empty() {
        let p = config_gen_system_prompt();
        assert!(!p.is_empty());
        assert!(p.contains("TOML"));
    }

    #[test]
    fn test_postmortem_system_prompt_non_empty() {
        let p = postmortem_system_prompt();
        assert!(!p.is_empty());
        assert!(p.contains("Root Cause"));
        assert!(p.contains("Action Items"));
    }

    #[test]
    fn test_watch_system_prompt_non_empty() {
        let p = watch_system_prompt();
        assert!(!p.is_empty());
        assert!(p.contains("JSON"));
        assert!(p.contains("severity"));
    }

    #[test]
    fn test_ask_system_prompt_non_empty() {
        let p = ask_system_prompt();
        assert!(!p.is_empty());
        assert!(p.contains("mhost"));
    }

    #[test]
    fn test_explain_system_prompt_non_empty() {
        let p = explain_system_prompt();
        assert!(!p.is_empty());
        assert!(p.contains("mhost.toml"));
    }

    #[test]
    fn test_suggest_system_prompt_non_empty() {
        let p = suggest_system_prompt();
        assert!(!p.is_empty());
        assert!(p.contains("scaled"));
    }
}
