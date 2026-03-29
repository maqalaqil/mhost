use crate::config::ServerConfig;
use async_trait::async_trait;

#[derive(Debug, Clone)]
pub struct CloudInstance {
    pub name: String,
    pub host: String,
    pub user: String,
    pub region: Option<String>,
    pub instance_id: Option<String>,
    pub provider: String,
    pub tags: Vec<String>,
}

impl CloudInstance {
    pub fn to_server_config(&self) -> ServerConfig {
        ServerConfig {
            host: self.host.clone(),
            user: self.user.clone(),
            region: self.region.clone(),
            instance_id: self.instance_id.clone(),
            provider: Some(self.provider.clone()),
            tags: self.tags.clone(),
            ..Default::default()
        }
    }
}

pub struct ImportFilters {
    pub region: Option<String>,
    pub tags: Vec<(String, String)>,
}

impl ImportFilters {
    pub fn new() -> Self {
        Self {
            region: None,
            tags: Vec::new(),
        }
    }

    pub fn with_region(mut self, region: &str) -> Self {
        self.region = Some(region.to_string());
        self
    }

    pub fn with_tag(mut self, key: &str, value: &str) -> Self {
        self.tags.push((key.to_string(), value.to_string()));
        self
    }
}

impl Default for ImportFilters {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
pub trait CloudProvider: Send + Sync {
    async fn list_instances(&self, filters: &ImportFilters) -> Result<Vec<CloudInstance>, String>;
    fn provider_name(&self) -> &str;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_instance(name: &str, host: &str, provider: &str) -> CloudInstance {
        CloudInstance {
            name: name.to_string(),
            host: host.to_string(),
            user: "ubuntu".to_string(),
            region: Some("us-east-1".to_string()),
            instance_id: Some("i-123456".to_string()),
            provider: provider.to_string(),
            tags: vec!["env:prod".to_string(), "role:web".to_string()],
        }
    }

    #[test]
    fn test_cloud_instance_to_server_config() {
        let instance = make_instance("web1", "54.12.34.56", "aws");
        let config = instance.to_server_config();

        assert_eq!(config.host, "54.12.34.56");
        assert_eq!(config.user, "ubuntu");
        assert_eq!(config.region, Some("us-east-1".to_string()));
        assert_eq!(config.instance_id, Some("i-123456".to_string()));
        assert_eq!(config.provider, Some("aws".to_string()));
        assert_eq!(config.tags, vec!["env:prod".to_string(), "role:web".to_string()]);
    }

    #[test]
    fn test_cloud_instance_to_server_config_defaults() {
        let instance = CloudInstance {
            name: "minimal".to_string(),
            host: "10.0.0.1".to_string(),
            user: "root".to_string(),
            region: None,
            instance_id: None,
            provider: "digitalocean".to_string(),
            tags: vec![],
        };
        let config = instance.to_server_config();

        assert_eq!(config.host, "10.0.0.1");
        assert_eq!(config.user, "root");
        assert!(config.region.is_none());
        assert!(config.instance_id.is_none());
        // Default port from #[derive(Default)] is 0 (not the serde default of 22)
        assert!(config.key_path.is_none());
    }

    #[test]
    fn test_import_filters_defaults() {
        let filters = ImportFilters::default();
        assert!(filters.region.is_none());
        assert!(filters.tags.is_empty());
    }

    #[test]
    fn test_import_filters_builder() {
        let filters = ImportFilters::new()
            .with_region("eu-west-1")
            .with_tag("env", "production")
            .with_tag("role", "web");

        assert_eq!(filters.region, Some("eu-west-1".to_string()));
        assert_eq!(filters.tags.len(), 2);
        assert_eq!(filters.tags[0], ("env".to_string(), "production".to_string()));
        assert_eq!(filters.tags[1], ("role".to_string(), "web".to_string()));
    }

    #[test]
    fn test_cloud_instance_tags_preserved_in_server_config() {
        let instance = make_instance("api1", "10.0.0.5", "azure");
        let config = instance.to_server_config();
        assert_eq!(config.tags.len(), 2);
        assert!(config.tags.contains(&"env:prod".to_string()));
        assert!(config.tags.contains(&"role:web".to_string()));
    }
}
