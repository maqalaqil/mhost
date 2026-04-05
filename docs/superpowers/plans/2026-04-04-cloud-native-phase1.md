# Cloud-Native Phase 1: Core Types, Feature Flag, Credentials, Railway + Fly.io

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build the unified CloudAdapter trait, core types, feature flag, credential management, and the first two provider adapters (Railway + Fly.io) — the simplest APIs to validate the architecture.

**Architecture:** New `adapter/` module inside existing `mhost-cloud` crate, gated behind `cloud-native` Cargo feature flag. Each provider implements the `CloudAdapter` async trait. Credentials stored in `~/.mhost/cloud-credentials.json`. CLI gets new subcommands under `mhost cloud`.

**Tech Stack:** Rust, axum (existing), reqwest (existing), serde, async-trait, aes-gcm (secrets), tokio

**Spec:** `docs/superpowers/specs/2026-04-04-cloud-native-design.md`

---

## File Map

### New files (all inside `crates/mhost-cloud/src/`)

| File | Responsibility |
|---|---|
| `adapter/mod.rs` | `CloudAdapter` trait, `CloudService`, `ProvisionSpec`, `ServiceType`, `ServiceStatus`, `DeployConfig`, `Resources`, `CostEstimate`, `ServiceMetrics`, `CloudError` |
| `adapter/registry.rs` | `AdapterRegistry` — creates and returns the right adapter for a provider name |
| `adapter/railway.rs` | Railway adapter — GraphQL API calls |
| `adapter/fly.rs` | Fly.io adapter — Machines REST API calls |
| `credentials.rs` | `CloudCredentials` — load/save/get per-provider credentials |

### Modified files

| File | Change |
|---|---|
| `crates/mhost-cloud/Cargo.toml` | Add `[features] cloud-native`, add `aes-gcm`, `base64` as optional deps |
| `crates/mhost-cloud/src/lib.rs` | Add `#[cfg(feature = "cloud-native")] pub mod adapter; pub mod credentials;` |
| `crates/mhost-core/src/paths.rs` | Add `cloud_credentials()` path method |
| `crates/mhost-daemon/Cargo.toml` | Add `cloud-native = ["mhost-cloud/cloud-native"]` feature |
| `Cargo.toml` (workspace root) | Add `aes-gcm`, `base64` to workspace deps |
| `crates/mhost-cli/src/commands/cloud.rs` | Add `provision`, `deploy`, `scale`, `destroy`, `auth` subcommands |
| `crates/mhost-cli/tests/cli_test.rs` | Add cloud-native CLI tests |

---

## Task 1: Core Types — CloudAdapter Trait & Data Structures

**Files:**
- Create: `crates/mhost-cloud/src/adapter/mod.rs`
- Modify: `crates/mhost-cloud/Cargo.toml`
- Modify: `crates/mhost-cloud/src/lib.rs`
- Modify: `Cargo.toml` (workspace root)
- Modify: `crates/mhost-core/src/paths.rs`

- [ ] **Step 1: Add feature flag to mhost-cloud Cargo.toml**

In `crates/mhost-cloud/Cargo.toml`, add:

```toml
[features]
default = []
cloud-native = []
```

No optional deps yet — those come in later tasks.

- [ ] **Step 2: Add cloud_credentials path to MhostPaths**

In `crates/mhost-core/src/paths.rs`, add after the `webhook_failures()` method:

```rust
pub fn cloud_credentials(&self) -> PathBuf {
    self.root.join("cloud-credentials.json")
}
pub fn cloud_state(&self) -> PathBuf {
    self.root.join("cloud-state.toml")
}
pub fn cloud_backups(&self) -> PathBuf {
    self.root.join("cloud-backups")
}
pub fn cloud_cost_cache(&self) -> PathBuf {
    self.root.join("cloud-cost-cache.json")
}
```

- [ ] **Step 3: Write tests for new paths**

Add to `#[cfg(test)]` block in `paths.rs`:

```rust
#[test]
fn test_cloud_credentials_path() {
    let paths = MhostPaths::with_root(PathBuf::from("/tmp/mhost-test"));
    assert_eq!(paths.cloud_credentials(), PathBuf::from("/tmp/mhost-test/cloud-credentials.json"));
}

#[test]
fn test_cloud_state_path() {
    let paths = MhostPaths::with_root(PathBuf::from("/tmp/mhost-test"));
    assert_eq!(paths.cloud_state(), PathBuf::from("/tmp/mhost-test/cloud-state.toml"));
}

#[test]
fn test_cloud_backups_path() {
    let paths = MhostPaths::with_root(PathBuf::from("/tmp/mhost-test"));
    assert_eq!(paths.cloud_backups(), PathBuf::from("/tmp/mhost-test/cloud-backups"));
}

#[test]
fn test_cloud_cost_cache_path() {
    let paths = MhostPaths::with_root(PathBuf::from("/tmp/mhost-test"));
    assert_eq!(paths.cloud_cost_cache(), PathBuf::from("/tmp/mhost-test/cloud-cost-cache.json"));
}
```

- [ ] **Step 4: Run path tests**

Run: `cargo test -p mhost-core -- test_cloud_credentials_path test_cloud_state_path test_cloud_backups_path test_cloud_cost_cache_path`
Expected: 4 tests PASS

- [ ] **Step 5: Create adapter/mod.rs with trait and types**

Create `crates/mhost-cloud/src/adapter/mod.rs`:

