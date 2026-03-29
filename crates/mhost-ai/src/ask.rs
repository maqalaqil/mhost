use crate::context::ProcessContext;
use crate::prompts;
use crate::provider::{LlmMessage, LlmProvider, LlmRequest};

/// Ask a free-form question about the managed processes.
///
/// All process contexts are summarised and included as background information
/// so the LLM can answer questions about any process or compare them.
pub async fn ask(
    provider: &dyn LlmProvider,
    question: &str,
    all_contexts: &[ProcessContext],
) -> Result<String, String> {
    let context_summary = build_context_summary(all_contexts);
    let user_content = format!(
        "Current process state:\n{context_summary}\n\nQuestion: {question}"
    );

    let request = LlmRequest {
        messages: vec![
            LlmMessage {
                role: "system".into(),
                content: prompts::ask_system_prompt().into(),
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

/// Build a compact multi-process summary suitable for an LLM prompt.
fn build_context_summary(contexts: &[ProcessContext]) -> String {
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

    fn make_context(name: &str, status: ProcessStatus) -> ProcessContext {
        let config = ProcessConfig {
            name: name.to_string(),
            command: "node".to_string(),
            args: vec!["app.js".to_string()],
            ..Default::default()
        };
        let mut info = ProcessInfo::new(config, 0);
        info.status = status;
        ProcessContext::from_process_info(&info, vec![], vec![], vec![])
    }

    #[tokio::test]
    async fn test_ask_returns_provider_response() {
        let provider = MockProvider {
            response_text: "Run: mhost restart api".into(),
        };
        let ctx = make_context("api", ProcessStatus::Online);
        let result = ask(&provider, "How do I restart the API?", &[ctx]).await;

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "Run: mhost restart api");
    }

    #[tokio::test]
    async fn test_ask_request_contains_question_and_all_contexts() {
        let captured = Arc::new(Mutex::new(None));
        let provider = CapturingProvider {
            captured: Arc::clone(&captured),
            response: "answer".into(),
        };

        let contexts = vec![
            make_context("web", ProcessStatus::Online),
            make_context("worker", ProcessStatus::Stopped),
        ];

        ask(&provider, "Which processes are stopped?", &contexts)
            .await
            .unwrap();

        let req = captured.lock().unwrap().take().unwrap();
        assert_eq!(req.messages.len(), 2);
        assert_eq!(req.messages[0].role, "system");
        assert_eq!(req.messages[1].role, "user");
        // Question must be in the user message
        assert!(req.messages[1]
            .content
            .contains("Which processes are stopped?"));
        // Both process names must be present in the context summary
        assert!(req.messages[1].content.contains("web"));
        assert!(req.messages[1].content.contains("worker"));
        assert_eq!(req.max_tokens, 2048);
        assert!((req.temperature - 0.3).abs() < 0.001);
    }

    #[tokio::test]
    async fn test_ask_handles_empty_context_list() {
        let captured = Arc::new(Mutex::new(None));
        let provider = CapturingProvider {
            captured: Arc::clone(&captured),
            response: "No processes running.".into(),
        };

        ask(&provider, "What is running?", &[]).await.unwrap();

        let req = captured.lock().unwrap().take().unwrap();
        assert!(req.messages[1]
            .content
            .contains("No processes currently managed."));
    }

    #[tokio::test]
    async fn test_ask_propagates_provider_error() {
        struct FailingProvider;

        #[async_trait]
        impl LlmProvider for FailingProvider {
            async fn complete(&self, _: LlmRequest) -> Result<LlmResponse, String> {
                Err("upstream error".into())
            }

            fn provider_name(&self) -> &str {
                "fail"
            }

            fn model_name(&self) -> &str {
                "fail-model"
            }
        }

        let result = ask(&FailingProvider, "anything", &[]).await;
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), "upstream error");
    }
}
