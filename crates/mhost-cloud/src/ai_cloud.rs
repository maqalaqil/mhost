use mhost_ai::{LlmMessage, LlmProvider, LlmRequest};

use crate::remote::RemoteHost;

// ---------------------------------------------------------------------------
// AI-powered cloud operations
// ---------------------------------------------------------------------------

/// Fetch live data from a remote host and ask the LLM to diagnose its health.
pub async fn ai_diagnose_remote(
    provider: &dyn LlmProvider,
    host: &RemoteHost,
) -> Result<String, String> {
    // 1. Fetch remote process list
    let list_output = host.list_processes().await.unwrap_or_default();

    // 2. Fetch recent logs from all processes
    let logs = host
        .ssh
        .exec("tail -100 ~/.mhost/logs/*.log 2>/dev/null || echo 'No logs'")
        .await
        .map(|o| o.stdout)
        .unwrap_or_default();

    // 3. Get system info
    let sysinfo = host
        .ssh
        .exec("uname -a && uptime && free -h 2>/dev/null || vm_stat 2>/dev/null")
        .await
        .map(|o| o.stdout)
        .unwrap_or_default();

    let request = LlmRequest {
        messages: vec![
            LlmMessage {
                role: "system".into(),
                content: "You are a senior DevOps engineer. Analyze this remote server's \
                          processes, logs, and system state. Provide: 1) Overview of server \
                          health 2) Issues found 3) Recommendations. Reference specific log lines."
                    .into(),
            },
            LlmMessage {
                role: "user".into(),
                content: format!(
                    "## Server: {}\n\n### Process List\n```\n{}\n```\n\n\
                     ### Recent Logs\n```\n{}\n```\n\n### System Info\n```\n{}\n```",
                    host.name, list_output, logs, sysinfo
                ),
            },
        ],
        max_tokens: 2048,
        temperature: 0.3,
    };

    let resp = provider.complete(request).await?;
    Ok(resp.content)
}

/// Ask the LLM to recommend an infrastructure layout and produce an `mhost.toml`.
pub async fn ai_setup_infra(
    provider: &dyn LlmProvider,
    description: &str,
) -> Result<String, String> {
    let request = LlmRequest {
        messages: vec![
            LlmMessage {
                role: "system".into(),
                content: "You are an infrastructure advisor. The user describes what they need. \
                          Respond with:\n\
                          1. Recommended cloud provider and instance type\n\
                          2. Number of instances needed\n\
                          3. A complete mhost.toml config\n\
                          4. Step-by-step setup commands\n\n\
                          Be specific with instance types, regions, and pricing estimates."
                    .into(),
            },
            LlmMessage {
                role: "user".into(),
                content: description.into(),
            },
        ],
        max_tokens: 4096,
        temperature: 0.5,
    };

    let resp = provider.complete(request).await?;
    Ok(resp.content)
}

