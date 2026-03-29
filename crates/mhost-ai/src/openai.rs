use async_trait::async_trait;
use serde_json::{json, Value};

use crate::provider::{LlmProvider, LlmRequest, LlmResponse, TokenUsage};

const OPENAI_CHAT_URL: &str = "https://api.openai.com/v1/chat/completions";

/// OpenAI chat-completions provider.
pub struct OpenAiProvider {
    api_key: String,
    model: String,
    client: reqwest::Client,
}

impl OpenAiProvider {
    /// Create a new provider with the given API key and model name.
    pub fn new(api_key: &str, model: &str) -> Self {
        Self {
            api_key: api_key.to_owned(),
            model: model.to_owned(),
            client: reqwest::Client::new(),
        }
    }

    /// Build the JSON request body without sending it.
    ///
    /// Exposed for unit-testing the serialisation logic.
    pub fn build_request_body(&self, request: &LlmRequest) -> Value {
        let messages: Vec<Value> = request
            .messages
            .iter()
            .map(|m| json!({ "role": m.role, "content": m.content }))
            .collect();

        json!({
            "model": self.model,
            "messages": messages,
            "max_tokens": request.max_tokens,
            "temperature": request.temperature,
        })
    }
}

#[async_trait]
impl LlmProvider for OpenAiProvider {
    async fn complete(&self, request: LlmRequest) -> Result<LlmResponse, String> {
        let body = self.build_request_body(&request);

        let http_response = self
            .client
            .post(OPENAI_CHAT_URL)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|e| format!("OpenAI request failed: {e}"))?;

        let status = http_response.status();
        let json: Value = http_response
            .json()
            .await
            .map_err(|e| format!("Failed to parse OpenAI response: {e}"))?;

        if !status.is_success() {
            let message = json["error"]["message"]
                .as_str()
                .unwrap_or("unknown error")
                .to_owned();
            return Err(format!("OpenAI API error {status}: {message}"));
        }

        let content = json["choices"][0]["message"]["content"]
            .as_str()
            .ok_or_else(|| format!("Unexpected OpenAI response shape: {json}"))?
            .to_owned();

        let model = json["model"].as_str().unwrap_or(&self.model).to_owned();

        let usage = parse_usage(&json);

        Ok(LlmResponse {
            content,
            model,
            usage,
        })
    }

    fn provider_name(&self) -> &str {
        "openai"
    }

    fn model_name(&self) -> &str {
        &self.model
    }
}

/// Extract token-usage information from an OpenAI response envelope.
fn parse_usage(json: &Value) -> Option<TokenUsage> {
    let usage = json.get("usage")?;
    let input_tokens = usage["prompt_tokens"].as_u64()? as u32;
    let output_tokens = usage["completion_tokens"].as_u64()? as u32;
    Some(TokenUsage {
        input_tokens,
        output_tokens,
    })
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider::LlmMessage;

    fn make_provider() -> OpenAiProvider {
        OpenAiProvider::new("test-key", "gpt-4o")
    }

    fn make_request() -> LlmRequest {
        LlmRequest {
            messages: vec![
                LlmMessage {
                    role: "system".into(),
                    content: "You are a helpful assistant.".into(),
                },
                LlmMessage {
                    role: "user".into(),
                    content: "Hello!".into(),
                },
            ],
            max_tokens: 256,
            temperature: 0.7,
        }
    }

    #[test]
    fn test_provider_metadata() {
        let p = make_provider();
        assert_eq!(p.provider_name(), "openai");
        assert_eq!(p.model_name(), "gpt-4o");
    }

    #[test]
    fn test_request_body_top_level_fields() {
        let p = make_provider();
        let body = p.build_request_body(&make_request());

        assert_eq!(body["model"], "gpt-4o");
        assert_eq!(body["max_tokens"], 256);
        assert!((body["temperature"].as_f64().unwrap() - 0.7).abs() < 1e-6);
    }

    #[test]
    fn test_request_body_messages_format() {
        let p = make_provider();
        let body = p.build_request_body(&make_request());
        let messages = body["messages"].as_array().unwrap();

        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0]["role"], "system");
        assert_eq!(messages[0]["content"], "You are a helpful assistant.");
        assert_eq!(messages[1]["role"], "user");
        assert_eq!(messages[1]["content"], "Hello!");
    }

    #[test]
    fn test_parse_usage_success() {
        let json = json!({
            "usage": {
                "prompt_tokens": 10,
                "completion_tokens": 20
            }
        });
        let usage = parse_usage(&json).unwrap();
        assert_eq!(usage.input_tokens, 10);
        assert_eq!(usage.output_tokens, 20);
    }

    #[test]
    fn test_parse_usage_missing_returns_none() {
        let json = json!({});
        assert!(parse_usage(&json).is_none());
    }
}
