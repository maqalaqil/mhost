use crate::prompts;
use crate::provider::{LlmMessage, LlmProvider, LlmRequest};

/// Generate a complete `mhost.toml` ecosystem configuration from a
/// natural-language description of the desired setup.
///
/// Returns the raw TOML string produced by the LLM.
pub async fn generate_config(
    provider: &dyn LlmProvider,
    description: &str,
) -> Result<String, String> {
    let request = LlmRequest {
        messages: vec![
            LlmMessage {
                role: "system".into(),
                content: prompts::config_gen_system_prompt().into(),
            },
            LlmMessage {
                role: "user".into(),
                content: description.to_string(),
            },
        ],
        max_tokens: 2048,
        temperature: 0.2,
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
                    output_tokens: 30,
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

    #[tokio::test]
    async fn test_generate_config_returns_provider_response() {
        let toml = "[process.api]\ncommand = \"node server.js\"";
        let provider = MockProvider {
            response_text: toml.to_string(),
        };

        let result = generate_config(&provider, "a Node.js API server").await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), toml);
    }

    #[tokio::test]
    async fn test_generate_config_request_contains_description() {
        let captured = Arc::new(Mutex::new(None));
        let provider = CapturingProvider {
            captured: Arc::clone(&captured),
            response: "[process.api]\ncommand = \"./api\"".into(),
        };

        let description = "a REST API with a Redis worker and a Postgres database";
        generate_config(&provider, description).await.unwrap();

        let req = captured.lock().unwrap().take().unwrap();
        assert_eq!(req.messages.len(), 2);
        assert_eq!(req.messages[0].role, "system");
        assert_eq!(req.messages[1].role, "user");
        assert!(req.messages[1].content.contains("REST API"));
        assert!(req.messages[1].content.contains("Redis worker"));
        assert!(req.messages[1].content.contains("Postgres database"));
        assert_eq!(req.max_tokens, 2048);
        assert!((req.temperature - 0.2).abs() < 0.001);
    }

    #[tokio::test]
    async fn test_generate_config_system_prompt_present() {
        let captured = Arc::new(Mutex::new(None));
        let provider = CapturingProvider {
            captured: Arc::clone(&captured),
            response: "".into(),
        };

        generate_config(&provider, "simple web app").await.unwrap();

        let req = captured.lock().unwrap().take().unwrap();
        assert!(!req.messages[0].content.is_empty());
        // System prompt should mention TOML (from config_gen_system_prompt)
        assert!(req.messages[0].content.contains("TOML"));
    }

    #[tokio::test]
    async fn test_generate_config_propagates_provider_error() {
        struct FailingProvider;

        #[async_trait]
        impl LlmProvider for FailingProvider {
            async fn complete(&self, _: LlmRequest) -> Result<LlmResponse, String> {
                Err("connection refused".into())
            }

            fn provider_name(&self) -> &str {
                "fail"
            }

            fn model_name(&self) -> &str {
                "fail-model"
            }
        }

        let result = generate_config(&FailingProvider, "anything").await;
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), "connection refused");
    }
}