```rust
#[cfg(feature = "cloud-native")]
pub mod registry;
#[cfg(feature = "cloud-native")]
pub mod railway;
#[cfg(feature = "cloud-native")]
pub mod fly;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt;

// ─── Error ──────────────────────────────────────────────────

#[derive(Debug)]
pub enum CloudError {
    NotFound(String),
    AuthError(String),
    ApiError { provider: String, status: u16, message: String },
    NetworkError(String),
    InvalidConfig(String),
    NotSupported(String),
}

impl fmt::Display for CloudError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CloudError::NotFound(msg) => write!(f, "Not found: {msg}"),
            CloudError::AuthError(msg) => write!(f, "Auth error: {msg}"),
            CloudError::ApiError { provider, status, message } => {
                write!(f, "{provider} API error ({status}): {message}")
            }
            CloudError::NetworkError(msg) => write!(f, "Network error: {msg}"),
            CloudError::InvalidConfig(msg) => write!(f, "Invalid config: {msg}"),
            CloudError::NotSupported(msg) => write!(f, "Not supported: {msg}"),
        }
    }
}

impl std::error::Error for CloudError {}

// ─── Service Types ──────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ServiceType {
    Container,
    Kubernetes,
    VM,
    Serverless,
    AppRunner,
    EdgeFunction,
    StaticSite,
}

impl fmt::Display for ServiceType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ServiceType::Container => write!(f, "container"),
            ServiceType::Kubernetes => write!(f, "kubernetes"),
            ServiceType::VM => write!(f, "vm"),
            ServiceType::Serverless => write!(f, "serverless"),
            ServiceType::AppRunner => write!(f, "app_runner"),
            ServiceType::EdgeFunction => write!(f, "edge_function"),
            ServiceType::StaticSite => write!(f, "static_site"),
        }
    }
}

impl std::str::FromStr for ServiceType {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "container" => Ok(ServiceType::Container),
            "kubernetes" | "k8s" => Ok(ServiceType::Kubernetes),
            "vm" => Ok(ServiceType::VM),
            "serverless" | "lambda" | "function" => Ok(ServiceType::Serverless),
            "app_runner" | "app-runner" => Ok(ServiceType::AppRunner),
            "edge_function" | "edge" | "worker" => Ok(ServiceType::EdgeFunction),
            "static_site" | "static" | "site" => Ok(ServiceType::StaticSite),
            _ => Err(format!("Unknown service type: {s}")),
        }
    }
}

// ─── Service Status ─────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ServiceStatus {
    Running,
    Stopped,
    Deploying,
    Failed,
    Unknown,
}

impl fmt::Display for ServiceStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ServiceStatus::Running => write!(f, "running"),
            ServiceStatus::Stopped => write!(f, "stopped"),
            ServiceStatus::Deploying => write!(f, "deploying"),
            ServiceStatus::Failed => write!(f, "failed"),
            ServiceStatus::Unknown => write!(f, "unknown"),
        }
    }
}

// ─── Resources ──────────────────────────────────────────────

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Resources {
    pub cpu: Option<String>,
    pub memory: Option<String>,
    pub disk: Option<String>,
}

// ─── CloudService ───────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CloudService {
    pub name: String,
    pub provider: String,
    pub service_type: ServiceType,
    pub region: String,
    pub status: ServiceStatus,
    pub instances: u32,
    pub url: Option<String>,
    pub image: Option<String>,
    pub resources: Resources,
    pub created_at: DateTime<Utc>,
    pub provider_id: String,
}

// ─── ProvisionSpec ──────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProvisionSpec {
    pub name: String,
    pub service_type: ServiceType,
    pub region: String,
    pub image: Option<String>,
    pub command: Option<String>,
    pub instances: u32,
    pub cpu: Option<String>,
    pub memory: Option<String>,
    pub env: HashMap<String, String>,
    pub port: Option<u16>,
    pub public: bool,
}

impl Default for ProvisionSpec {
    fn default() -> Self {
        Self {
            name: String::new(),
            service_type: ServiceType::Container,
            region: "us-east-1".into(),
            image: None,
            command: None,
            instances: 1,
            cpu: None,
            memory: None,
            env: HashMap::new(),
            port: None,
            public: true,
        }
    }
}

// ─── DeployConfig ───────────────────────────────────────────

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DeployConfig {
    pub image: Option<String>,
    pub command: Option<String>,
    pub env: HashMap<String, String>,
    pub port: Option<u16>,
    pub health_check: Option<String>,
}

// ─── Cost ───────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CostEstimate {
    pub hourly: f64,
    pub monthly: f64,
    pub currency: String,
    pub breakdown: Vec<CostLine>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CostLine {
    pub item: String,
    pub amount: f64,
}

// ─── Metrics ────────────────────────────────────────────────

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ServiceMetrics {
    pub cpu_percent: f64,
    pub memory_mb: f64,
    pub requests_per_sec: Option<f64>,
    pub error_rate: Option<f64>,
}

// ─── CloudAdapter Trait ─────────────────────────────────────

#[async_trait]
pub trait CloudAdapter: Send + Sync {
    fn provider_name(&self) -> &str;

    // Discovery
    async fn list_services(&self) -> Result<Vec<CloudService>, CloudError>;
    async fn get_service(&self, name: &str) -> Result<CloudService, CloudError>;

    // Provisioning
    async fn provision(&self, spec: ProvisionSpec) -> Result<CloudService, CloudError>;
    async fn destroy(&self, name: &str) -> Result<(), CloudError>;

    // Day-2 Operations
    async fn deploy(&self, name: &str, config: DeployConfig) -> Result<CloudService, CloudError>;
    async fn scale(&self, name: &str, instances: u32) -> Result<(), CloudError>;
    async fn restart(&self, name: &str) -> Result<(), CloudError>;

    // Observability
    async fn logs(&self, name: &str, lines: usize) -> Result<Vec<String>, CloudError>;
    async fn status(&self, name: &str) -> Result<ServiceStatus, CloudError>;
    async fn metrics(&self, name: &str) -> Result<ServiceMetrics, CloudError>;

    // Cost
    async fn estimate_cost(&self, spec: &ProvisionSpec) -> Result<CostEstimate, CloudError>;
}

// ─── Tests ──────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_service_type_from_str() {
        assert_eq!("container".parse::<ServiceType>().unwrap(), ServiceType::Container);
        assert_eq!("k8s".parse::<ServiceType>().unwrap(), ServiceType::Kubernetes);
        assert_eq!("serverless".parse::<ServiceType>().unwrap(), ServiceType::Serverless);
        assert_eq!("lambda".parse::<ServiceType>().unwrap(), ServiceType::Serverless);
        assert_eq!("edge".parse::<ServiceType>().unwrap(), ServiceType::EdgeFunction);
        assert_eq!("worker".parse::<ServiceType>().unwrap(), ServiceType::EdgeFunction);
        assert!("invalid".parse::<ServiceType>().is_err());
    }

    #[test]
    fn test_service_type_display() {
        assert_eq!(ServiceType::Container.to_string(), "container");
        assert_eq!(ServiceType::Kubernetes.to_string(), "kubernetes");
        assert_eq!(ServiceType::Serverless.to_string(), "serverless");
    }

    #[test]
    fn test_service_status_display() {
        assert_eq!(ServiceStatus::Running.to_string(), "running");
        assert_eq!(ServiceStatus::Deploying.to_string(), "deploying");
        assert_eq!(ServiceStatus::Failed.to_string(), "failed");
    }

    #[test]
    fn test_provision_spec_default() {
        let spec = ProvisionSpec::default();
        assert_eq!(spec.service_type, ServiceType::Container);
        assert_eq!(spec.region, "us-east-1");
        assert_eq!(spec.instances, 1);
        assert!(spec.public);
    }

    #[test]
    fn test_cloud_error_display() {
        let err = CloudError::ApiError {
            provider: "railway".into(),
            status: 401,
            message: "Unauthorized".into(),
        };
        assert!(err.to_string().contains("railway"));
        assert!(err.to_string().contains("401"));
    }

    #[test]
    fn test_cloud_service_serialize() {
        let service = CloudService {
            name: "api".into(),
            provider: "railway".into(),
            service_type: ServiceType::Container,
            region: "us-east-1".into(),
            status: ServiceStatus::Running,
            instances: 2,
            url: Some("https://api.up.railway.app".into()),
            image: Some("node:20".into()),
            resources: Resources { cpu: Some("1".into()), memory: Some("512MB".into()), disk: None },
            created_at: Utc::now(),
            provider_id: "srv_abc123".into(),
        };
        let json = serde_json::to_string(&service).unwrap();
        assert!(json.contains("railway"));
        assert!(json.contains("container"));
        let deserialized: CloudService = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.name, "api");
        assert_eq!(deserialized.instances, 2);
    }
}
```

- [ ] **Step 6: Add adapter module to lib.rs**

In `crates/mhost-cloud/src/lib.rs`, add after the existing module declarations:

```rust
#[cfg(feature = "cloud-native")]
pub mod adapter;
#[cfg(feature = "cloud-native")]
pub mod credentials;
```

Create a stub for `credentials.rs`:
```rust
// credentials.rs — implemented in Task 2
```

Create stubs for `adapter/registry.rs`, `adapter/railway.rs`, `adapter/fly.rs`:
```rust
// stub — implemented in later tasks
```

- [ ] **Step 7: Run tests**

Run: `cargo test -p mhost-cloud --features cloud-native -- adapter`
Expected: 6 tests PASS

Run: `cargo test -p mhost-core -- test_cloud`
Expected: 4 tests PASS

- [ ] **Step 8: Commit**

```bash
git add crates/mhost-cloud/ crates/mhost-core/src/paths.rs
git commit -m "feat(cloud): add CloudAdapter trait, core types, feature flag"
```

---

## Task 2: Credential Management

**Files:**
- Replace: `crates/mhost-cloud/src/credentials.rs`

- [ ] **Step 1: Write credentials.rs**

Replace the stub `crates/mhost-cloud/src/credentials.rs`:

