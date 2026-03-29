use crate::prompts;
use crate::provider::{LlmMessage, LlmProvider, LlmRequest};

/// Structured search parameters translated from a natural-language log query.
#[derive(Debug, Clone, serde::Deserialize)]
pub struct LogQueryResult {
    /// Full-text-search terms (FTS5 compatible).
    pub search: Option<String>,
    /// Log level filter: "error", "warn", "info", or null.
    pub level: Option<String>,
    /// Time window: "1h", "24h", "7d", or null.
    pub since: Option<String>,
    /// Maximum number of log lines to return.
    pub limit: Option<u32>,
}

/// Translate a natural-language log query into structured [`LogQueryResult`]
/// search parameters via the configured LLM provider.
pub async fn translate_log_query(
    provider: &dyn LlmProvider,
    process_name: &str,
    natural_query: &str,
) -> Result<LogQueryResult, String> {
    let request = LlmRequest {
        messages: vec![
            LlmMessage {
                role: "system".into(),
                content: prompts::log_query_system_prompt().into(),
            },
            LlmMessage {
                role: "user".into(),
                content: format!("Process: {}\nQuery: {}", process_name, natural_query),
            },
        ],
        max_tokens: 256,
        temperature: 0.0,
    };

    let response = provider.complete(request).await?;

    // Strip optional markdown code fences before parsing JSON.
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

    serde_json::from_str(&json_str)
        .map_err(|e| format!("Failed to parse LLM response as query: {e}"))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider::{LlmRequest, LlmResponse, TokenUsage};
    use async_trait::async_trait;

    // -----------------------------------------------------------------------
    // Echo provider — returns the user message content as the response
    // -----------------------------------------------------------------------

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

    // -----------------------------------------------------------------------
    // Capturing provider — records the request for inspection
    // -----------------------------------------------------------------------

    use std::sync::{Arc, Mutex};

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

    // -----------------------------------------------------------------------
    // Tests
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_translate_log_query_parses_valid_json() {
        let json = r#"{"search":"out of memory","level":"error","since":"1h","limit":50}"#;
        let provider = EchoProvider {
            response: json.to_string(),
        };

        let result = translate_log_query(&provider, "api-server", "show memory errors").await;
        assert!(result.is_ok(), "unexpected error: {:?}", result.err());

        let q = result.unwrap();
        assert_eq!(q.search.as_deref(), Some("out of memory"));
        assert_eq!(q.level.as_deref(), Some("error"));
        assert_eq!(q.since.as_deref(), Some("1h"));
        assert_eq!(q.limit, Some(50));
    }

    #[tokio::test]
    async fn test_translate_log_query_strips_markdown_fences() {
        let json = "```json\n{\"search\":\"timeout\",\"level\":null,\"since\":\"24h\",\"limit\":100}\n```";
        let provider = EchoProvider {
            response: json.to_string(),
        };

        let result = translate_log_query(&provider, "worker", "show timeouts last day").await;
        assert!(result.is_ok(), "unexpected error: {:?}", result.err());

        let q = result.unwrap();
        assert_eq!(q.search.as_deref(), Some("timeout"));
        assert_eq!(q.since.as_deref(), Some("24h"));
        assert_eq!(q.limit, Some(100));
    }

    #[tokio::test]
    async fn test_translate_log_query_request_contains_process_name_and_query() {
        let json = r#"{"search":null,"level":null,"since":null,"limit":null}"#;
        let captured = Arc::new(Mutex::new(None));
        let provider = CapturingProvider {
            captured: Arc::clone(&captured),
            response: json.to_string(),
        };

        translate_log_query(&provider, "my-process", "find all errors")
            .await
            .unwrap();

        let req = captured.lock().unwrap().take().unwrap();
        assert_eq!(req.messages.len(), 2);
        assert_eq!(req.messages[0].role, "system");
        assert_eq!(req.messages[1].role, "user");
        assert!(req.messages[1].content.contains("my-process"));
        assert!(req.messages[1].content.contains("find all errors"));
        assert_eq!(req.max_tokens, 256);
        assert!((req.temperature - 0.0).abs() < 0.001);
    }

    #[tokio::test]
    async fn test_translate_log_query_returns_error_on_invalid_json() {
        let provider = EchoProvider {
            response: "not valid json".to_string(),
        };

        let result = translate_log_query(&provider, "proc", "anything").await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Failed to parse LLM response as query"));
    }

    #[tokio::test]
    async fn test_translate_log_query_handles_null_fields() {
        let json = r#"{"search":null,"level":null,"since":null,"limit":null}"#;
        let provider = EchoProvider {
            response: json.to_string(),
        };

        let result = translate_log_query(&provider, "proc", "show everything").await;
        assert!(result.is_ok());
        let q = result.unwrap();
        assert!(q.search.is_none());
        assert!(q.level.is_none());
        assert!(q.since.is_none());
        assert!(q.limit.is_none());
    }
}
