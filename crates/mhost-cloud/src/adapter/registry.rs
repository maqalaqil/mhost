use std::sync::Arc;

use super::fly::FlyAdapter;
use super::railway::RailwayAdapter;
use super::{CloudAdapter, CloudError};
use crate::credentials::{CloudCredentials, ProviderCredential};

const SUPPORTED_PROVIDERS: &[&str] = &[
    "railway",
    "fly",
    "aws",
    "gcp",
    "azure",
    "digitalocean",
    "vercel",
    "cloudflare",
    "netlify",
    "supabase",
];

const IMPLEMENTED_PROVIDERS: &[&str] = &["railway", "fly"];

pub struct AdapterRegistry;

impl AdapterRegistry {
    /// Create an adapter for the given provider, resolving credentials from
    /// stored config or environment variables.
    pub fn create(
        provider: &str,
        credentials: &CloudCredentials,
    ) -> Result<Arc<dyn CloudAdapter>, CloudError> {
        if !SUPPORTED_PROVIDERS.contains(&provider) {
            return Err(CloudError::NotSupported(format!(
                "Unknown provider: {provider}"
            )));
        }

        let cred = credentials.resolve(provider).ok_or_else(|| {
            CloudError::AuthError(format!(
                "No credentials found for provider '{provider}'"
            ))
        })?;

        match provider {
            "railway" => {
                let token = extract_token(&cred, "railway")?;
                Ok(Arc::new(RailwayAdapter::new(&token)))
            }
            "fly" => {
                let token = extract_token(&cred, "fly")?;
                Ok(Arc::new(FlyAdapter::new(&token)))
            }
            other => Err(CloudError::NotSupported(format!(
                "Provider '{other}' is not yet implemented"
            ))),
        }
    }

    /// Returns all 10 provider names that the registry knows about.
    pub fn supported_providers() -> &'static [&'static str] {
        SUPPORTED_PROVIDERS
    }

    /// Returns only the providers that have working adapter implementations.
    pub fn implemented_providers() -> &'static [&'static str] {
        IMPLEMENTED_PROVIDERS
    }

    /// Create adapters for every provider that has credentials configured.
    /// Providers without credentials are silently skipped.
    pub fn create_all(
        credentials: &CloudCredentials,
    ) -> Vec<Arc<dyn CloudAdapter>> {
        IMPLEMENTED_PROVIDERS
            .iter()
            .filter_map(|provider| Self::create(provider, credentials).ok())
            .collect()
    }
}

fn extract_token(cred: &ProviderCredential, provider: &str) -> Result<String, CloudError> {
    cred.get_token()
        .map(String::from)
        .ok_or_else(|| {
            CloudError::AuthError(format!(
                "Provider '{provider}' requires a token credential"
            ))
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_no_credentials() {
        let creds = CloudCredentials::default();
        let result = AdapterRegistry::create("railway", &creds);
        match result {
            Err(ref e) => assert!(
                e.to_string().contains("No credentials"),
                "Expected auth error, got: {e}"
            ),
            Ok(_) => panic!("Expected error for missing credentials"),
        }
    }

    #[test]
    fn test_create_unsupported() {
        let creds = CloudCredentials::default();
        let result = AdapterRegistry::create("unknown-cloud", &creds);
        match result {
            Err(ref e) => assert!(
                e.to_string().contains("Unknown provider"),
                "Expected not-supported error, got: {e}"
            ),
            Ok(_) => panic!("Expected error for unsupported provider"),
        }
    }

    #[test]
    fn test_create_railway_with_token() {
        let mut creds = CloudCredentials::default();
        creds.set("railway", ProviderCredential::token("test-railway-tok"));
        let adapter = AdapterRegistry::create("railway", &creds).unwrap();
        assert_eq!(adapter.provider_name(), "railway");
    }

    #[test]
    fn test_create_fly_with_token() {
        let mut creds = CloudCredentials::default();
        creds.set("fly", ProviderCredential::token("test-fly-tok"));
        let adapter = AdapterRegistry::create("fly", &creds).unwrap();
        assert_eq!(adapter.provider_name(), "fly");
    }

    #[test]
    fn test_supported_providers() {
        let providers = AdapterRegistry::supported_providers();
        assert!(providers.contains(&"railway"));
        assert!(providers.contains(&"fly"));
        assert!(providers.contains(&"aws"));
        assert_eq!(providers.len(), 10);
    }

    #[test]
    fn test_create_all_empty() {
        let creds = CloudCredentials::default();
        let adapters = AdapterRegistry::create_all(&creds);
        assert!(adapters.is_empty());
    }
}