```rust
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CloudCredentials {
    pub providers: HashMap<String, ProviderCredential>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ProviderCredential {
    Token {
        token: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        default_region: Option<String>,
    },
    AwsKeys {
        access_key_id: String,
        secret_access_key: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        default_region: Option<String>,
    },
    AzureServicePrincipal {
        client_id: String,
        client_secret: String,
        tenant_id: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        subscription_id: Option<String>,
    },
    GcpServiceAccount {
        credentials_file: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        default_region: Option<String>,
    },
}

impl ProviderCredential {
    pub fn token(token: &str) -> Self {
        ProviderCredential::Token {
            token: token.to_string(),
            default_region: None,
        }
    }

    pub fn get_token(&self) -> Option<&str> {
        match self {
            ProviderCredential::Token { token, .. } => Some(token),
            _ => None,
        }
    }

    pub fn default_region(&self) -> Option<&str> {
        match self {
            ProviderCredential::Token { default_region, .. } => default_region.as_deref(),
            ProviderCredential::AwsKeys { default_region, .. } => default_region.as_deref(),
            ProviderCredential::GcpServiceAccount { default_region, .. } => default_region.as_deref(),
            ProviderCredential::AzureServicePrincipal { .. } => None,
        }
    }
}

impl CloudCredentials {
    pub fn load(path: &Path) -> Result<Self, String> {
        if !path.exists() {
            return Ok(Self::default());
        }
        let data = std::fs::read_to_string(path)
            .map_err(|e| format!("Failed to read credentials: {e}"))?;
        serde_json::from_str(&data)
            .map_err(|e| format!("Failed to parse credentials: {e}"))
    }

    pub fn save(&self, path: &Path) -> Result<(), String> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("Failed to create directory: {e}"))?;
        }
        let data = serde_json::to_string_pretty(self)
            .map_err(|e| format!("Failed to serialize credentials: {e}"))?;
        std::fs::write(path, data)
            .map_err(|e| format!("Failed to write credentials: {e}"))?;
        // Restrict permissions on Unix
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let perms = std::fs::Permissions::from_mode(0o600);
            let _ = std::fs::set_permissions(path, perms);
        }
        Ok(())
    }

    pub fn get(&self, provider: &str) -> Option<&ProviderCredential> {
        self.providers.get(provider)
    }

    pub fn set(&mut self, provider: &str, credential: ProviderCredential) {
        self.providers.insert(provider.to_string(), credential);
    }

    pub fn remove(&mut self, provider: &str) -> bool {
        self.providers.remove(provider).is_some()
    }

    pub fn list_providers(&self) -> Vec<&str> {
        self.providers.keys().map(|s| s.as_str()).collect()
    }

    /// Try to get credential from stored config, falling back to env vars.
    pub fn resolve(&self, provider: &str) -> Option<ProviderCredential> {
        // Check stored credentials first
        if let Some(cred) = self.providers.get(provider) {
            return Some(cred.clone());
        }
        // Fall back to environment variables
        match provider {
            "railway" => std::env::var("RAILWAY_TOKEN").ok()
                .map(|t| ProviderCredential::token(&t)),
            "fly" => std::env::var("FLY_API_TOKEN").ok()
                .map(|t| ProviderCredential::token(&t)),
            "vercel" => std::env::var("VERCEL_TOKEN").ok()
                .map(|t| ProviderCredential::token(&t)),
            "digitalocean" | "do" => std::env::var("DIGITALOCEAN_TOKEN").ok()
                .map(|t| ProviderCredential::token(&t)),
            "cloudflare" => std::env::var("CLOUDFLARE_API_TOKEN").ok()
                .map(|t| ProviderCredential::token(&t)),
            "netlify" => std::env::var("NETLIFY_TOKEN").ok()
                .map(|t| ProviderCredential::token(&t)),
            "supabase" => std::env::var("SUPABASE_ACCESS_TOKEN").ok()
                .map(|t| ProviderCredential::token(&t)),
            "aws" => {
                let key = std::env::var("AWS_ACCESS_KEY_ID").ok()?;
                let secret = std::env::var("AWS_SECRET_ACCESS_KEY").ok()?;
                Some(ProviderCredential::AwsKeys {
                    access_key_id: key,
                    secret_access_key: secret,
                    default_region: std::env::var("AWS_DEFAULT_REGION").ok(),
                })
            }
            "gcp" | "google" => std::env::var("GOOGLE_APPLICATION_CREDENTIALS").ok()
                .map(|f| ProviderCredential::GcpServiceAccount {
                    credentials_file: f,
                    default_region: std::env::var("GCP_DEFAULT_REGION").ok(),
                }),
            "azure" => {
                let client_id = std::env::var("AZURE_CLIENT_ID").ok()?;
                let client_secret = std::env::var("AZURE_CLIENT_SECRET").ok()?;
                let tenant_id = std::env::var("AZURE_TENANT_ID").ok()?;
                Some(ProviderCredential::AzureServicePrincipal {
                    client_id,
                    client_secret,
                    tenant_id,
                    subscription_id: std::env::var("AZURE_SUBSCRIPTION_ID").ok(),
                })
            }
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_load_empty() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("creds.json");
        let creds = CloudCredentials::load(&path).unwrap();
        assert!(creds.providers.is_empty());
    }

    #[test]
    fn test_save_and_load() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("creds.json");
        let mut creds = CloudCredentials::default();
        creds.set("railway", ProviderCredential::token("test-token-123"));
        creds.save(&path).unwrap();

        let loaded = CloudCredentials::load(&path).unwrap();
        assert_eq!(loaded.list_providers().len(), 1);
        let railway = loaded.get("railway").unwrap();
        assert_eq!(railway.get_token().unwrap(), "test-token-123");
    }

    #[test]
    fn test_set_and_remove() {
        let mut creds = CloudCredentials::default();
        creds.set("fly", ProviderCredential::token("fly-tok"));
        assert_eq!(creds.list_providers().len(), 1);
        assert!(creds.remove("fly"));
        assert!(creds.providers.is_empty());
        assert!(!creds.remove("fly")); // Already removed
    }

    #[test]
    fn test_get_token() {
        let cred = ProviderCredential::token("abc");
        assert_eq!(cred.get_token().unwrap(), "abc");
        let aws = ProviderCredential::AwsKeys {
            access_key_id: "k".into(),
            secret_access_key: "s".into(),
            default_region: None,
        };
        assert!(aws.get_token().is_none());
    }

    #[test]
    fn test_default_region() {
        let cred = ProviderCredential::Token {
            token: "tok".into(),
            default_region: Some("eu-west-1".into()),
        };
        assert_eq!(cred.default_region().unwrap(), "eu-west-1");
    }

    #[test]
    fn test_serde_roundtrip() {
        let mut creds = CloudCredentials::default();
        creds.set("railway", ProviderCredential::token("r-tok"));
        creds.set("aws", ProviderCredential::AwsKeys {
            access_key_id: "AKIA".into(),
            secret_access_key: "secret".into(),
            default_region: Some("us-east-1".into()),
        });
        let json = serde_json::to_string(&creds).unwrap();
        let loaded: CloudCredentials = serde_json::from_str(&json).unwrap();
        assert_eq!(loaded.list_providers().len(), 2);
    }
}
```

- [ ] **Step 2: Run tests**

Run: `cargo test -p mhost-cloud --features cloud-native -- credentials`
Expected: 6 tests PASS

- [ ] **Step 3: Commit**

```bash
git add crates/mhost-cloud/src/credentials.rs
git commit -m "feat(cloud): add credential management with env var fallback"
```

---

## Task 3: Railway Adapter

**Files:**
- Replace: `crates/mhost-cloud/src/adapter/railway.rs`

- [ ] **Step 1: Write Railway adapter**

Replace stub `crates/mhost-cloud/src/adapter/railway.rs`:

