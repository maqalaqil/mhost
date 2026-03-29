use async_trait::async_trait;
use serde_json::{json, Value};

use crate::provider::{LlmProvider, LlmRequest, LlmResponse, TokenUsage};

const CLAUDE_MESSAGES_URL: &str = "https://api.anthropic.com/v1/messages";
const ANTHROPIC_VERSION: &str = "2023-06-01";

/// Anthropic Claude messages provider.
pub struct ClaudeProvider {
    api_key: String,
    model: String,
    client: reqwest::Client,
}

impl ClaudeProvider {
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
    /// The Claude API separates the system prompt from the conversation
    /// messages.  Any message with `role == "system"` is extracted and joined
    /// into the top-level `system` field; the remaining messages form the
    /// `messages` array.
    ///
    /// Exposed for unit-testing the serialisation logic.
    pub fn build_request_body(&self, request: &LlmRequest) -> Value {
        let system: String = request
            .messages
            .iter()
            .filter(|m| m.role == "system")
            .map(|m| m.content.as_str())
            .collect::<Vec<_>>()
            .join("\n");

        let messages: Vec<Value> = request
            .messages
            .iter()
            .filter(|m| m.role != "system")
            .map(|m| json!({ "role": m.role, "content": m.content }))
            .collect();

        let mut body = json!({
            "model": self.model,
            "max_tokens": request.max_tokens,
            "messages": messages,
        });

        if !system.is_empty() {
            body["system"] = json!(system);
        }

        body
    }
}

#[async_trait]
impl LlmProvider for ClaudeProvider {
    async fn complete(&self, request: LlmRequest) -> Result<LlmResponse, String> {
        let body = self.build_request_body(&request);

        let http_response = self
            .client
            .post(CLAUDE_MESSAGES_URL)
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", ANTHROPIC_VERSION)
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|e| format!("Claude request failed: {e}"))?;

        let status = http_response.status();
        let json: Value = http_response
            .json()
            .await
            .map_err(|e| format!("Failed to parse Claude response: {e}"))?;

        if !status.is_success() {
            let message = json["error"]["message"]
                .as_str()
                .unwrap_or("unknown error")
                .to_owned();
            return Err(format!("Claude API error {status}: {message}"));
        }

        let content = json["content"][0]["text"]
            .as_str()
            .ok_or_else(|| format!("Unexpected Claude response shape: {json}"))?
            .to_owned();

        let model = json["model"]
            .as_str()
            .unwrap_or(&self.model)
            .to_owned();

        let usage = parse_usage(&json);

        Ok(LlmResponse { content, model, usage })
    }

    fn provider_name(&self) -> &str {
        "claude"
    }

    fn model_name(&self) -> &str {
        &self.model
    }
}

/// Extract token-usage information from a Claude response envelope.
fn parse_usage(json: &Value) -> Option<TokenUsage> {
    let usage = json.get("usage")?;
    let input_tokens = usage["input_tokens"].as_u64()? as u32;
    let output_tokens = usage["output_tokens"].as_u64()? as u32;
    Some(TokenUsage { input_tokens, output_tokens })
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider::LlmMessage;

    fn make_provider() -> ClaudeProvider {
        ClaudeProvider::new("test-key", "claude-3-5-sonnet-20241022")
    }

    fn make_request_with_system() -> LlmRequest {
        LlmRequest {
            messages: vec![
                LlmMessage { role: "system".into(), content: "You are a helpful assistant.".into() },
                LlmMessage { role: "user".into(), content: "Hello!".into() },
            ],
            max_tokens: 256,
            temperature: 0.5,
        }
    }

    fn make_request_no_system() -> LlmRequest {
        LlmRequest {
            messages: vec![
                LlmMessage { role: "user".into(), content: "Hello!".into() },
            ],
            max_tokens: 128,
            temperature: 0.0,
        }
    }

    #[test]
    fn test_provider_metadata() {
        let p = make_provider();
        assert_eq!(p.provider_name(), "claude");
        assert_eq!(p.model_name(), "claude-3-5-sonnet-20241022");
    }

    #[test]
    fn test_request_body_top_level_fields() {
        let p = make_provider();
        let body = p.build_request_body(&make_request_with_system());

        assert_eq!(body["model"], "claude-3-5-sonnet-20241022");
        assert_eq!(body["max_tokens"], 256);
    }

    #[test]
    fn test_system_extracted_to_top_level() {
        let p = make_provider();
        let body = p.build_request_body(&make_request_with_system());

        assert_eq!(body["system"], "You are a helpful assistant.");

        let messages = body["messages"].as_array().unwrap();
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0]["role"], "user");
        assert_eq!(messages[0]["content"], "Hello!");
    }

    #[test]
    fn test_no_system_field_when_absent() {
        let p = make_provider();
        let body = p.build_request_body(&make_request_no_system());

        assert!(body.get("system").is_none() || body["system"].is_null());

        let messages = body["messages"].as_array().unwrap();
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0]["role"], "user");
    }

    #[test]
    fn test_parse_usage_success() {
        let json = json!({
            "usage": {
                "input_tokens": 15,
                "output_tokens": 30
            }
        });
        let usage = parse_usage(&json).unwrap();
        assert_eq!(usage.input_tokens, 15);
        assert_eq!(usage.output_tokens, 30);
    }

    #[test]
    fn test_parse_usage_missing_returns_none() {
        let json = json!({});
        assert!(parse_usage(&json).is_none());
    }
}
