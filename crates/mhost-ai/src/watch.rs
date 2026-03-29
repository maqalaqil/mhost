use crate::prompts;
use crate::provider::{LlmMessage, LlmProvider, LlmRequest};

/// A single anomaly alert returned by the watch/monitoring command.
#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub struct AnomalyAlert {
    /// Name of the affected process.
    pub process: String,
    /// Severity: "critical", "warning", or "info".
    pub severity: String,
    /// Human-readable description of the detected anomaly.
    pub message: String,
}

/// Analyse recent log batches from multiple processes and return any anomalies
/// detected by the LLM.  Returns an empty `Vec` when nothing unusual is found.
///
/// `log_batches` is a slice of `(process_name, recent_log_lines)` pairs.
pub async fn detect_anomalies(
    provider: &dyn LlmProvider,
    log_batches: &[(String, Vec<String>)],
) -> Result<Vec<AnomalyAlert>, String> {
    let formatted = format_log_batches(log_batches);

    let request = LlmRequest {
        messages: vec![
            LlmMessage {
                role: "system".into(),
                content: prompts::watch_system_prompt().into(),
            },
            LlmMessage {
                role: "user".into(),
                content: formatted,
            },
        ],
        max_tokens: 1024,
        temperature: 0.0,
    };

    let response = provider.complete(request).await?;

    // Strip optional markdown fences before parsing JSON.
    let content = response.content.trim();
    let json_str = if content.starts_with("```") {
        content
            .lines()
            .filter(|l| !l.starts_with("```"))
            .collect::<Vec<_>>()
            .join("\n")
    } else {
        content.to_string()
    };

    // An empty array from the LLM means no anomalies were found.
    serde_json::from_str::<Vec<AnomalyAlert>>(&json_str)
        .map_err(|e| format!("Failed to parse LLM response as anomaly alerts: {e}"))
}

/// Format log batches into a prompt-friendly multi-process text block.
fn format_log_batches(batches: &[(String, Vec<String>)]) -> String {
    let mut text = String::new();
    for (name, lines) in batches {
        text.push_str(&format!("### Process: {}\n```\n", name));
        for line in lines {
            text.push_str(line);
            text.push('\n');
        }
        text.push_str("```\n\n");
    }
    text
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider::{LlmRequest, LlmResponse, TokenUsage};
    use async_trait::async_trait;
    use std::sync::{Arc, Mutex};

    struct EchoProvider {
        response: String,
    }

    #[async_trait]
    impl LlmProvider for EchoProvider {
        async fn complete(&self, _request: LlmRequest) -> Result<LlmResponse, String> {
            Ok(LlmResponse {
                content: self.response.clone(),
                model: "mock".into(),
                usage: Some(TokenUsage {
                    input_tokens: 5,
                    output_tokens: 10,
                }),
            })
        }

        fn provider_name(&self) -> &str {
            "echo"
        }

        fn model_name(&self) -> &str {
            "echo-model"
        }
    }

    struct CapturingProvider {
        captured: Arc<Mutex<Option<LlmRequest>>>,
        response: String,
    }

    #[async_trait]
    impl LlmProvider for CapturingProvider {
        async fn complete(&self, request: LlmRequest) -> Result<LlmResponse, String> {
            *self.captured.lock().unwrap() = Some(request);
            Ok(LlmResponse {
                content: self.response.clone(),
                model: "mock".into(),
                usage: None,
            })
        }

        fn provider_name(&self) -> &str {
            "capturing"
        }

        fn model_name(&self) -> &str {
            "capturing-model"
        }
    }

    #[tokio::test]
    async fn test_detect_anomalies_empty_array_means_no_alerts() {
        let provider = EchoProvider {
            response: "[]".to_string(),
        };

        let batches = vec![("api".to_string(), vec!["INFO all ok".to_string()])];
        let result = detect_anomalies(&provider, &batches).await;

        assert!(result.is_ok());
        assert!(result.unwrap().is_empty());
    }

    #[tokio::test]
    async fn test_detect_anomalies_parses_alert_array() {
        let json = r#"[{"process":"api","severity":"critical","message":"OOM detected"}]"#;
        let provider = EchoProvider {
            response: json.to_string(),
        };

        let batches = vec![("api".to_string(), vec!["FATAL out of memory".to_string()])];
        let result = detect_anomalies(&provider, &batches).await;

        assert!(result.is_ok());
        let alerts = result.unwrap();
        assert_eq!(alerts.len(), 1);
        assert_eq!(alerts[0].process, "api");
        assert_eq!(alerts[0].severity, "critical");
        assert_eq!(alerts[0].message, "OOM detected");
    }

    #[tokio::test]
    async fn test_detect_anomalies_strips_markdown_fences() {
        let json = "```json\n[{\"process\":\"db\",\"severity\":\"warning\",\"message\":\"slow queries\"}]\n```";
        let provider = EchoProvider {
            response: json.to_string(),
        };

        let batches = vec![("db".to_string(), vec!["WARN slow query 2s".to_string()])];
        let result = detect_anomalies(&provider, &batches).await;

        assert!(result.is_ok());
        let alerts = result.unwrap();
        assert_eq!(alerts.len(), 1);
        assert_eq!(alerts[0].severity, "warning");
    }

    #[tokio::test]
    async fn test_detect_anomalies_request_contains_formatted_log_batches() {
        let captured = Arc::new(Mutex::new(None));
        let provider = CapturingProvider {
            captured: Arc::clone(&captured),
            response: "[]".into(),
        };

        let batches = vec![
            (
                "web-server".to_string(),
                vec![
                    "ERROR 500 /api/users".to_string(),
                    "INFO GET /health 200".to_string(),
                ],
            ),
            ("worker".to_string(), vec!["INFO job started".to_string()]),
        ];

        detect_anomalies(&provider, &batches).await.unwrap();

        let req = captured.lock().unwrap().take().unwrap();
        assert_eq!(req.messages.len(), 2);
        assert_eq!(req.messages[0].role, "system");
        assert_eq!(req.messages[1].role, "user");
        assert!(req.messages[1].content.contains("web-server"));
        assert!(req.messages[1].content.contains("ERROR 500 /api/users"));
        assert!(req.messages[1].content.contains("worker"));
        assert!(req.messages[1].content.contains("INFO job started"));
        assert_eq!(req.max_tokens, 1024);
        assert!((req.temperature - 0.0).abs() < 0.001);
    }

    #[tokio::test]
    async fn test_detect_anomalies_returns_error_on_invalid_json() {
        let provider = EchoProvider {
            response: "not json".to_string(),
        };

        let result = detect_anomalies(&provider, &[]).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Failed to parse LLM response"));
    }

    #[tokio::test]
    async fn test_format_log_batches_includes_process_headers() {
        let batches = vec![
            ("api".to_string(), vec!["INFO startup".to_string()]),
            ("db".to_string(), vec!["INFO connected".to_string()]),
        ];
        let formatted = format_log_batches(&batches);
        assert!(formatted.contains("### Process: api"));
        assert!(formatted.contains("### Process: db"));
        assert!(formatted.contains("INFO startup"));
        assert!(formatted.contains("INFO connected"));
    }
}