```rust
use async_trait::async_trait;
use chrono::Utc;
use reqwest::Client;
use serde::Deserialize;
use tracing::{info, warn};

use super::{
    CloudAdapter, CloudError, CloudService, CostEstimate, CostLine, DeployConfig,
    ProvisionSpec, Resources, ServiceMetrics, ServiceStatus, ServiceType,
};

const RAILWAY_API: &str = "https://backboard.railway.app/graphql/v2";

pub struct RailwayAdapter {
    token: String,
    client: Client,
    project_id: Option<String>,
}

impl RailwayAdapter {
    pub fn new(token: &str) -> Self {
        Self {
            token: token.to_string(),
            client: Client::new(),
            project_id: None,
        }
    }

    pub fn with_project(mut self, project_id: &str) -> Self {
        self.project_id = Some(project_id.to_string());
        self
    }

    async fn graphql(&self, query: &str, variables: serde_json::Value) -> Result<serde_json::Value, CloudError> {
        let body = serde_json::json!({
            "query": query,
            "variables": variables,
        });

        let resp = self.client
            .post(RAILWAY_API)
            .header("Authorization", format!("Bearer {}", self.token))
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|e| CloudError::NetworkError(e.to_string()))?;

        let status = resp.status().as_u16();
        if status == 401 {
            return Err(CloudError::AuthError("Invalid Railway token".into()));
        }

        let data: serde_json::Value = resp.json().await
            .map_err(|e| CloudError::NetworkError(e.to_string()))?;

        if let Some(errors) = data.get("errors") {
            let msg = errors[0]["message"].as_str().unwrap_or("Unknown error");
            return Err(CloudError::ApiError {
                provider: "railway".into(),
                status,
                message: msg.into(),
            });
        }

        Ok(data["data"].clone())
    }

    fn parse_service(&self, svc: &serde_json::Value, project_id: &str) -> CloudService {
        let name = svc["name"].as_str().unwrap_or("unknown").to_string();
        let id = svc["id"].as_str().unwrap_or("").to_string();

        let status = match svc["status"].as_str() {
            Some("ACTIVE") | Some("SUCCESS") => ServiceStatus::Running,
            Some("BUILDING") | Some("DEPLOYING") => ServiceStatus::Deploying,
            Some("FAILED") | Some("CRASHED") => ServiceStatus::Failed,
            Some("REMOVED") => ServiceStatus::Stopped,
            _ => ServiceStatus::Unknown,
        };

        let url = svc["serviceInstances"]["edges"]
            .as_array()
            .and_then(|edges| edges.first())
            .and_then(|edge| edge["node"]["domains"]["serviceDomains"].as_array())
            .and_then(|domains| domains.first())
            .and_then(|d| d["domain"].as_str())
            .map(|d| format!("https://{d}"));

        CloudService {
            name,
            provider: "railway".into(),
            service_type: ServiceType::Container,
            region: svc["region"].as_str().unwrap_or("us-east-1").to_string(),
            status,
            instances: 1,
            url,
            image: svc["source"]["image"].as_str().map(String::from),
            resources: Resources::default(),
            created_at: Utc::now(),
            provider_id: format!("{project_id}/{id}"),
        }
    }
}

#[async_trait]
impl CloudAdapter for RailwayAdapter {
    fn provider_name(&self) -> &str {
        "railway"
    }

    async fn list_services(&self) -> Result<Vec<CloudService>, CloudError> {
        let query = r#"
            query { me { projects { edges { node {
                id name services { edges { node {
                    id name status region
                    source { image }
                    serviceInstances { edges { node {
                        domains { serviceDomains { domain } }
                    }}}
                }}}
            }}}}}
        "#;

        let data = self.graphql(query, serde_json::json!({})).await?;
        let mut services = Vec::new();

        if let Some(projects) = data["me"]["projects"]["edges"].as_array() {
            for project in projects {
                let project_id = project["node"]["id"].as_str().unwrap_or("");
                if let Some(filter_id) = &self.project_id {
                    if project_id != filter_id { continue; }
                }
                if let Some(svcs) = project["node"]["services"]["edges"].as_array() {
                    for svc in svcs {
                        services.push(self.parse_service(&svc["node"], project_id));
                    }
                }
            }
        }

        Ok(services)
    }

    async fn get_service(&self, name: &str) -> Result<CloudService, CloudError> {
        let services = self.list_services().await?;
        services.into_iter()
            .find(|s| s.name == name)
            .ok_or_else(|| CloudError::NotFound(format!("Service '{name}' not found on Railway")))
    }

    async fn provision(&self, spec: ProvisionSpec) -> Result<CloudService, CloudError> {
        // Step 1: Create project if needed
        let project_id = if let Some(id) = &self.project_id {
            id.clone()
        } else {
            let query = r#"mutation($input: ProjectCreateInput!) {
                projectCreate(input: $input) { id }
            }"#;
            let data = self.graphql(query, serde_json::json!({
                "input": { "name": spec.name }
            })).await?;
            data["projectCreate"]["id"].as_str()
                .ok_or_else(|| CloudError::ApiError {
                    provider: "railway".into(), status: 500,
                    message: "Failed to create project".into(),
                })?.to_string()
        };

        // Step 2: Create service
        let query = r#"mutation($input: ServiceCreateInput!) {
            serviceCreate(input: $input) { id name }
        }"#;
        let mut input = serde_json::json!({
            "projectId": project_id,
            "name": spec.name,
        });
        if let Some(ref image) = spec.image {
            input["source"] = serde_json::json!({ "image": image });
        }
        let data = self.graphql(query, serde_json::json!({ "input": input })).await?;

        let service_id = data["serviceCreate"]["id"].as_str().unwrap_or("").to_string();
        info!(provider = "railway", service = %spec.name, "Service provisioned");

        // Step 3: Set env vars
        if !spec.env.is_empty() {
            let query = r#"mutation($input: VariableCollectionUpsertInput!) {
                variableCollectionUpsert(input: $input)
            }"#;
            let _ = self.graphql(query, serde_json::json!({
                "input": {
                    "projectId": project_id,
                    "serviceId": service_id,
                    "environmentId": serde_json::Value::Null,
                    "variables": spec.env,
                }
            })).await;
        }

        Ok(CloudService {
            name: spec.name,
            provider: "railway".into(),
            service_type: ServiceType::Container,
            region: spec.region,
            status: ServiceStatus::Deploying,
            instances: spec.instances,
            url: None,
            image: spec.image,
            resources: Resources { cpu: spec.cpu, memory: spec.memory, disk: None },
            created_at: Utc::now(),
            provider_id: format!("{project_id}/{service_id}"),
        })
    }

    async fn destroy(&self, name: &str) -> Result<(), CloudError> {
        let service = self.get_service(name).await?;
        let parts: Vec<&str> = service.provider_id.split('/').collect();
        if parts.len() != 2 {
            return Err(CloudError::InvalidConfig("Invalid provider_id format".into()));
        }
        let query = r#"mutation($id: String!) { serviceDelete(id: $id) }"#;
        self.graphql(query, serde_json::json!({ "id": parts[1] })).await?;
        info!(provider = "railway", service = %name, "Service destroyed");
        Ok(())
    }

    async fn deploy(&self, name: &str, config: DeployConfig) -> Result<CloudService, CloudError> {
        let service = self.get_service(name).await?;
        let parts: Vec<&str> = service.provider_id.split('/').collect();
        if parts.len() != 2 {
            return Err(CloudError::InvalidConfig("Invalid provider_id".into()));
        }

        if let Some(ref image) = config.image {
            let query = r#"mutation($input: ServiceUpdateInput!) {
                serviceUpdate(input: $input) { id }
            }"#;
            self.graphql(query, serde_json::json!({
                "input": {
                    "id": parts[1],
                    "source": { "image": image },
                }
            })).await?;
        }

        if !config.env.is_empty() {
            let query = r#"mutation($input: VariableCollectionUpsertInput!) {
                variableCollectionUpsert(input: $input)
            }"#;
            let _ = self.graphql(query, serde_json::json!({
                "input": {
                    "projectId": parts[0],
                    "serviceId": parts[1],
                    "environmentId": serde_json::Value::Null,
                    "variables": config.env,
                }
            })).await;
        }

        info!(provider = "railway", service = %name, "Deploy triggered");
        self.get_service(name).await
    }

    async fn scale(&self, name: &str, instances: u32) -> Result<(), CloudError> {
        let service = self.get_service(name).await?;
        let parts: Vec<&str> = service.provider_id.split('/').collect();
        if parts.len() != 2 {
            return Err(CloudError::InvalidConfig("Invalid provider_id".into()));
        }
        let query = r#"mutation($input: ServiceUpdateInput!) {
            serviceUpdate(input: $input) { id }
        }"#;
        self.graphql(query, serde_json::json!({
            "input": {
                "id": parts[1],
                "numReplicas": instances,
            }
        })).await?;
        info!(provider = "railway", service = %name, instances, "Scaled");
        Ok(())
    }

    async fn restart(&self, name: &str) -> Result<(), CloudError> {
        let service = self.get_service(name).await?;
        let parts: Vec<&str> = service.provider_id.split('/').collect();
        if parts.len() != 2 {
            return Err(CloudError::InvalidConfig("Invalid provider_id".into()));
        }
        let query = r#"mutation($id: String!) { serviceRestart(id: $id) }"#;
        self.graphql(query, serde_json::json!({ "id": parts[1] })).await?;
        info!(provider = "railway", service = %name, "Restarted");
        Ok(())
    }

    async fn logs(&self, name: &str, lines: usize) -> Result<Vec<String>, CloudError> {
        let service = self.get_service(name).await?;
        let parts: Vec<&str> = service.provider_id.split('/').collect();
        if parts.len() != 2 {
            return Err(CloudError::InvalidConfig("Invalid provider_id".into()));
        }
        let query = r#"query($input: DeploymentLogsInput!) {
            deploymentLogs(input: $input) { message timestamp }
        }"#;
        let data = self.graphql(query, serde_json::json!({
            "input": {
                "serviceId": parts[1],
                "limit": lines,
            }
        })).await?;

        let logs = data["deploymentLogs"].as_array()
            .map(|arr| arr.iter().filter_map(|l| l["message"].as_str().map(String::from)).collect())
            .unwrap_or_default();
        Ok(logs)
    }

    async fn status(&self, name: &str) -> Result<ServiceStatus, CloudError> {
        let service = self.get_service(name).await?;
        Ok(service.status)
    }

    async fn metrics(&self, _name: &str) -> Result<ServiceMetrics, CloudError> {
        // Railway doesn't expose metrics via API currently
        Err(CloudError::NotSupported("Railway does not expose metrics via API".into()))
    }

    async fn estimate_cost(&self, spec: &ProvisionSpec) -> Result<CostEstimate, CloudError> {
        // Railway pricing: ~$5/month per 0.5 vCPU + 512MB
        let cpu_factor = spec.cpu.as_deref().unwrap_or("0.5").parse::<f64>().unwrap_or(0.5);
        let monthly = cpu_factor * 10.0 * spec.instances as f64;
        Ok(CostEstimate {
            hourly: monthly / 730.0,
            monthly,
            currency: "USD".into(),
            breakdown: vec![
                CostLine { item: format!("{}x container ({} vCPU)", spec.instances, cpu_factor), amount: monthly },
            ],
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_railway_adapter_creation() {
        let adapter = RailwayAdapter::new("test-token");
        assert_eq!(adapter.provider_name(), "railway");
    }

    #[test]
    fn test_estimate_cost() {
        let adapter = RailwayAdapter::new("test");
        let spec = ProvisionSpec {
            name: "api".into(),
            instances: 2,
            cpu: Some("1".into()),
            ..Default::default()
        };
        let rt = tokio::runtime::Runtime::new().unwrap();
        let cost = rt.block_on(adapter.estimate_cost(&spec)).unwrap();
        assert_eq!(cost.monthly, 20.0); // 1 cpu * 10 * 2 instances
        assert_eq!(cost.currency, "USD");
    }
}
```

