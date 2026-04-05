use std::collections::HashMap;
use std::fmt;
use std::str::FromStr;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

#[cfg(feature = "cloud-native")]
pub mod fly;
#[cfg(feature = "cloud-native")]
pub mod railway;
#[cfg(feature = "cloud-native")]
pub mod registry;

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

#[derive(Debug)]
pub enum CloudError {
    NotFound(String),
    AuthError(String),
    ApiError(String),
    NetworkError(String),
    InvalidConfig(String),
    NotSupported(String),
}

impl fmt::Display for CloudError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CloudError::NotFound(msg) => write!(f, "not found: {msg}"),
            CloudError::AuthError(msg) => write!(f, "auth error: {msg}"),
            CloudError::ApiError(msg) => write!(f, "api error: {msg}"),
            CloudError::NetworkError(msg) => write!(f, "network error: {msg}"),
            CloudError::InvalidConfig(msg) => write!(f, "invalid config: {msg}"),
            CloudError::NotSupported(msg) => write!(f, "not supported: {msg}"),
        }
    }
}

impl std::error::Error for CloudError {}

// ---------------------------------------------------------------------------
// Enums
// ---------------------------------------------------------------------------

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
        let label = match self {
            ServiceType::Container => "container",
            ServiceType::Kubernetes => "kubernetes",
            ServiceType::VM => "vm",
            ServiceType::Serverless => "serverless",
            ServiceType::AppRunner => "app_runner",
            ServiceType::EdgeFunction => "edge_function",
            ServiceType::StaticSite => "static_site",
        };
        write!(f, "{label}")
    }
}

impl FromStr for ServiceType {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "container" => Ok(ServiceType::Container),
            "kubernetes" | "k8s" => Ok(ServiceType::Kubernetes),
            "vm" => Ok(ServiceType::VM),
            "serverless" | "lambda" => Ok(ServiceType::Serverless),
            "app_runner" | "apprunner" => Ok(ServiceType::AppRunner),
            "edge_function" | "edge" | "worker" => Ok(ServiceType::EdgeFunction),
            "static_site" | "static" => Ok(ServiceType::StaticSite),
            other => Err(format!("unknown service type: {other}")),
        }
    }
}

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
        let label = match self {
            ServiceStatus::Running => "running",
            ServiceStatus::Stopped => "stopped",
            ServiceStatus::Deploying => "deploying",
            ServiceStatus::Failed => "failed",
            ServiceStatus::Unknown => "unknown",
        };
        write!(f, "{label}")
    }
}

// ---------------------------------------------------------------------------
// Data structures
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Resources {
    pub cpu: Option<String>,
    pub memory: Option<String>,
    pub disk: Option<String>,
}

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
    pub resources: Option<Resources>,
    pub created_at: Option<String>,
    pub provider_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProvisionSpec {
    pub name: String,
    pub service_type: ServiceType,
    pub region: String,
    pub instances: u32,
    pub public: bool,
    pub image: Option<String>,
    pub env: HashMap<String, String>,
    pub resources: Option<Resources>,
}

