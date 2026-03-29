use serde::{Deserialize, Serialize};
use std::path::Path;

use crate::{
    claude::ClaudeProvider,
    openai::OpenAiProvider,
    provider::LlmProvider,
};

/// Configuration for the AI integration layer.
///
/// Serialised to / deserialised from `~/.mhost/ai.json`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiConfig {
    /// Provider backend to use: `"openai"` or `"claude"`.
    pub provider: String,
    /// API key, or an environment-variable reference such as `"${OPENAI_API_KEY}"`.
    pub api_key: String,
    /// Model name passed verbatim to the provider (e.g. `"gpt-4o"`).
    pub model: String,
    /// Upper bound on tokens generated per request.
    pub max_tokens: u32,
}

impl Default for AiConfig {
    fn default() -> Self {
        Self {
            provider: "openai".into(),
            api_key: String::new(),
            model: "gpt-4o".into(),
            max_tokens: 4096,
        }
    }
}

impl AiConfig {
    /// Load configuration from a JSON file.  Returns `None` if the file does
    /// not exist or cannot be parsed.
    pub fn load(path: &Path) -> Option<Self> {
        let content = std::fs::read_to_string(path).ok()?;
        serde_json::from_str(&content).ok()
    }

    /// Persist this configuration as pretty-printed JSON.
    pub fn save(&self, path: &Path) -> Result<(), String> {
        let json = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("Failed to create config directory: {e}"))?;
        }
        std::fs::write(path, json).map_err(|e| format!("Failed to write config: {e}"))
    }

    /// Instantiate the configured [`LlmProvider`].
    ///
    /// The `api_key` field supports environment-variable expansion: a value
    /// wrapped in `${…}` is replaced with the corresponding environment
    /// variable at runtime.
    pub fn create_provider(&self) -> Result<Box<dyn LlmProvider>, String> {
        let key = resolve_api_key(&self.api_key)?;

        match self.provider.as_str() {
            "openai" => Ok(Box::new(OpenAiProvider::new(&key, &self.model))),
            "claude" => Ok(Box::new(ClaudeProvider::new(&key, &self.model))),
            other => Err(format!(
                "Unknown AI provider: '{other}'. Use 'openai' or 'claude'."
            )),
        }
    }
}