- [ ] **Step 2: Run tests**

Run: `cargo test -p mhost-cloud --features cloud-native -- railway`
Expected: 2 tests PASS

- [ ] **Step 3: Commit**

```bash
git add crates/mhost-cloud/src/adapter/railway.rs
git commit -m "feat(cloud): add Railway adapter with GraphQL API"
```

---

## Task 4: Fly.io Adapter

**Files:**
- Replace: `crates/mhost-cloud/src/adapter/fly.rs`

- [ ] **Step 1: Write Fly.io adapter**

Replace stub `crates/mhost-cloud/src/adapter/fly.rs`:

```rust
use async_trait::async_trait;
use chrono::Utc;
use reqwest::Client;
use tracing::info;

use super::{
    CloudAdapter, CloudError, CloudService, CostEstimate, CostLine, DeployConfig,
    ProvisionSpec, Resources, ServiceMetrics, ServiceStatus, ServiceType,
};

const FLY_API: &str = "https://api.machines.dev/v1";

pub struct FlyAdapter {
    token: String,
    client: Client,
    org: String,
}

impl FlyAdapter {
    pub fn new(token: &str) -> Self {
        Self {
            token: token.to_string(),
            client: Client::new(),
            org: "personal".to_string(),
        }
    }

    pub fn with_org(mut self, org: &str) -> Self {
        self.org = org.to_string();
        self
    }

    async fn api_get(&self, path: &str) -> Result<serde_json::Value, CloudError> {
        let url = format!("{FLY_API}{path}");
        let resp = self.client.get(&url)
            .header("Authorization", format!("Bearer {}", self.token))
            .send()
            .await
            .map_err(|e| CloudError::NetworkError(e.to_string()))?;

        let status = resp.status().as_u16();
        if status == 401 { return Err(CloudError::AuthError("Invalid Fly.io token".into())); }
        if status == 404 { return Err(CloudError::NotFound(format!("Not found: {path}"))); }

        let data: serde_json::Value = resp.json().await
            .map_err(|e| CloudError::NetworkError(e.to_string()))?;

        if status >= 400 {
            let msg = data["error"].as_str().unwrap_or("Unknown error");
            return Err(CloudError::ApiError { provider: "fly".into(), status, message: msg.into() });
        }
        Ok(data)
    }

    async fn api_post(&self, path: &str, body: serde_json::Value) -> Result<serde_json::Value, CloudError> {
        let url = format!("{FLY_API}{path}");
        let resp = self.client.post(&url)
            .header("Authorization", format!("Bearer {}", self.token))
            .json(&body)
            .send()
            .await
            .map_err(|e| CloudError::NetworkError(e.to_string()))?;

        let status = resp.status().as_u16();
        if status == 401 { return Err(CloudError::AuthError("Invalid Fly.io token".into())); }

        let data: serde_json::Value = resp.json().await
            .map_err(|e| CloudError::NetworkError(e.to_string()))?;

        if status >= 400 {
            let msg = data["error"].as_str().unwrap_or("Unknown error");
            return Err(CloudError::ApiError { provider: "fly".into(), status, message: msg.into() });
        }
        Ok(data)
    }

    async fn api_delete(&self, path: &str) -> Result<(), CloudError> {
        let url = format!("{FLY_API}{path}");
        let resp = self.client.delete(&url)
            .header("Authorization", format!("Bearer {}", self.token))
            .send()
            .await
            .map_err(|e| CloudError::NetworkError(e.to_string()))?;

        let status = resp.status().as_u16();
        if status == 401 { return Err(CloudError::AuthError("Invalid Fly.io token".into())); }
        if status >= 400 && status != 404 {
            return Err(CloudError::ApiError { provider: "fly".into(), status, message: "Delete failed".into() });
        }
        Ok(())
    }

    fn parse_machine(&self, app_name: &str, machine: &serde_json::Value) -> CloudService {
        let status = match machine["state"].as_str() {
            Some("started") | Some("running") => ServiceStatus::Running,
            Some("stopped") => ServiceStatus::Stopped,
            Some("created") | Some("starting") => ServiceStatus::Deploying,
            _ => ServiceStatus::Unknown,
        };

        let config = &machine["config"];
        let image = config["image"].as_str().map(String::from);

        let resources = Resources {
            cpu: config["guest"]["cpus"].as_u64().map(|c| c.to_string()),
            memory: config["guest"]["memory_mb"].as_u64().map(|m| format!("{m}MB")),
            disk: None,
        };

        CloudService {
            name: app_name.to_string(),
            provider: "fly".into(),
            service_type: ServiceType::Container,
            region: machine["region"].as_str().unwrap_or("").to_string(),
            status,
            instances: 1,
            url: Some(format!("https://{app_name}.fly.dev")),
            image,
            resources,
            created_at: Utc::now(),
            provider_id: machine["id"].as_str().unwrap_or("").to_string(),
        }
    }
}

#[async_trait]
impl CloudAdapter for FlyAdapter {
    fn provider_name(&self) -> &str {
        "fly"
    }

    async fn list_services(&self) -> Result<Vec<CloudService>, CloudError> {
        let data = self.api_get(&format!("/apps?org_slug={}", self.org)).await?;
        let apps = data.as_array().unwrap_or(&vec![]);
        let mut services = Vec::new();

        for app in apps {
            let app_name = app["name"].as_str().unwrap_or("");
            if app_name.is_empty() { continue; }

            let status = match app["status"].as_str() {
                Some("deployed") => ServiceStatus::Running,
                Some("suspended") => ServiceStatus::Stopped,
                Some("pending") => ServiceStatus::Deploying,
                _ => ServiceStatus::Unknown,
            };

            services.push(CloudService {
                name: app_name.to_string(),
                provider: "fly".into(),
                service_type: ServiceType::Container,
                region: app["currentRelease"]["region"].as_str().unwrap_or("").to_string(),
                status,
                instances: app["machineCount"].as_u64().unwrap_or(0) as u32,
                url: Some(format!("https://{app_name}.fly.dev")),
                image: None,
                resources: Resources::default(),
                created_at: Utc::now(),
                provider_id: app["id"].as_str().unwrap_or("").to_string(),
            });
        }
        Ok(services)
    }

    async fn get_service(&self, name: &str) -> Result<CloudService, CloudError> {
        let machines = self.api_get(&format!("/apps/{name}/machines")).await?;
        let machine_list = machines.as_array()
            .ok_or_else(|| CloudError::NotFound(format!("App '{name}' not found on Fly.io")))?;

        if machine_list.is_empty() {
            return Err(CloudError::NotFound(format!("No machines for app '{name}'")));
        }

        let mut service = self.parse_machine(name, &machine_list[0]);
        service.instances = machine_list.len() as u32;
        Ok(service)
    }

    async fn provision(&self, spec: ProvisionSpec) -> Result<CloudService, CloudError> {
        // Step 1: Create app
        let app_data = self.api_post("/apps", serde_json::json!({
            "app_name": spec.name,
            "org_slug": self.org,
        })).await?;

        // Step 2: Create machine
        let mut machine_config = serde_json::json!({
            "config": {
                "image": spec.image.as_deref().unwrap_or("nginx:latest"),
                "guest": {
                    "cpus": spec.cpu.as_deref().unwrap_or("1").parse::<u32>().unwrap_or(1),
                    "memory_mb": parse_memory_mb(spec.memory.as_deref().unwrap_or("256MB")),
                    "cpu_kind": "shared",
                },
                "env": spec.env,
            },
            "region": spec.region,
        });

        if let Some(port) = spec.port {
            machine_config["config"]["services"] = serde_json::json!([{
                "ports": [{ "port": port, "handlers": ["http"] }],
                "protocol": "tcp",
                "internal_port": port,
            }]);
        }

        let machine = self.api_post(&format!("/apps/{}/machines", spec.name), machine_config).await?;

        info!(provider = "fly", app = %spec.name, "Machine provisioned");

        Ok(CloudService {
            name: spec.name,
            provider: "fly".into(),
            service_type: ServiceType::Container,
            region: spec.region,
            status: ServiceStatus::Deploying,
            instances: 1,
            url: Some(format!("https://{}.fly.dev", app_data["name"].as_str().unwrap_or(""))),
            image: spec.image,
            resources: Resources { cpu: spec.cpu, memory: spec.memory, disk: None },
            created_at: Utc::now(),
            provider_id: machine["id"].as_str().unwrap_or("").to_string(),
        })
    }

    async fn destroy(&self, name: &str) -> Result<(), CloudError> {
        self.api_delete(&format!("/apps/{name}")).await?;
        info!(provider = "fly", app = %name, "App destroyed");
        Ok(())
    }

    async fn deploy(&self, name: &str, config: DeployConfig) -> Result<CloudService, CloudError> {
        let machines = self.api_get(&format!("/apps/{name}/machines")).await?;
        let machine_list = machines.as_array()
            .ok_or_else(|| CloudError::NotFound(format!("App '{name}' not found")))?;

        for machine in machine_list {
            let machine_id = machine["id"].as_str().unwrap_or("");
            let mut update = machine["config"].clone();
            if let Some(ref image) = config.image {
                update["image"] = serde_json::json!(image);
            }
            if !config.env.is_empty() {
                let existing_env = update["env"].as_object().cloned().unwrap_or_default();
                let mut merged = existing_env;
                for (k, v) in &config.env {
                    merged.insert(k.clone(), serde_json::json!(v));
                }
                update["env"] = serde_json::json!(merged);
            }

            self.api_post(
                &format!("/apps/{name}/machines/{machine_id}"),
                serde_json::json!({ "config": update }),
            ).await?;
        }

        info!(provider = "fly", app = %name, "Deploy complete");
        self.get_service(name).await
    }

    async fn scale(&self, name: &str, instances: u32) -> Result<(), CloudError> {
        let machines = self.api_get(&format!("/apps/{name}/machines")).await?;
        let machine_list = machines.as_array()
            .ok_or_else(|| CloudError::NotFound(format!("App '{name}' not found")))?;

        let current = machine_list.len() as u32;
        if instances > current {
            // Scale up: clone first machine config
            if let Some(template) = machine_list.first() {
                let config = template["config"].clone();
                let region = template["region"].as_str().unwrap_or("iad");
                for _ in 0..(instances - current) {
                    self.api_post(&format!("/apps/{name}/machines"), serde_json::json!({
                        "config": config,
                        "region": region,
                    })).await?;
                }
            }
        } else if instances < current {
            // Scale down: stop and destroy excess machines
            for machine in machine_list.iter().skip(instances as usize) {
                let id = machine["id"].as_str().unwrap_or("");
                let _ = self.api_delete(&format!("/apps/{name}/machines/{id}")).await;
            }
        }

        info!(provider = "fly", app = %name, from = current, to = instances, "Scaled");
        Ok(())
    }

    async fn restart(&self, name: &str) -> Result<(), CloudError> {
        let machines = self.api_get(&format!("/apps/{name}/machines")).await?;
        let machine_list = machines.as_array()
            .ok_or_else(|| CloudError::NotFound(format!("App '{name}' not found")))?;

        for machine in machine_list {
            let id = machine["id"].as_str().unwrap_or("");
            let _ = self.api_post(&format!("/apps/{name}/machines/{id}/restart"), serde_json::json!({})).await;
        }

        info!(provider = "fly", app = %name, "Restarted all machines");
        Ok(())
    }

    async fn logs(&self, _name: &str, _lines: usize) -> Result<Vec<String>, CloudError> {
        // Fly logs require the Nats-based log stream, not available via REST
        Err(CloudError::NotSupported("Use 'flyctl logs' for Fly.io log streaming".into()))
    }

    async fn status(&self, name: &str) -> Result<ServiceStatus, CloudError> {
        let service = self.get_service(name).await?;
        Ok(service.status)
    }

    async fn metrics(&self, _name: &str) -> Result<ServiceMetrics, CloudError> {
        Err(CloudError::NotSupported("Fly.io metrics available via Prometheus endpoint only".into()))
    }

    async fn estimate_cost(&self, spec: &ProvisionSpec) -> Result<CostEstimate, CloudError> {
        // Fly.io pricing: shared-cpu-1x ~$1.94/mo, performance-1x ~$29/mo
        let cpus = spec.cpu.as_deref().unwrap_or("1").parse::<f64>().unwrap_or(1.0);
        let memory_mb = parse_memory_mb(spec.memory.as_deref().unwrap_or("256MB")) as f64;
        let cpu_cost = cpus * 1.94 * spec.instances as f64;
        let mem_cost = (memory_mb / 256.0) * 1.94 * spec.instances as f64;
        let monthly = cpu_cost + mem_cost;
        Ok(CostEstimate {
            hourly: monthly / 730.0,
            monthly,
            currency: "USD".into(),
            breakdown: vec![
                CostLine { item: format!("{}x {cpus} vCPU", spec.instances), amount: cpu_cost },
                CostLine { item: format!("{}x {memory_mb}MB RAM", spec.instances), amount: mem_cost },
            ],
        })
    }
}

fn parse_memory_mb(s: &str) -> u32 {
    let s = s.trim().to_uppercase();
    if let Some(gb) = s.strip_suffix("GB") {
        gb.trim().parse::<u32>().unwrap_or(1) * 1024
    } else if let Some(mb) = s.strip_suffix("MB") {
        mb.trim().parse::<u32>().unwrap_or(256)
    } else {
        s.parse::<u32>().unwrap_or(256)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fly_adapter_creation() {
        let adapter = FlyAdapter::new("test-token");
        assert_eq!(adapter.provider_name(), "fly");
    }

    #[test]
    fn test_parse_memory_mb() {
        assert_eq!(parse_memory_mb("256MB"), 256);
        assert_eq!(parse_memory_mb("1GB"), 1024);
        assert_eq!(parse_memory_mb("512mb"), 512);
        assert_eq!(parse_memory_mb("2GB"), 2048);
        assert_eq!(parse_memory_mb("256"), 256);
    }

    #[test]
    fn test_estimate_cost() {
        let adapter = FlyAdapter::new("test");
        let spec = ProvisionSpec {
            name: "api".into(),
            instances: 2,
            cpu: Some("1".into()),
            memory: Some("512MB".into()),
            ..Default::default()
        };
        let rt = tokio::runtime::Runtime::new().unwrap();
        let cost = rt.block_on(adapter.estimate_cost(&spec)).unwrap();
        assert!(cost.monthly > 0.0);
        assert_eq!(cost.currency, "USD");
        assert_eq!(cost.breakdown.len(), 2);
    }
}
```