impl Default for ProvisionSpec {
    fn default() -> Self {
        Self {
            name: String::new(),
            service_type: ServiceType::Container,
            region: "us-east-1".to_string(),
            instances: 1,
            public: true,
            image: None,
            env: HashMap::new(),
            resources: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeployConfig {
    pub image: String,
    pub command: Option<String>,
    pub env: HashMap<String, String>,
    pub port: Option<u16>,
    pub health_check: Option<String>,
}

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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceMetrics {
    pub cpu_percent: f64,
    pub memory_mb: f64,
    pub requests_per_sec: f64,
    pub error_rate: f64,
}

// ---------------------------------------------------------------------------
// Trait
// ---------------------------------------------------------------------------

#[async_trait]
pub trait CloudAdapter: Send + Sync {
    fn provider_name(&self) -> &str;

    async fn list_services(&self) -> Result<Vec<CloudService>, CloudError>;
    async fn get_service(&self, name: &str) -> Result<CloudService, CloudError>;
    async fn provision(&self, spec: &ProvisionSpec) -> Result<CloudService, CloudError>;
    async fn destroy(&self, name: &str) -> Result<(), CloudError>;
    async fn deploy(&self, name: &str, config: &DeployConfig) -> Result<CloudService, CloudError>;
    async fn scale(&self, name: &str, instances: u32) -> Result<CloudService, CloudError>;
    async fn restart(&self, name: &str) -> Result<(), CloudError>;
    async fn logs(&self, name: &str, lines: u32) -> Result<Vec<String>, CloudError>;
    async fn status(&self, name: &str) -> Result<ServiceStatus, CloudError>;
    async fn metrics(&self, name: &str) -> Result<ServiceMetrics, CloudError>;
    async fn estimate_cost(&self, spec: &ProvisionSpec) -> Result<CostEstimate, CloudError>;
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_service_type_from_str() {
        assert_eq!(
            "container".parse::<ServiceType>().unwrap(),
            ServiceType::Container
        );
        assert_eq!(
            "k8s".parse::<ServiceType>().unwrap(),
            ServiceType::Kubernetes
        );
        assert_eq!(
            "kubernetes".parse::<ServiceType>().unwrap(),
            ServiceType::Kubernetes
        );
        assert_eq!(
            "lambda".parse::<ServiceType>().unwrap(),
            ServiceType::Serverless
        );
        assert_eq!(
            "edge".parse::<ServiceType>().unwrap(),
            ServiceType::EdgeFunction
        );
        assert_eq!(
            "worker".parse::<ServiceType>().unwrap(),
            ServiceType::EdgeFunction
        );
        assert_eq!("vm".parse::<ServiceType>().unwrap(), ServiceType::VM);
        assert!("nonsense".parse::<ServiceType>().is_err());
    }

    #[test]
    fn test_service_type_display() {
        assert_eq!(ServiceType::Container.to_string(), "container");
        assert_eq!(ServiceType::Kubernetes.to_string(), "kubernetes");
        assert_eq!(ServiceType::VM.to_string(), "vm");
        assert_eq!(ServiceType::Serverless.to_string(), "serverless");
        assert_eq!(ServiceType::AppRunner.to_string(), "app_runner");
        assert_eq!(ServiceType::EdgeFunction.to_string(), "edge_function");
        assert_eq!(ServiceType::StaticSite.to_string(), "static_site");
    }

    #[test]
    fn test_service_status_display() {
        assert_eq!(ServiceStatus::Running.to_string(), "running");
        assert_eq!(ServiceStatus::Stopped.to_string(), "stopped");
        assert_eq!(ServiceStatus::Deploying.to_string(), "deploying");
        assert_eq!(ServiceStatus::Failed.to_string(), "failed");
        assert_eq!(ServiceStatus::Unknown.to_string(), "unknown");
    }

    #[test]
    fn test_provision_spec_default() {
        let spec = ProvisionSpec::default();
        assert_eq!(spec.service_type, ServiceType::Container);
        assert_eq!(spec.region, "us-east-1");
        assert_eq!(spec.instances, 1);
        assert!(spec.public);
        assert!(spec.name.is_empty());
        assert!(spec.image.is_none());
        assert!(spec.env.is_empty());
        assert!(spec.resources.is_none());
    }

    #[test]
    fn test_cloud_error_display() {
        let err = CloudError::NotFound("svc-1".to_string());
        assert_eq!(err.to_string(), "not found: svc-1");

        let err = CloudError::AuthError("bad token".to_string());
        assert_eq!(err.to_string(), "auth error: bad token");

        let err = CloudError::ApiError("500".to_string());
        assert_eq!(err.to_string(), "api error: 500");

        let err = CloudError::NetworkError("timeout".to_string());
        assert_eq!(err.to_string(), "network error: timeout");

        let err = CloudError::InvalidConfig("missing field".to_string());
        assert_eq!(err.to_string(), "invalid config: missing field");

        let err = CloudError::NotSupported("gpu".to_string());
        assert_eq!(err.to_string(), "not supported: gpu");
    }

    #[test]
    fn test_cloud_service_serialize() {
        let svc = CloudService {
            name: "my-app".to_string(),
            provider: "railway".to_string(),
            service_type: ServiceType::Container,
            region: "us-east-1".to_string(),
            status: ServiceStatus::Running,
            instances: 2,
            url: Some("https://my-app.up.railway.app".to_string()),
            image: Some("my-app:latest".to_string()),
            resources: Some(Resources {
                cpu: Some("0.5".to_string()),
                memory: Some("512Mi".to_string()),
                disk: None,
            }),
            created_at: Some("2026-01-01T00:00:00Z".to_string()),
            provider_id: Some("proj-123".to_string()),
        };

        let json = serde_json::to_string(&svc).expect("serialize");
        let back: CloudService = serde_json::from_str(&json).expect("deserialize");

        assert_eq!(back.name, "my-app");
        assert_eq!(back.provider, "railway");
        assert_eq!(back.service_type, ServiceType::Container);
        assert_eq!(back.region, "us-east-1");
        assert_eq!(back.status, ServiceStatus::Running);
        assert_eq!(back.instances, 2);
        assert_eq!(back.url.as_deref(), Some("https://my-app.up.railway.app"));
        assert_eq!(back.image.as_deref(), Some("my-app:latest"));
        assert_eq!(back.resources.as_ref().unwrap().cpu.as_deref(), Some("0.5"));
        assert_eq!(
            back.resources.as_ref().unwrap().memory.as_deref(),
            Some("512Mi")
        );
        assert!(back.resources.as_ref().unwrap().disk.is_none());
        assert_eq!(back.created_at.as_deref(), Some("2026-01-01T00:00:00Z"));
        assert_eq!(back.provider_id.as_deref(), Some("proj-123"));
    }
}
