use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SecretStore {
    pub services: HashMap<String, HashMap<String, String>>,
}

impl SecretStore {
    pub fn load(path: &Path) -> Result<Self, String> {
        let data = std::fs::read_to_string(path)
            .map_err(|e| format!("failed to read secrets file: {e}"))?;
        serde_json::from_str(&data)
            .map_err(|e| format!("failed to parse secrets file: {e}"))
    }

    pub fn save(&self, path: &Path) -> Result<(), String> {
        let data = serde_json::to_string_pretty(self)
            .map_err(|e| format!("failed to serialize secrets: {e}"))?;
        std::fs::write(path, data)
            .map_err(|e| format!("failed to write secrets file: {e}"))
    }

    pub fn set(&self, service: &str, key: &str, value: &str) -> Self {
        let mut new_store = self.clone();
        new_store
            .services
            .entry(service.to_string())
            .or_default()
            .insert(key.to_string(), value.to_string());
        new_store
    }

    pub fn get(&self, service: &str, key: &str) -> Option<&str> {
        self.services
            .get(service)
            .and_then(|m| m.get(key))
            .map(|s| s.as_str())
    }

    pub fn list(&self, service: &str) -> Vec<&str> {
        self.services
            .get(service)
            .map(|m| m.keys().map(|k| k.as_str()).collect())
            .unwrap_or_default()
    }

    pub fn remove(&self, service: &str, key: &str) -> (Self, bool) {
        let mut new_store = self.clone();
        let removed = new_store
            .services
            .get_mut(service)
            .map(|m| m.remove(key).is_some())
            .unwrap_or(false);
        (new_store, removed)
    }

    pub fn all_for_service(&self, service: &str) -> HashMap<String, String> {
        self.services
            .get(service)
            .cloned()
            .unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_set_and_get() {
        let store = SecretStore::default();
        let store = store.set("my-app", "DB_URL", "postgres://localhost/db");
        let store = store.set("my-app", "API_KEY", "secret-123");

        assert_eq!(store.get("my-app", "DB_URL"), Some("postgres://localhost/db"));
        assert_eq!(store.get("my-app", "API_KEY"), Some("secret-123"));
        assert_eq!(store.get("my-app", "MISSING"), None);
        assert_eq!(store.get("other", "DB_URL"), None);
    }

    #[test]
    fn test_list_keys() {
        let store = SecretStore::default();
        let store = store.set("svc", "A", "1");
        let store = store.set("svc", "B", "2");

        let mut keys = store.list("svc");
        keys.sort();
        assert_eq!(keys, vec!["A", "B"]);
        assert!(store.list("missing").is_empty());
    }

    #[test]
    fn test_remove() {
        let store = SecretStore::default();
        let store = store.set("svc", "KEY", "val");

        let (store, removed) = store.remove("svc", "KEY");
        assert!(removed);
        assert_eq!(store.get("svc", "KEY"), None);

        let (_, not_removed) = store.remove("svc", "KEY");
        assert!(!not_removed);
    }

    #[test]
    fn test_persistence() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("secrets.json");

        let store = SecretStore::default();
        let store = store.set("app", "TOKEN", "abc");
        store.save(&path).unwrap();

        let loaded = SecretStore::load(&path).unwrap();
        assert_eq!(loaded.get("app", "TOKEN"), Some("abc"));
    }
}
