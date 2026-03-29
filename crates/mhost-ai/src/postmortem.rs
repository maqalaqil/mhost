use crate::context::ProcessContext;
use crate::prompts;
use crate::provider::{LlmMessage, LlmProvider, LlmRequest};

/// Generate a structured post-mortem incident report in Markdown format from
/// a process context and its metrics history.
pub async fn generate_postmortem(
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
                content: prompts::postmortem_system_prompt().into(),
            },
            LlmMessage {
                role: "user".into(),
                content: user_content,
            },
        ],
        max_tokens: 4096,
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
                    output_tokens: 50,
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
            name: "payment-service".to_string(),
            command: "java".to_string(),
            args: vec!["-jar".to_string(), "app.jar".to_string()],
            ..Default::default()
        };
        let mut info = ProcessInfo::new(config, 0);
        info.status = ProcessStatus::Errored;
        info.exit_code = Some(1);
        info.restart_count = 3;

        ProcessContext::from_process_info(
            &info,
            vec!["Starting payment service...".into(), "ERROR: DB connection failed".into()],
            vec!["ERROR: DB connection failed".into()],
            vec!["2026-03-28T09:00:00Z process crashed".into()],
        )
    }

    #[tokio::test]
    async fn test_generate_postmortem_returns_provider_response() {
        let markdown = "# Incident Report: payment-service\n## Summary\nDB outage.";
        let provider = MockProvider {
            response_text: markdown.to_string(),
        };
        let ctx = make_context();
        let result = generate_postmortem(&provider, &ctx, "mem=512MB,cpu=10%").await;

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), markdown);
    }

    #[tokio::test]
    async fn test_generate_postmortem_request_contains_context_and_metrics() {
        let captured = Arc::new(Mutex::new(None));
        let provider = CapturingProvider {
            captured: Arc::clone(&captured),
            response: "# Report".into(),
        };
        let ctx = make_context();
        let metrics = "avg_cpu=5%,peak_mem=600MB";

        generate_postmortem(&provider, &ctx, metrics).await.unwrap();

        let req = captured.lock().unwrap().take().unwrap();
        assert_eq!(req.messages.len(), 2);
        assert_eq!(req.messages[0].role, "system");
        assert_eq!(req.messages[1].role, "user");
        assert!(req.messages[1].content.contains("payment-service"));
        assert!(req.messages[1].content.contains("avg_cpu=5%"));
        assert!(req.messages[1].content.contains("peak_mem=600MB"));
        assert_eq!(req.max_tokens, 4096);
        assert!((req.temperature - 0.3).abs() < 0.001);
    }

    #[tokio::test]
    async fn test_generate_postmortem_system_prompt_has_root_cause() {
        let captured = Arc::new(Mutex::new(None));
        let provider = CapturingProvider {
            captured: Arc::clone(&captured),
            response: "".into(),
        };
        let ctx = make_context();

        generate_postmortem(&provider, &ctx, "").await.unwrap();

        let req = captured.lock().unwrap().take().unwrap();
        assert!(req.messages[0].content.contains("Root Cause"));
        assert!(req.messages[0].content.contains("Action Items"));
    }

    #[tokio::test]
    async fn test_generate_postmortem_propagates_provider_error() {
        struct FailingProvider;

        #[async_trait]
        impl LlmProvider for FailingProvider {
            async fn complete(&self, _: LlmRequest) -> Result<LlmResponse, String> {
                Err("rate limited".into())
            }

            fn provider_name(&self) -> &str {
                "fail"
            }

            fn model_name(&self) -> &str {
                "fail-model"
            }
        }

        let ctx = make_context();
        let result = generate_postmortem(&FailingProvider, &ctx, "").await;
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), "rate limited");
    }
}