- [ ] **Step 2: Run tests**

Run: `cargo test -p mhost-cloud --features cloud-native -- fly`
Expected: 3 tests PASS

- [ ] **Step 3: Commit**

```bash
git add crates/mhost-cloud/src/adapter/fly.rs
git commit -m "feat(cloud): add Fly.io Machines adapter with REST API"
```

---

## Task 5: Adapter Registry

**Files:**
- Replace: `crates/mhost-cloud/src/adapter/registry.rs`

- [ ] **Step 1: Write registry**

Replace stub `crates/mhost-cloud/src/adapter/registry.rs`:

```rust
use std::sync::Arc;

use crate::credentials::{CloudCredentials, ProviderCredential};

use super::fly::FlyAdapter;
use super::railway::RailwayAdapter;
use super::{CloudAdapter, CloudError};

pub struct AdapterRegistry;

impl AdapterRegistry {
    /// Create an adapter for the given provider, resolving credentials
    /// from the credentials store or environment variables.
    pub fn create(
        provider: &str,
        credentials: &CloudCredentials,
    ) -> Result<Arc<dyn CloudAdapter>, CloudError> {
        let cred = credentials.resolve(provider)
            .ok_or_else(|| CloudError::AuthError(format!(
                "No credentials configured for '{provider}'. Run: mhost cloud auth {provider}"
            )))?;

        match provider {
            "railway" => {
                let token = cred.get_token()
                    .ok_or_else(|| CloudError::AuthError("Railway requires a token".into()))?;
                Ok(Arc::new(RailwayAdapter::new(token)))
            }
            "fly" | "flyio" | "fly.io" => {
                let token = cred.get_token()
                    .ok_or_else(|| CloudError::AuthError("Fly.io requires a token".into()))?;
                Ok(Arc::new(FlyAdapter::new(token)))
            }
            _ => Err(CloudError::NotSupported(format!(
                "Provider '{provider}' is not supported yet. Supported: railway, fly"
            ))),
        }
    }

    /// List all supported provider names.
    pub fn supported_providers() -> &'static [&'static str] {
        &["railway", "fly", "aws", "gcp", "azure", "vercel", "digitalocean", "cloudflare", "netlify", "supabase"]
    }

    /// List providers that are currently implemented.
    pub fn implemented_providers() -> &'static [&'static str] {
        &["railway", "fly"]
    }

    /// Create adapters for all configured providers.
    pub fn create_all(credentials: &CloudCredentials) -> Vec<(String, Arc<dyn CloudAdapter>)> {
        let mut adapters = Vec::new();
        for provider in Self::implemented_providers() {
            if let Ok(adapter) = Self::create(provider, credentials) {
                adapters.push((provider.to_string(), adapter));
            }
        }
        adapters
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_no_credentials() {
        let creds = CloudCredentials::default();
        let result = AdapterRegistry::create("railway", &creds);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("No credentials"));
    }

    #[test]
    fn test_create_unsupported() {
        let creds = CloudCredentials::default();
        let result = AdapterRegistry::create("unknown-cloud", &creds);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("not supported"));
    }

    #[test]
    fn test_create_railway_with_token() {
        let mut creds = CloudCredentials::default();
        creds.set("railway", ProviderCredential::token("test-token"));
        let adapter = AdapterRegistry::create("railway", &creds).unwrap();
        assert_eq!(adapter.provider_name(), "railway");
    }

    #[test]
    fn test_create_fly_with_token() {
        let mut creds = CloudCredentials::default();
        creds.set("fly", ProviderCredential::token("fly-test"));
        let adapter = AdapterRegistry::create("fly", &creds).unwrap();
        assert_eq!(adapter.provider_name(), "fly");
    }

    #[test]
    fn test_supported_providers() {
        let providers = AdapterRegistry::supported_providers();
        assert!(providers.contains(&"railway"));
        assert!(providers.contains(&"fly"));
        assert!(providers.contains(&"aws"));
    }

    #[test]
    fn test_create_all_empty() {
        let creds = CloudCredentials::default();
        let adapters = AdapterRegistry::create_all(&creds);
        assert!(adapters.is_empty()); // No credentials configured
    }
}
```

