use std::sync::Arc;

use super::aws::AwsAdapter;
use super::azure::AzureAdapter;
use super::cloudflare::CloudflareAdapter;
use super::digitalocean::DigitaloceanAdapter;
use super::fly::FlyAdapter;
use super::gcp::GcpAdapter;
use super::netlify::NetlifyAdapter;
use super::railway::RailwayAdapter;
use super::supabase::SupabaseAdapter;
use super::vercel::VercelAdapter;
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

const IMPLEMENTED_PROVIDERS: &[&str] = &[
    "railway",
    "fly",
    "aws",
    "gcp",
    "azure",
    "cloudflare",
    "vercel",
    "netlify",
    "digitalocean",
    "supabase",
];

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
            CloudError::AuthError(format!("No credentials found for provider '{provider}'"))
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
            "aws" => match cred {
                ProviderCredential::AwsKeys {
                    access_key_id,
                    secret_access_key,
                    default_region,
                } => {
                    let region = default_region.as_deref().unwrap_or("us-east-1");
                    Ok(Arc::new(AwsAdapter::new(
                        &access_key_id,
                        &secret_access_key,
                        region,
                    )))
                }
                _ => Err(CloudError::AuthError(
                    "Provider 'aws' requires AwsKeys credentials".into(),
                )),
            },
            "gcp" | "google" => match cred {
                ProviderCredential::GcpServiceAccount {
                    credentials_file,
                    default_region,
                } => {
                    let region = default_region.as_deref().unwrap_or("us-central1");
                    Ok(Arc::new(GcpAdapter::new(&credentials_file, region)))
                }
                _ => Err(CloudError::AuthError(
                    "Provider 'gcp' requires GcpServiceAccount credentials".into(),
                )),
            },
            "azure" => match cred {
                ProviderCredential::AzureServicePrincipal {
                    client_id,
                    client_secret,
                    tenant_id,
                    subscription_id,
                } => {
                    let sub_id = subscription_id.as_deref().unwrap_or("");
                    if sub_id.is_empty() {
                        return Err(CloudError::InvalidConfig(
                            "Azure requires a subscription_id".into(),
                        ));
                    }
                    Ok(Arc::new(AzureAdapter::new(
                        &client_id,
                        &client_secret,
                        &tenant_id,
                        sub_id,
                        "eastus",
                    )))
                }
                _ => Err(CloudError::AuthError(
                    "Provider 'azure' requires AzureServicePrincipal credentials".into(),
                )),
            },
            "cloudflare" => {
                let token = extract_token(&cred, "cloudflare")?;
                // Account ID can be passed via default_region field or extracted from token metadata
                let account_id = cred.default_region().unwrap_or("").to_string();
                if account_id.is_empty() {
                    return Err(CloudError::InvalidConfig(
                        "Cloudflare requires an account_id (set via default_region in credentials)".into(),
                    ));
                }
                Ok(Arc::new(CloudflareAdapter::new(&token, &account_id)))
            }
            "vercel" => {
                let token = extract_token(&cred, "vercel")?;
                Ok(Arc::new(VercelAdapter::new(&token)))
            }
            "netlify" => {
                let token = extract_token(&cred, "netlify")?;
                Ok(Arc::new(NetlifyAdapter::new(&token)))
            }
            "digitalocean" => {
                let token = extract_token(&cred, "digitalocean")?;
                Ok(Arc::new(DigitaloceanAdapter::new(&token)))
            }
            "supabase" => {
                let token = extract_token(&cred, "supabase")?;
                Ok(Arc::new(SupabaseAdapter::new(&token)))
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
    pub fn create_all(credentials: &CloudCredentials) -> Vec<Arc<dyn CloudAdapter>> {
        IMPLEMENTED_PROVIDERS
            .iter()
            .filter_map(|provider| Self::create(provider, credentials).ok())
            .collect()
    }
}

fn extract_token(cred: &ProviderCredential, provider: &str) -> Result<String, CloudError> {
    cred.get_token().map(String::from).ok_or_else(|| {
        CloudError::AuthError(format!("Provider '{provider}' requires a token credential"))
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
    fn test_create_vercel_with_token() {
        let mut creds = CloudCredentials::default();
        creds.set("vercel", ProviderCredential::token("test-vercel-tok"));
        let adapter = AdapterRegistry::create("vercel", &creds).unwrap();
        assert_eq!(adapter.provider_name(), "vercel");
    }

    #[test]
    fn test_create_netlify_with_token() {
        let mut creds = CloudCredentials::default();
        creds.set("netlify", ProviderCredential::token("test-netlify-tok"));
        let adapter = AdapterRegistry::create("netlify", &creds).unwrap();
        assert_eq!(adapter.provider_name(), "netlify");
    }

    #[test]
    fn test_create_digitalocean_with_token() {
        let mut creds = CloudCredentials::default();
        creds.set(
            "digitalocean",
            ProviderCredential::token("test-do-tok"),
        );
        let adapter = AdapterRegistry::create("digitalocean", &creds).unwrap();
        assert_eq!(adapter.provider_name(), "digitalocean");
    }

    #[test]
    fn test_create_supabase_with_token() {
        let mut creds = CloudCredentials::default();
        creds.set("supabase", ProviderCredential::token("test-sb-tok"));
        let adapter = AdapterRegistry::create("supabase", &creds).unwrap();
        assert_eq!(adapter.provider_name(), "supabase");
    }

    #[test]
    fn test_create_cloudflare_requires_account_id() {
        let mut creds = CloudCredentials::default();
        creds.set("cloudflare", ProviderCredential::token("test-cf-tok"));
        let result = AdapterRegistry::create("cloudflare", &creds);
        match result {
            Err(ref e) => assert!(
                e.to_string().contains("account_id"),
                "Expected account_id error, got: {e}"
            ),
            Ok(_) => panic!("Expected error for missing account_id"),
        }
    }

    #[test]
    fn test_create_cloudflare_with_account_id() {
        let mut creds = CloudCredentials::default();
        creds.set(
            "cloudflare",
            ProviderCredential::Token {
                token: "test-cf-tok".into(),
                default_region: Some("account-abc123".into()),
            },
        );
        let adapter = AdapterRegistry::create("cloudflare", &creds).unwrap();
        assert_eq!(adapter.provider_name(), "cloudflare");
    }

    #[test]
    fn test_implemented_providers_count() {
        let providers = AdapterRegistry::implemented_providers();
        assert_eq!(providers.len(), 10);
        assert!(providers.contains(&"cloudflare"));
        assert!(providers.contains(&"vercel"));
        assert!(providers.contains(&"netlify"));
        assert!(providers.contains(&"digitalocean"));
        assert!(providers.contains(&"supabase"));
    }

    #[test]
    fn test_create_all_empty() {
        let creds = CloudCredentials::default();
        let adapters = AdapterRegistry::create_all(&creds);
        assert!(adapters.is_empty());
    }
}
