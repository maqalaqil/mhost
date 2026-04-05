#[cfg(feature = "cloud-native")]
pub mod adapter;
#[cfg(feature = "cloud-native")]
pub mod credentials;

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
pub use credentials::{CloudCredentials, ProviderCredential};