- [ ] **Step 2: Run tests**

Run: `cargo test -p mhost-cloud --features cloud-native -- registry`
Expected: 6 tests PASS

- [ ] **Step 3: Commit**

```bash
git add crates/mhost-cloud/src/adapter/registry.rs
git commit -m "feat(cloud): add adapter registry for provider discovery"
```

---

## Task 6: Feature Flag Wiring & Full Build Verification

**Files:**
- Modify: `crates/mhost-daemon/Cargo.toml`
- Modify: `Cargo.toml` (workspace root)

- [ ] **Step 1: Add cloud-native feature to daemon**

In `crates/mhost-daemon/Cargo.toml`, add to the `[features]` section:

```toml
cloud-native = ["mhost-cloud/cloud-native"]
```

- [ ] **Step 2: Update lib.rs exports**

In `crates/mhost-cloud/src/lib.rs`, update the cloud-native exports:

```rust
#[cfg(feature = "cloud-native")]
pub use adapter::{
    CloudAdapter, CloudError, CloudService, CostEstimate, DeployConfig,
    ProvisionSpec, Resources, ServiceMetrics, ServiceStatus, ServiceType,
};
#[cfg(feature = "cloud-native")]
pub use adapter::registry::AdapterRegistry;
#[cfg(feature = "cloud-native")]
pub use credentials::{CloudCredentials, ProviderCredential};
```

