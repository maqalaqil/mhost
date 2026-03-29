use crate::context::ProcessContext;
use crate::prompts;
use crate::provider::{LlmMessage, LlmProvider, LlmRequest};

/// Analyse process metrics history and return concrete performance
/// optimisation recommendations as Markdown text.
pub async fn optimize(
    provider: &dyn LlmProvider,
    context: &ProcessContext,
    metrics_history: &str,
) -> Result<String, String> {
    let user_content = format!(
        "Process context:\n{}\n\n### Metrics History\n{}",
        context.to_prompt_text(),
        metrics_history,
    );

    let request = LlmRequest {
        messages: vec![
            LlmMessage {
                role: "system".into(),
                content: prompts::optimize_system_prompt().into(),
            },
            LlmMessage {
                role: "user".into(),
                content: user_content,
            },
        ],
        max_tokens: 2048,
        temperature: 0.3,
    };

    let response = provider.complete(request).await?;
    Ok(response.content)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider::{LlmRequest, LlmResponse, TokenUsage};
    use async_trait::async_trait;
    use mhost_core::process::{ProcessConfig, ProcessInfo, ProcessStatus};
    use std::sync::{Arc, Mutex};

    struct MockProvider {
        response_text: String,
    }

    #[async_trait]
    impl LlmProvider for MockProvider {
        async fn complete(&self, _request: LlmRequest) -> Result<LlmResponse, String> {
            Ok(LlmResponse {
                content: self.response_text.clone(),
                model: "mock".into(),
                usage: Some(TokenUsage {
                    input_tokens: 10,
                    output_tokens: 20,
                }),
            })
        }

        fn provider_name(&self) -> &str {
            "mock"
        }

        fn model_name(&self) -> &str {
            "mock-model"
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

    fn make_context() -> ProcessContext {
        let config = ProcessConfig {
            name: "perf-worker".to_string(),
            command: "node".to_string(),
            args: vec!["worker.js".to_string()],
            ..Default::default()
        };
        let mut info = ProcessInfo::new(config, 0);
        info.status = ProcessStatus::Online;
        info.cpu_percent = Some(80.0);
        info.memory_bytes = Some(512 * 1_048_576);
        ProcessContext::from_process_info(&info, vec![], vec![], vec![])
    }

    #[tokio::test]
    async fn test_optimize_returns_provider_response() {
        let provider = MockProvider {
            response_text: "## Resource Sizing\nIncrease memory limit.".into(),
        };
        let ctx = make_context();
        let result = optimize(&provider, &ctx, "cpu=80%,mem=512MB").await;

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "## Resource Sizing\nIncrease memory limit.");
    }

    #[tokio::test]
    async fn test_optimize_request_contains_context_and_metrics() {
        let captured = Arc::new(Mutex::new(None));
        let provider = CapturingProvider {
            captured: Arc::clone(&captured),
            response: "ok".into(),
        };
        let ctx = make_context();
        let metrics = "cpu=80%,mem=512MB,requests=1000/s";

        optimize(&provider, &ctx, metrics).await.unwrap();

        let req = captured.lock().unwrap().take().unwrap();
        assert_eq!(req.messages.len(), 2);
        assert_eq!(req.messages[0].role, "system");
        assert_eq!(req.messages[1].role, "user");
        // Process name must appear in the user message
        assert!(req.messages[1].content.contains("perf-worker"));
        // Metrics string must be present
        assert!(req.messages[1].content.contains("cpu=80%"));
        assert!(req.messages[1].content.contains("requests=1000/s"));
        assert_eq!(req.max_tokens, 2048);
        assert!((req.temperature - 0.3).abs() < 0.001);
    }

    #[tokio::test]
    async fn test_optimize_propagates_provider_error() {
        struct FailingProvider;

        #[async_trait]
        impl LlmProvider for FailingProvider {
            async fn complete(&self, _: LlmRequest) -> Result<LlmResponse, String> {
                Err("timeout".into())
            }

            fn provider_name(&self) -> &str {
                "fail"
            }

            fn model_name(&self) -> &str {
                "fail-model"
            }
        }

        let ctx = make_context();
        let result = optimize(&FailingProvider, &ctx, "").await;
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), "timeout");
    }
}
