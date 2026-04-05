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
