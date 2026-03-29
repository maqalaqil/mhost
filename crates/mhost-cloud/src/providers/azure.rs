use crate::provider::{CloudInstance, CloudProvider, ImportFilters};
use async_trait::async_trait;
use tokio::process::Command;

pub struct AzureProvider {
    pub subscription_id: Option<String>,
}

impl AzureProvider {
    pub fn new() -> Self {
        Self {
            subscription_id: std::env::var("AZURE_SUBSCRIPTION_ID").ok(),
        }
    }

    async fn list_via_cli(&self, filters: &ImportFilters) -> Result<Vec<CloudInstance>, String> {
        let mut args = vec![
            "vm".to_string(),
            "list".to_string(),
            "--show-details".to_string(),
            "--output".to_string(),
            "json".to_string(),
        ];

        if let Some(ref sub) = self.subscription_id {
            args.push("--subscription".to_string());
            args.push(sub.clone());
        }

        let output = Command::new("az")
            .args(&args)
            .output()
            .await
            .map_err(|e| format!("Azure CLI spawn failed: {e}"))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(format!("Azure CLI error: {stderr}"));
        }

        let json: serde_json::Value = serde_json::from_slice(&output.stdout)
            .map_err(|e| format!("Failed to parse Azure CLI output: {e}"))?;

        parse_azure_vms(&json, filters)
    }
}

fn parse_azure_vms(
    json: &serde_json::Value,
    filters: &ImportFilters,
) -> Result<Vec<CloudInstance>, String> {
    let vms = json
        .as_array()
        .ok_or("Expected JSON array from Azure CLI")?;

    let mut instances = Vec::new();

    for vm in vms {
        let name = match vm["name"].as_str() {
            Some(n) => n.to_string(),
            None => continue,
        };

        let ip = vm["publicIps"].as_str().unwrap_or("").to_string();
        if ip.is_empty() {
            continue;
        }

        let location = vm["location"].as_str().map(String::from);

        let instance_id = vm["id"].as_str().map(String::from);

        // Azure tags are a JSON object {"key": "value"}
        let tags: Vec<String> = vm["tags"]
            .as_object()
            .map(|obj| {
                obj.iter()
                    .map(|(k, v)| format!("{}={}", k, v.as_str().unwrap_or("")))
                    .collect()
            })
            .unwrap_or_default();

        // Apply region filter
        if let Some(ref required_region) = filters.region {
            if location.as_deref() != Some(required_region.as_str()) {
                continue;
            }
        }

        // Apply tag filter
        if !filters.tags.is_empty() {
            let matches = filters
                .tags
                .iter()
                .any(|(k, v)| tags.contains(&format!("{k}={v}")));
            if !matches {
                continue;
            }
        }

        instances.push(CloudInstance {
            name,
            host: ip,
            user: "azureuser".to_string(),
            region: location,
            instance_id,
            provider: "azure".to_string(),
            tags,
        });
    }

    Ok(instances)
}

impl Default for AzureProvider {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl CloudProvider for AzureProvider {
    async fn list_instances(&self, filters: &ImportFilters) -> Result<Vec<CloudInstance>, String> {
        self.list_via_cli(filters).await
    }

    fn provider_name(&self) -> &str {
        "azure"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider::ImportFilters;

    fn sample_azure_json() -> serde_json::Value {
        serde_json::json!([
            {
                "name": "vm-web-1",
                "publicIps": "20.0.0.1",
                "location": "eastus",
                "id": "/subscriptions/sub-id/resourceGroups/rg1/providers/Microsoft.Compute/virtualMachines/vm-web-1",
                "tags": { "env": "prod", "role": "web" }
            },
            {
                "name": "vm-db-1",
                "publicIps": "20.0.0.2",
                "location": "westeurope",
                "id": "/subscriptions/sub-id/resourceGroups/rg1/providers/Microsoft.Compute/virtualMachines/vm-db-1",
                "tags": { "env": "prod", "role": "db" }
            }
        ])
    }

    #[test]
    fn test_parse_azure_vms_all() {
        let json = sample_azure_json();
        let filters = ImportFilters::default();
        let instances = parse_azure_vms(&json, &filters).unwrap();

        assert_eq!(instances.len(), 2);
        assert_eq!(instances[0].name, "vm-web-1");
        assert_eq!(instances[0].host, "20.0.0.1");
        assert_eq!(instances[0].user, "azureuser");
        assert_eq!(instances[0].region, Some("eastus".to_string()));
        assert_eq!(instances[0].provider, "azure");
    }

    #[test]
    fn test_parse_azure_vms_region_filter() {
        let json = sample_azure_json();
        let filters = ImportFilters::default().with_region("westeurope");
        let instances = parse_azure_vms(&json, &filters).unwrap();

        assert_eq!(instances.len(), 1);
        assert_eq!(instances[0].name, "vm-db-1");
    }

    #[test]
    fn test_parse_azure_vms_tag_filter() {
        let json = sample_azure_json();
        let filters = ImportFilters::default().with_tag("role", "web");
        let instances = parse_azure_vms(&json, &filters).unwrap();

        assert_eq!(instances.len(), 1);
        assert_eq!(instances[0].name, "vm-web-1");
    }

    #[test]
    fn test_parse_azure_vms_skips_no_public_ip() {
        let json = serde_json::json!([{
            "name": "private-vm",
            "publicIps": "",
            "location": "eastus",
            "id": "/subs/1/rg/rg1/vm/private-vm",
            "tags": {}
        }]);
        let filters = ImportFilters::default();
        let instances = parse_azure_vms(&json, &filters).unwrap();
        assert!(instances.is_empty());
    }

    #[test]
    fn test_parse_azure_vms_tags_as_key_value_pairs() {
        let json = sample_azure_json();
        let filters = ImportFilters::default();
        let instances = parse_azure_vms(&json, &filters).unwrap();

        assert!(instances[0].tags.contains(&"env=prod".to_string()));
        assert!(instances[0].tags.contains(&"role=web".to_string()));
    }

    #[test]
    fn test_azure_provider_name() {
        let provider = AzureProvider::new();
        assert_eq!(provider.provider_name(), "azure");
    }
}