- [ ] **Step 3: Verify builds**

Run: `cargo build -p mhost-cloud --features cloud-native`
Expected: Compiles

Run: `cargo build -p mhost-cloud`
Expected: Compiles (no cloud-native code)

Run: `cargo test -p mhost-cloud --features cloud-native`
Expected: All tests pass

Run: `cargo test --workspace`
Expected: All tests pass

- [ ] **Step 4: Commit**

```bash
git add crates/mhost-cloud/ crates/mhost-daemon/Cargo.toml Cargo.toml
git commit -m "feat(cloud): wire cloud-native feature flag across workspace"
```

---

## Task 7: CLI Commands for Cloud-Native

**Files:**
- Modify: `crates/mhost-cli/src/commands/cloud.rs`
- Modify: `crates/mhost-cli/tests/cli_test.rs`

- [ ] **Step 1: Add cloud-native CLI subcommands**

Read `crates/mhost-cli/src/commands/cloud.rs` and `crates/mhost-cli/src/cli.rs` to understand the existing command dispatch pattern. Then add new functions at the end of `cloud.rs`:

```rust
// ─── Cloud Native Commands (feature-gated at runtime) ─────

pub fn run_auth(paths: &MhostPaths, provider: &str) {
    println!("\n  mhost Cloud Auth — {provider}");
    println!("  {}", "─".repeat(40));

    match provider {
        "railway" => {
            println!("  Railway requires an API token.");
            println!("  Get one at: https://railway.app/account/tokens\n");
            print!("  Token: ");
            std::io::Write::flush(&mut std::io::stdout()).unwrap();
            let mut token = String::new();
            std::io::stdin().read_line(&mut token).unwrap();
            let token = token.trim();
            if token.is_empty() {
                println!("  {} No token provided", "✖".red());
                return;
            }
            save_credential(paths, provider, token);
        }
        "fly" | "flyio" => {
            println!("  Fly.io requires an API token.");
            println!("  Get one at: https://fly.io/user/personal_access_tokens\n");
            print!("  Token: ");
            std::io::Write::flush(&mut std::io::stdout()).unwrap();
            let mut token = String::new();
            std::io::stdin().read_line(&mut token).unwrap();
            let token = token.trim();
            if token.is_empty() {
                println!("  {} No token provided", "✖".red());
                return;
            }
            save_credential(paths, "fly", token);
        }
        _ => {
            println!("  {} Provider '{provider}' setup not yet implemented", "✖".red());
            println!("  Supported: railway, fly");
        }
    }
}

fn save_credential(paths: &MhostPaths, provider: &str, token: &str) {
    let cred_path = paths.cloud_credentials();
    let mut creds = load_credentials(&cred_path);
    creds.providers.insert(
        provider.to_string(),
        serde_json::json!({ "token": token }),
    );
    let data = serde_json::to_string_pretty(&creds).unwrap();
    if let Some(parent) = cred_path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    std::fs::write(&cred_path, data).unwrap();
    println!("  {} Credentials saved for '{provider}'", "✓".green());
}

fn load_credentials(path: &std::path::Path) -> serde_json::Value {
    if path.exists() {
        let data = std::fs::read_to_string(path).unwrap_or_default();
        serde_json::from_str(&data).unwrap_or(serde_json::json!({"providers": {}}))
    } else {
        serde_json::json!({"providers": {}})
    }
}

pub fn run_auth_list(paths: &MhostPaths) {
    let cred_path = paths.cloud_credentials();
    let creds = load_credentials(&cred_path);
    let providers = creds["providers"].as_object();
    match providers {
        Some(p) if !p.is_empty() => {
            println!("\n  Configured Providers:");
            println!("  {}", "─".repeat(30));
            for (name, _) in p {
                println!("  {} {name}", "●".green());
            }
            println!();
        }
        _ => println!("  No cloud providers configured. Run: mhost cloud auth <provider>"),
    }
}

pub fn run_auth_remove(paths: &MhostPaths, provider: &str) {
    let cred_path = paths.cloud_credentials();
    let mut creds = load_credentials(&cred_path);
    if let Some(providers) = creds["providers"].as_object_mut() {
        if providers.remove(provider).is_some() {
            let data = serde_json::to_string_pretty(&creds).unwrap();
            std::fs::write(&cred_path, data).unwrap();
            println!("  {} Credentials removed for '{provider}'", "✓".green());
        } else {
            println!("  {} Provider '{provider}' not configured", "✖".red());
        }
    }
}

pub fn run_cloud_provision_stub() {
    println!("\n  {} Cloud-native provisioning requires the 'cloud-native' feature.", "ℹ".cyan());
    println!("  Reinstall with:");
    println!("    cargo install mhost --features cloud-native");
    println!();
}
```

Wire these into the CLI command dispatch (add to the Cloud subcommand enum and match arms). The exact wiring depends on the existing pattern — read `cli.rs` before editing.

- [ ] **Step 2: Add CLI tests**

Append to `crates/mhost-cli/tests/cli_test.rs`:

```rust
#[test]
fn test_cloud_auth_list() {
    let (stdout, _, _) = run(&["cloud", "auth", "list"]);
    assert!(stdout.contains("No cloud providers") || stdout.contains("Configured"));
}
```

- [ ] **Step 3: Run tests**

Run: `cargo test -p mhost-cli -- test_cloud_auth`
Expected: PASS

Run: `cargo test --workspace`
Expected: All tests pass

- [ ] **Step 4: Commit**

```bash
git add crates/mhost-cli/
git commit -m "feat(cloud): add cloud auth CLI commands"
```

---

## Summary

| Task | What it builds | Tests |
|---|---|---|
| 1 | CloudAdapter trait, core types, feature flag, paths | 10 unit tests |
| 2 | Credential management with env var fallback | 6 unit tests |
| 3 | Railway adapter (GraphQL API) | 2 unit tests |
| 4 | Fly.io adapter (Machines REST API) | 3 unit tests |
| 5 | Adapter registry (provider discovery) | 6 unit tests |
| 6 | Feature flag wiring, build verification | Build checks |
| 7 | CLI commands (auth setup/list/remove) | 1 CLI test |

**Total: 7 tasks, ~28 unit tests, 2 provider adapters, feature-flagged.**

**Next phases (separate plans):**
- Phase 2: AWS, GCP, Azure adapters
- Phase 3: Cloudflare, Vercel, Netlify, DO, Supabase adapters
- Phase 4: Secrets, Cost, Drift, Backup, Export, Multi-region
- Phase 5: Agent tools, full CLI commands, documentation
