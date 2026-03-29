use crate::context::ProcessContext;
use crate::prompts;
use crate::provider::{LlmMessage, LlmProvider, LlmRequest};

/// Send a process context to the configured LLM provider and return a
/// structured diagnostic analysis.
///
/// The response is plain Markdown text suitable for terminal rendering.
pub async fn diagnose(
    provider: &dyn LlmProvider,
    context: &ProcessContext,
) -> Result<String, String> {
    let request = LlmRequest {
        messages: vec![
            LlmMessage {
                role: "system".into(),
                content: prompts::diagnose_system_prompt().into(),
            },
            LlmMessage {
                role: "user".into(),
                content: format!("Diagnose this process:\n\n{}", context.to_prompt_text()),
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

    // -----------------------------------------------------------------------
    // Mock provider that captures the request without calling any API
    // -----------------------------------------------------------------------

    struct MockProvider {
        /// Fixed response text returned for every request.
        response_text: String,
    }

    #[async_trait]
    impl LlmProvider for MockProvider {
        async fn complete(&self, _request: LlmRequest) -> Result<LlmResponse, String> {
            Ok(LlmResponse {
                content: self.response_text.clone(),
                model: "mock-model".into(),
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

    fn make_errored_context() -> ProcessContext {
        let config = ProcessConfig {
            name: "crashed-worker".to_string(),
            command: "node".to_string(),
            args: vec!["worker.js".to_string()],
            ..Default::default()
        };
        let mut info = ProcessInfo::new(config, 0);
        info.status = ProcessStatus::Errored;
        info.exit_code = Some(137);
        info.restart_count = 5;

        ProcessContext::from_process_info(
            &info,
            vec!["Starting worker...".into(), "FATAL: out of memory".into()],
            vec!["FATAL: out of memory".into()],
            vec!["worker exited with code 137".into()],
        )
    }

    // -----------------------------------------------------------------------
    // diagnose returns provider response unchanged
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_diagnose_returns_provider_response() {
        let provider = MockProvider {
            response_text: "## Root Cause\nOOM kill".into(),
        };
        let ctx = make_errored_context();
        let result = diagnose(&provider, &ctx).await;

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "## Root Cause\nOOM kill");
    }

    // -----------------------------------------------------------------------
    // diagnose propagates provider errors
    // -----------------------------------------------------------------------

    struct FailingProvider;

    #[async_trait]
    impl LlmProvider for FailingProvider {
        async fn complete(&self, _request: LlmRequest) -> Result<LlmResponse, String> {
            Err("network timeout".into())
        }

        fn provider_name(&self) -> &str {
            "failing"
        }

        fn model_name(&self) -> &str {
            "failing-model"
        }
    }

    #[tokio::test]
    async fn test_diagnose_propagates_provider_error() {
        let provider = FailingProvider;
        let ctx = make_errored_context();
        let result = diagnose(&provider, &ctx).await;

        assert!(result.is_err());
        assert_eq!(result.err().unwrap(), "network timeout");
    }

    // -----------------------------------------------------------------------
    // diagnose builds a request with the correct structure
    // -----------------------------------------------------------------------

    /// A provider that validates the request shape and echoes back metadata.
    struct ValidatingProvider;

    #[async_trait]
    impl LlmProvider for ValidatingProvider {
        async fn complete(&self, request: LlmRequest) -> Result<LlmResponse, String> {
            // Validate message count
            if request.messages.len() != 2 {
                return Err(format!(
                    "expected 2 messages, got {}",
                    request.messages.len()
                ));
            }
            // Validate roles
            if request.messages[0].role != "system" {
                return Err(format!(
                    "expected first role 'system', got '{}'",
                    request.messages[0].role
                ));
            }
            if request.messages[1].role != "user" {
                return Err(format!(
                    "expected second role 'user', got '{}'",
                    request.messages[1].role
                ));
            }
            // Validate system prompt is not empty
            if request.messages[0].content.is_empty() {
                return Err("system prompt must not be empty".into());
            }
            // Validate user message contains process name
            if !request.messages[1].content.contains("crashed-worker") {
                return Err("user message must reference the process name".into());
            }
            // Validate token budget
            if request.max_tokens != 2048 {
                return Err(format!(
                    "expected max_tokens=2048, got {}",
                    request.max_tokens
                ));
            }
            // Validate temperature
            if (request.temperature - 0.3).abs() > 0.001 {
                return Err(format!(
                    "expected temperature=0.3, got {}",
                    request.temperature
                ));
            }

            Ok(LlmResponse {
                content: "ok".into(),
                model: "validating-model".into(),
                usage: None,
            })
        }

        fn provider_name(&self) -> &str {
            "validating"
        }

        fn model_name(&self) -> &str {
            "validating-model"
        }
    }

    #[tokio::test]
    async fn test_diagnose_builds_correct_request_structure() {
        let provider = ValidatingProvider;
        let ctx = make_errored_context();
        let result = diagnose(&provider, &ctx).await;

        // If the provider returned an error it means the request was malformed.
        assert!(
            result.is_ok(),
            "diagnose built a malformed request: {:?}",
            result.err()
        );
    }

    // -----------------------------------------------------------------------
    // diagnose embeds process context in user message
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_diagnose_user_message_contains_prompt_text() {
        // Use a provider that returns the user message back as the response
        // so we can inspect what was sent.
        struct EchoUserMessageProvider;

        #[async_trait]
        impl LlmProvider for EchoUserMessageProvider {
            async fn complete(&self, request: LlmRequest) -> Result<LlmResponse, String> {
                let user_msg = request
                    .messages
                    .iter()
                    .find(|m| m.role == "user")
                    .map(|m| m.content.clone())
                    .unwrap_or_default();
                Ok(LlmResponse {
                    content: user_msg,
                    model: "echo".into(),
                    usage: None,
                })
            }

            fn provider_name(&self) -> &str {
                "echo"
            }

            fn model_name(&self) -> &str {
                "echo"
            }
        }

        let provider = EchoUserMessageProvider;
        let ctx = make_errored_context();
        let result = diagnose(&provider, &ctx).await.unwrap();

        // The user message should include the rendered prompt text
        assert!(result.contains("## Process: crashed-worker"));
        assert!(result.contains("FATAL: out of memory"));
        assert!(result.contains("Exit Code: 137"));
    }
}
