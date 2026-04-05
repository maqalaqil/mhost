#[cfg(feature = "cloud-native")]
pub mod adapter;
#[cfg(feature = "cloud-native")]
pub mod backup;
#[cfg(feature = "cloud-native")]
pub mod cost;
#[cfg(feature = "cloud-native")]
pub mod credentials;
#[cfg(feature = "cloud-native")]
pub mod drift;
#[cfg(feature = "cloud-native")]
pub mod export;
#[cfg(feature = "cloud-native")]
pub mod secrets;

pub mod ai_cloud;
pub mod config;
pub mod fleet;
pub mod install;
pub mod provider;
pub mod providers;
pub mod remote;
pub mod ssh;

pub use config::{AuthMethod, FleetConfig, ServerConfig};
pub use fleet::Fleet;
pub use provider::{CloudInstance, CloudProvider, ImportFilters};
pub use remote::{RemoteHost, ServerStatus};
pub use ssh::{SshExecutor, SshOutput};

#[cfg(feature = "cloud-native")]
pub use adapter::registry::AdapterRegistry;
#[cfg(feature = "cloud-native")]
pub use adapter::{
    CloudAdapter, CloudError, CloudService, CostEstimate, DeployConfig, ProvisionSpec, Resources,
    ServiceMetrics, ServiceStatus, ServiceType,
};
#[cfg(feature = "cloud-native")]
pub use backup::ServiceBackup;
#[cfg(feature = "cloud-native")]
pub use cost::{BudgetConfig, CostReport, ServiceCost};
#[cfg(feature = "cloud-native")]
pub use credentials::{CloudCredentials, ProviderCredential};
#[cfg(feature = "cloud-native")]
pub use drift::{detect_drift, DriftResult};
#[cfg(feature = "cloud-native")]
pub use secrets::SecretStore;
