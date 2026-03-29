use crate::context::ProcessContext;
use crate::prompts;
use crate::provider::{LlmMessage, LlmProvider, LlmRequest};

/// Explain the contents of a `mhost.toml` configuration in plain English.
///
/// Returns a Markdown-formatted explanation suitable for display in the
/// terminal or a documentation page.
pub async fn explain_config(
    provider: &dyn LlmProvider,
    config_content: &str,
) -> Result<String, String> {
    let request = LlmRequest {
        messages: vec![
            LlmMessage {
                role: "system".into(),
                content: prompts::explain_system_prompt().into(),
            },
            LlmMessage {
                role: "user".into(),
                content: format!(
                    "Explain this mhost.toml:\n\n```toml\n{config_content}\n```"
                ),
            },
        ],
        max_tokens: 2048,
        temperature: 0.3,
    };

    let response = provider.complete(request).await?;
    Ok(response.content)
}

/// Analyse the current state of all processes and suggest proactive
/// improvements as a Markdown-formatted advisory.
pub async fn suggest_improvements(
    provider: &dyn LlmProvider,
    all_contexts: &[ProcessContext],
) -> Result<String, String> {
    let summary = build_all_contexts_summary(all_contexts);

    let request = LlmRequest {
        messages: vec![
            LlmMessage {
                role: "system".into(),
                content: prompts::suggest_system_prompt().into(),
            },
            LlmMessage {
                role: "user".into(),
                content: format!("Current process state:\n{summary}"),
            },
        ],
        max_tokens: 2048,
        temperature: 0.3,
    };

    let response = provider.complete(request).await?;
    Ok(response.content)
}

/// Build a multi-process summary for the suggest_improvements prompt.
fn build_all_contexts_summary(contexts: &[ProcessContext]) -> String {
    if contexts.is_empty() {
        return "No processes currently managed.".to_string();
    }

    let mut summary = String::new();
    for ctx in contexts {
        summary.push_str(&ctx.to_prompt_text());
        summary.push('\n');
    }
    summary
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

    fn make_context(name: &str) -> ProcessContext {
        let config = ProcessConfig {
            name: name.to_string(),
            command: "node".to_string(),
            args: vec!["app.js".to_string()],
            ..Default::default()
        };
        let mut info = ProcessInfo::new(config, 0);
        info.status = ProcessStatus::Online;
        info.restart_count = 2;
        ProcessContext::from_process_info(&info, vec![], vec![], vec![])
    }

    // -----------------------------------------------------------------------
    // explain_config tests
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_explain_config_returns_provider_response() {
        let provider = MockProvider {
            response_text: "This config runs a Node.js API server.".into(),
        };

        let toml = "[process.api]\ncommand = \"node server.js\"";
        let result = explain_config(&provider, toml).await;

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "This config runs a Node.js API server.");
    }

    #[tokio::test]
    async fn test_explain_config_request_contains_config_content() {
        let captured = Arc::new(Mutex::new(None));
        let provider = CapturingProvider {
            captured: Arc::clone(&captured),
            response: "explanation".into(),
        };

        let toml = "[process.worker]\ncommand = \"python worker.py\"\ninstances = 4";
        explain_config(&provider, toml).await.unwrap();

        let req = captured.lock().unwrap().take().unwrap();
        assert_eq!(req.messages.len(), 2);
        assert_eq!(req.messages[0].role, "system");
        assert_eq!(req.messages[1].role, "user");
        assert!(req.messages[1].content.contains("worker.py"));
        assert!(req.messages[1].content.contains("instances = 4"));
        assert_eq!(req.max_tokens, 2048);
        assert!((req.temperature - 0.3).abs() < 0.001);
    }

    #[tokio::test]
    async fn test_explain_config_system_prompt_mentions_mhost_toml() {
        let captured = Arc::new(Mutex::new(None));
        let provider = CapturingProvider {
            captured: Arc::clone(&captured),
            response: "".into(),
        };

        explain_config(&provider, "").await.unwrap();

        let req = captured.lock().unwrap().take().unwrap();
        assert!(req.messages[0].content.contains("mhost.toml"));
    }

    #[tokio::test]
    async fn test_explain_config_propagates_provider_error() {
        struct FailingProvider;

        #[async_trait]
        impl LlmProvider for FailingProvider {
            async fn complete(&self, _: LlmRequest) -> Result<LlmResponse, String> {
                Err("service unavailable".into())
            }

            fn provider_name(&self) -> &str {
                "fail"
            }

            fn model_name(&self) -> &str {
                "fail-model"
            }
        }

        let result = explain_config(&FailingProvider, "anything").await;
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), "service unavailable");
    }

    // -----------------------------------------------------------------------
    // suggest_improvements tests
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_suggest_improvements_returns_provider_response() {
        let provider = MockProvider {
            response_text: "Consider scaling the worker process to 4 instances.".into(),
        };

        let ctx = make_context("worker");
        let result = suggest_improvements(&provider, &[ctx]).await;

        assert!(result.is_ok());
        assert_eq!(
            result.unwrap(),
            "Consider scaling the worker process to 4 instances."
        );
    }

    #[tokio::test]
    async fn test_suggest_improvements_request_contains_all_contexts() {
        let captured = Arc::new(Mutex::new(None));
        let provider = CapturingProvider {
            captured: Arc::clone(&captured),
            response: "suggestions".into(),
        };

        let contexts = vec![
            make_context("api"),
            make_context("worker"),
            make_context("db"),
        ];

        suggest_improvements(&provider, &contexts).await.unwrap();

        let req = captured.lock().unwrap().take().unwrap();
        assert_eq!(req.messages.len(), 2);
        assert_eq!(req.messages[0].role, "system");
        assert_eq!(req.messages[1].role, "user");
        assert!(req.messages[1].content.contains("api"));
        assert!(req.messages[1].content.contains("worker"));
        assert!(req.messages[1].content.contains("db"));
        assert_eq!(req.max_tokens, 2048);
        assert!((req.temperature - 0.3).abs() < 0.001);
    }

    #[tokio::test]
    async fn test_suggest_improvements_handles_empty_contexts() {
        let captured = Arc::new(Mutex::new(None));
        let provider = CapturingProvider {
            captured: Arc::clone(&captured),
            response: "No suggestions.".into(),
        };

        suggest_improvements(&provider, &[]).await.unwrap();

        let req = captured.lock().unwrap().take().unwrap();
        assert!(req.messages[1]
            .content
            .contains("No processes currently managed."));
    }

    #[tokio::test]
    async fn test_suggest_improvements_system_prompt_mentions_scaling() {
        let captured = Arc::new(Mutex::new(None));
        let provider = CapturingProvider {
            captured: Arc::clone(&captured),
            response: "".into(),
        };

        suggest_improvements(&provider, &[]).await.unwrap();

        let req = captured.lock().unwrap().take().unwrap();
        assert!(req.messages[0].content.contains("scaled"));
    }
}
