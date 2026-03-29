pub mod aws;
pub mod azure;
pub mod digitalocean;
pub mod railway;

use crate::provider::CloudProvider;

pub fn create_provider(name: &str) -> Result<Box<dyn CloudProvider>, String> {
    match name {
        "aws" => Ok(Box::new(aws::AwsProvider::new("us-east-1"))),
        "digitalocean" | "do" => Ok(Box::new(digitalocean::DigitalOceanProvider::new()?)),
        "azure" => Ok(Box::new(azure::AzureProvider::new())),
        "railway" => Ok(Box::new(railway::RailwayProvider::new()?)),
        _ => Err(format!(
            "Unknown provider: '{}'. Supported: aws, digitalocean, azure, railway",
            name
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_provider_aws() {
        // AWS provider doesn't require env vars to construct
        let result = create_provider("aws");
        assert!(result.is_ok());
        assert_eq!(result.unwrap().provider_name(), "aws");
    }

    #[test]
    fn test_create_provider_azure() {
        // Azure provider doesn't require env vars to construct
        let result = create_provider("azure");
        assert!(result.is_ok());
        assert_eq!(result.unwrap().provider_name(), "azure");
    }

    #[test]
    fn test_create_provider_unknown() {
        let result = create_provider("gcp");
        assert!(result.is_err());
        let err = result.err().unwrap();
        assert!(err.contains("Unknown provider"));
        assert!(err.contains("gcp"));
        assert!(err.contains("aws"));
        assert!(err.contains("digitalocean"));
    }

    #[test]
    fn test_create_provider_do_alias() {
        // "do" alias requires DIGITALOCEAN_TOKEN, so we just confirm it
        // attempts the right provider (will error without env var)
        let result = create_provider("do");
        if std::env::var("DIGITALOCEAN_TOKEN").is_ok() {
            assert!(result.is_ok());
        } else {
            // Should fail with the right error message
            assert!(result.is_err());
            assert!(result.err().unwrap().contains("DIGITALOCEAN_TOKEN"));
        }
    }

    #[test]
    fn test_create_provider_railway_without_token() {
        if std::env::var("RAILWAY_TOKEN").is_err() {
            let result = create_provider("railway");
            assert!(result.is_err());
            assert!(result.err().unwrap().contains("RAILWAY_TOKEN"));
        }
    }

    #[test]
    fn test_create_provider_supported_list_in_error() {
        let err = create_provider("unknown-cloud").err().unwrap();
        assert!(err.contains("digitalocean"));
        assert!(err.contains("azure"));
        assert!(err.contains("railway"));
    }
}
