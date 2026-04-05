use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

use chrono::{DateTime, Utc};

use crate::adapter::{CloudService, ProvisionSpec, Resources, ServiceType};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceBackup {
    pub name: String,
    pub provider: String,
    pub region: String,
    pub image: Option<String>,
    pub instances: u32,
    pub env: HashMap<String, String>,
    pub resources: Option<Resources>,
    pub created_at: DateTime<Utc>,
}

impl ServiceBackup {
    pub fn from_service(service: &CloudService, secrets: &HashMap<String, String>) -> Self {
        Self {
            name: service.name.clone(),
            provider: service.provider.clone(),
            region: service.region.clone(),
            image: service.image.clone(),
            instances: service.instances,
            env: secrets.clone(),
            resources: service.resources.clone(),
            created_at: Utc::now(),
        }
    }

    pub fn save(&self, dir: &Path) -> Result<String, String> {
        std::fs::create_dir_all(dir)
            .map_err(|e| format!("failed to create backup dir: {e}"))?;

        let ts = self.created_at.format("%Y%m%dT%H%M%S");
        let filename = format!("{}-{}.json", self.name, ts);
        let path = dir.join(&filename);

        let data = serde_json::to_string_pretty(self)
            .map_err(|e| format!("failed to serialize backup: {e}"))?;
        std::fs::write(&path, data)
            .map_err(|e| format!("failed to write backup: {e}"))?;

        Ok(filename)
    }

    pub fn load(path: &Path) -> Result<Self, String> {
        let data = std::fs::read_to_string(path)
            .map_err(|e| format!("failed to read backup: {e}"))?;
        serde_json::from_str(&data)
            .map_err(|e| format!("failed to parse backup: {e}"))
    }

    pub fn to_provision_spec(&self) -> ProvisionSpec {
        ProvisionSpec {
            name: self.name.clone(),
            service_type: ServiceType::Container,
            region: self.region.clone(),
            instances: self.instances,
            public: true,
            image: self.image.clone(),
            env: self.env.clone(),
            resources: self.resources.clone(),
        }
    }
}

pub fn list_backups(dir: &Path) -> Vec<(String, DateTime<Utc>)> {
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return Vec::new(),
    };

    let mut results = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        if let Ok(backup) = ServiceBackup::load(&path) {
            let filename = path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("")
                .to_string();
            results.push((filename, backup.created_at));
        }
    }
    results.sort_by(|a, b| b.1.cmp(&a.1));
    results
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapter::{ServiceStatus, ServiceType};

    fn make_service() -> CloudService {
        CloudService {
            name: "web-api".to_string(),
            provider: "fly".to_string(),
            service_type: ServiceType::Container,
            region: "ewr".to_string(),
            status: ServiceStatus::Running,
            instances: 3,
            url: Some("https://web-api.fly.dev".to_string()),
            image: Some("web-api:v2".to_string()),
            resources: Some(Resources {
                cpu: Some("1".to_string()),
                memory: Some("256Mi".to_string()),
                disk: None,
            }),
            created_at: None,
            provider_id: None,
        }
    }

    #[test]
    fn test_from_service() {
        let svc = make_service();
        let mut secrets = HashMap::new();
        secrets.insert("DB_URL".to_string(), "postgres://localhost".to_string());

        let backup = ServiceBackup::from_service(&svc, &secrets);
        assert_eq!(backup.name, "web-api");
        assert_eq!(backup.provider, "fly");
        assert_eq!(backup.instances, 3);
        assert_eq!(backup.env.get("DB_URL").unwrap(), "postgres://localhost");
        assert_eq!(backup.image.as_deref(), Some("web-api:v2"));
    }

    #[test]
    fn test_save_and_load() {
        let dir = tempfile::tempdir().unwrap();
        let svc = make_service();
        let secrets = HashMap::new();
        let backup = ServiceBackup::from_service(&svc, &secrets);

        let filename = backup.save(dir.path()).unwrap();
        assert!(filename.starts_with("web-api-"));
        assert!(filename.ends_with(".json"));

        let loaded = ServiceBackup::load(&dir.path().join(&filename)).unwrap();
        assert_eq!(loaded.name, "web-api");
        assert_eq!(loaded.instances, 3);
    }

    #[test]
    fn test_to_provision_spec() {
        let svc = make_service();
        let mut secrets = HashMap::new();
        secrets.insert("KEY".to_string(), "val".to_string());

        let backup = ServiceBackup::from_service(&svc, &secrets);
        let spec = backup.to_provision_spec();

        assert_eq!(spec.name, "web-api");
        assert_eq!(spec.region, "ewr");
        assert_eq!(spec.instances, 3);
        assert_eq!(spec.image.as_deref(), Some("web-api:v2"));
        assert_eq!(spec.env.get("KEY").unwrap(), "val");
        assert!(spec.resources.is_some());
    }
}
