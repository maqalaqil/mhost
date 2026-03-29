use async_trait::async_trait;

/// A single message in an LLM conversation.
#[derive(Debug, Clone)]
pub struct LlmMessage {
    /// Role of the message sender: "system", "user", or "assistant".
    pub role: String,
    /// Text content of the message.
    pub content: String,
}

/// Parameters for a single LLM completion request.
#[derive(Debug, Clone)]
pub struct LlmRequest {
    /// Conversation history including system, user, and assistant turns.
    pub messages: Vec<LlmMessage>,
    /// Maximum number of tokens to generate in the response.
    pub max_tokens: u32,
    /// Sampling temperature (0.0–2.0 for OpenAI, 0.0–1.0 for Claude).
    pub temperature: f32,
}

/// A successful response from an LLM provider.
#[derive(Debug, Clone)]
pub struct LlmResponse {
    /// The generated text content.
    pub content: String,
    /// Model identifier returned by the provider.
    pub model: String,
    /// Token usage stats when reported by the provider.
    pub usage: Option<TokenUsage>,
}

/// Token consumption for a single request/response pair.
#[derive(Debug, Clone)]
pub struct TokenUsage {
    /// Number of tokens in the prompt / input.
    pub input_tokens: u32,
    /// Number of tokens in the completion / output.
    pub output_tokens: u32,
}

/// Abstraction over different LLM back-ends.
///
/// Implement this trait to add a new provider without touching the rest of
/// `mhost-ai`.
#[async_trait]
pub trait LlmProvider: Send + Sync {
    /// Send `request` to the provider and return its response.
    async fn complete(&self, request: LlmRequest) -> Result<LlmResponse, String>;

    /// Human-readable name for this provider (e.g. "openai", "claude").
    fn provider_name(&self) -> &str;

    /// Model identifier used for requests (e.g. "gpt-4o", "claude-3-5-sonnet-20241022").
    fn model_name(&self) -> &str;
}