/// Compare two remote servers and produce a step-by-step migration plan.
pub async fn ai_migrate(
    provider: &dyn LlmProvider,
    from_host: &RemoteHost,
    to_host: &RemoteHost,
) -> Result<String, String> {
    let from_list = from_host.list_processes().await.unwrap_or_default();
    let to_list = to_host.list_processes().await.unwrap_or_default();

    let request = LlmRequest {
        messages: vec![
            LlmMessage {
                role: "system".into(),
                content: "You are a migration planning expert. Compare two servers and create a \
                          migration plan. Include:\n\
                          1. What processes need to move\n\
                          2. Config differences\n\
                          3. Step-by-step migration commands using mhost\n\
                          4. Rollback plan\n\
                          5. Estimated downtime"
                    .into(),
            },
            LlmMessage {
                role: "user".into(),
                content: format!(
                    "## Source: {}\n```\n{}\n```\n\n## Destination: {}\n```\n{}\n```",
                    from_host.name, from_list, to_host.name, to_list
                ),
            },
        ],
        max_tokens: 4096,
        temperature: 0.3,
    };

    let resp = provider.complete(request).await?;
    Ok(resp.content)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use mhost_ai::{LlmRequest, LlmResponse, TokenUsage};

    // -----------------------------------------------------------------------
    // Echo provider — returns the last user message as the response content,
    // so tests can verify that the correct data was injected into the prompt.
    // -----------------------------------------------------------------------

    struct EchoProvider;

    #[async_trait]
    impl LlmProvider for EchoProvider {
        async fn complete(&self, request: LlmRequest) -> Result<LlmResponse, String> {
            let user_msg = request
                .messages
                .iter()
                .rfind(|m| m.role == "user")
                .map(|m| m.content.clone())
                .unwrap_or_default();

            Ok(LlmResponse {
                content: user_msg,
                model: "echo".into(),
                usage: Some(TokenUsage {
                    input_tokens: 0,
                    output_tokens: 0,
                }),
            })
        }

        fn provider_name(&self) -> &str {
            "echo"
        }

        fn model_name(&self) -> &str {
            "echo-1"
        }
    }

    // -----------------------------------------------------------------------
    // Failing provider — simulates an API error.
    // -----------------------------------------------------------------------

    struct FailProvider;

    #[async_trait]
    impl LlmProvider for FailProvider {
        async fn complete(&self, _request: LlmRequest) -> Result<LlmResponse, String> {
            Err("provider unavailable".into())
        }

        fn provider_name(&self) -> &str {
            "fail"
        }

        fn model_name(&self) -> &str {
            "fail-1"
        }
    }

    // -----------------------------------------------------------------------
    // ai_setup_infra — request building
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_ai_setup_infra_embeds_description() {
        let provider = EchoProvider;
        let description = "I need a high-availability Node.js API with auto-scaling";
        let result = ai_setup_infra(&provider, description).await.unwrap();
        assert!(
            result.contains(description),
            "response should contain the original description"
        );
    }

    #[tokio::test]
    async fn test_ai_setup_infra_propagates_provider_error() {
        let provider = FailProvider;
        let result = ai_setup_infra(&provider, "some description").await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("provider unavailable"));
    }

    #[tokio::test]
    async fn test_ai_setup_infra_system_message_present() {
        // Capture the full request by using a provider that echoes all messages.
        struct AllMessagesEcho;

        #[async_trait]
        impl LlmProvider for AllMessagesEcho {
            async fn complete(&self, request: LlmRequest) -> Result<LlmResponse, String> {
                let combined = request
                    .messages
                    .iter()
                    .map(|m| format!("[{}]: {}", m.role, m.content))
                    .collect::<Vec<_>>()
                    .join("\n");
                Ok(LlmResponse {
                    content: combined,
                    model: "echo".into(),
                    usage: None,
                })
            }

            fn provider_name(&self) -> &str {
                "all-echo"
            }

            fn model_name(&self) -> &str {
                "all-echo-1"
            }
        }

        let provider = AllMessagesEcho;
        let result = ai_setup_infra(&provider, "run a blog").await.unwrap();
        assert!(result.contains("[system]:"), "system message must be present");
        assert!(result.contains("[user]:"), "user message must be present");
        assert!(
            result.contains("infrastructure advisor"),
            "system prompt should mention the advisor role"
        );
    }

    // -----------------------------------------------------------------------
    // ai_diagnose_remote — request parameters
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_ai_diagnose_request_max_tokens() {
        struct TokenCapture;

        #[async_trait]
        impl LlmProvider for TokenCapture {
            async fn complete(&self, request: LlmRequest) -> Result<LlmResponse, String> {
                Ok(LlmResponse {
                    content: format!("max_tokens={}", request.max_tokens),
                    model: "echo".into(),
                    usage: None,
                })
            }

            fn provider_name(&self) -> &str {
                "token-capture"
            }

            fn model_name(&self) -> &str {
                "tc-1"
            }
        }

        use crate::config::{AuthMethod, ServerConfig};
        use crate::remote::RemoteHost;

        let cfg = ServerConfig {
            host: "127.0.0.1".into(),
            port: 22,
            user: "root".into(),
            auth: AuthMethod::Key,
            key_path: None,
            tags: vec![],
            provider: None,
            instance_id: None,
            region: None,
        };
        let host = RemoteHost::new("test-host", &cfg);
        let provider = TokenCapture;

        let result = ai_diagnose_remote(&provider, &host).await.unwrap();
        assert!(
            result.contains("max_tokens=2048"),
            "diagnose should request 2048 max tokens"
        );
    }

    // -----------------------------------------------------------------------
    // ai_migrate — server names appear in prompt
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_ai_migrate_request_building() {
        use crate::config::{AuthMethod, ServerConfig};
        use crate::remote::RemoteHost;

        let make_cfg = |host: &str| ServerConfig {
            host: host.into(),
            port: 22,
            user: "root".into(),
            auth: AuthMethod::Key,
            key_path: None,
            tags: vec![],
            provider: None,
            instance_id: None,
            region: None,
        };

        let from_host = RemoteHost::new("prod-server", &make_cfg("10.0.0.1"));
        let to_host = RemoteHost::new("new-server", &make_cfg("10.0.0.2"));
        let provider = EchoProvider;

        let result = ai_migrate(&provider, &from_host, &to_host)
            .await
            .unwrap();

        assert!(
            result.contains("prod-server"),
            "prompt must reference the source server name"
        );
        assert!(
            result.contains("new-server"),
            "prompt must reference the destination server name"
        );
    }

    #[tokio::test]
    async fn test_ai_migrate_propagates_error() {
        use crate::config::{AuthMethod, ServerConfig};
        use crate::remote::RemoteHost;

        let cfg = ServerConfig {
            host: "10.0.0.1".into(),
            port: 22,
            user: "root".into(),
            auth: AuthMethod::Key,
            key_path: None,
            tags: vec![],
            provider: None,
            instance_id: None,
            region: None,
        };
        let from = RemoteHost::new("from", &cfg);
        let to = RemoteHost::new("to", &cfg);
        let provider = FailProvider;

        let result = ai_migrate(&provider, &from, &to).await;
        assert!(result.is_err());
    }
}