/// Resolve the raw api_key string, expanding `${ENV_VAR}` references.
fn resolve_api_key(raw: &str) -> Result<String, String> {
    if raw.starts_with("${") && raw.ends_with('}') {
        let var_name = &raw[2..raw.len() - 1];
        std::env::var(var_name)
            .map_err(|_| format!("Environment variable '{var_name}' not set"))
    } else {
        Ok(raw.to_owned())
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;

    // Helper: create a temporary file path (does not create the file itself).
    fn tmp_path(name: &str) -> std::path::PathBuf {
        env::temp_dir().join(format!("mhost-ai-test-{name}.json"))
    }

    // -----------------------------------------------------------------------
    // Serialisation round-trip
    // -----------------------------------------------------------------------

    #[test]
    fn test_default_values() {
        let cfg = AiConfig::default();
        assert_eq!(cfg.provider, "openai");
        assert_eq!(cfg.model, "gpt-4o");
        assert_eq!(cfg.max_tokens, 4096);
        assert!(cfg.api_key.is_empty());
    }

    #[test]
    fn test_serialisation_roundtrip() {
        let original = AiConfig {
            provider: "claude".into(),
            api_key: "sk-test".into(),
            model: "claude-3-5-sonnet-20241022".into(),
            max_tokens: 2048,
        };

        let json = serde_json::to_string(&original).unwrap();
        let decoded: AiConfig = serde_json::from_str(&json).unwrap();

        assert_eq!(decoded.provider, original.provider);
        assert_eq!(decoded.api_key, original.api_key);
        assert_eq!(decoded.model, original.model);
        assert_eq!(decoded.max_tokens, original.max_tokens);
    }

    // -----------------------------------------------------------------------
    // Load / save
    // -----------------------------------------------------------------------

    #[test]
    fn test_save_and_load() {
        let path = tmp_path("save-load");
        let _ = std::fs::remove_file(&path);

        let cfg = AiConfig {
            provider: "openai".into(),
            api_key: "mykey".into(),
            model: "gpt-4o-mini".into(),
            max_tokens: 1024,
        };

        cfg.save(&path).expect("save should succeed");
        let loaded = AiConfig::load(&path).expect("load should return Some");

        assert_eq!(loaded.provider, cfg.provider);
        assert_eq!(loaded.api_key, cfg.api_key);
        assert_eq!(loaded.model, cfg.model);
        assert_eq!(loaded.max_tokens, cfg.max_tokens);

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn test_load_returns_none_for_missing_file() {
        let path = tmp_path("does-not-exist-xyz987");
        assert!(AiConfig::load(&path).is_none());
    }

    #[test]
    fn test_load_returns_none_for_invalid_json() {
        let path = tmp_path("bad-json");
        std::fs::write(&path, b"not json at all!!!").unwrap();
        assert!(AiConfig::load(&path).is_none());
        let _ = std::fs::remove_file(&path);
    }

    // -----------------------------------------------------------------------
    // create_provider
    // -----------------------------------------------------------------------

    #[test]
    fn test_create_provider_openai() {
        let cfg = AiConfig {
            provider: "openai".into(),
            api_key: "test-key".into(),
            model: "gpt-4o".into(),
            max_tokens: 4096,
        };
        let provider = cfg.create_provider().unwrap();
        assert_eq!(provider.provider_name(), "openai");
        assert_eq!(provider.model_name(), "gpt-4o");
    }

    #[test]
    fn test_create_provider_claude() {
        let cfg = AiConfig {
            provider: "claude".into(),
            api_key: "test-key".into(),
            model: "claude-3-5-sonnet-20241022".into(),
            max_tokens: 2048,
        };
        let provider = cfg.create_provider().unwrap();
        assert_eq!(provider.provider_name(), "claude");
        assert_eq!(provider.model_name(), "claude-3-5-sonnet-20241022");
    }

    #[test]
    fn test_create_provider_unknown_returns_error() {
        let cfg = AiConfig {
            provider: "ollama".into(),
            api_key: "x".into(),
            model: "llama3".into(),
            max_tokens: 512,
        };
        let result = cfg.create_provider();
        assert!(result.is_err(), "expected Err for unknown provider");
        let err = result.err().unwrap();
        assert!(err.contains("Unknown AI provider"));
        assert!(err.contains("ollama"));
    }

    // -----------------------------------------------------------------------
    // Env-var expansion
    // -----------------------------------------------------------------------

    #[test]
    fn test_env_var_expansion_success() {
        let var = "MHOST_AI_TEST_KEY_9Z";
        env::set_var(var, "resolved-secret");

        let cfg = AiConfig {
            provider: "openai".into(),
            api_key: format!("${{{var}}}"),
            model: "gpt-4o".into(),
            max_tokens: 512,
        };

        let provider = cfg.create_provider().unwrap();
        assert_eq!(provider.provider_name(), "openai");

        env::remove_var(var);
    }

    #[test]
    fn test_env_var_expansion_missing_returns_error() {
        let var = "MHOST_AI_DEFINITELY_UNSET_VAR_XYZ";
        env::remove_var(var);

        let cfg = AiConfig {
            provider: "openai".into(),
            api_key: format!("${{{var}}}"),
            model: "gpt-4o".into(),
            max_tokens: 512,
        };

        let result = cfg.create_provider();
        assert!(result.is_err(), "expected Err for missing env var");
        let err = result.err().unwrap();
        assert!(err.contains(var));
        assert!(err.contains("not set"));
    }
}
