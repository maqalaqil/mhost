use async_trait::async_trait;
use chrono::Utc;
use reqwest::Client;
use tracing::info;

use super::{
    CloudAdapter, CloudError, CloudService, CostEstimate, CostLine, DeployConfig, ProvisionSpec,
    Resources, ServiceMetrics, ServiceStatus, ServiceType,
};

const DO_API: &str = "https://api.digitalocean.com/v2";

pub struct DigitaloceanAdapter {
    token: String,
    client: Client,
}

impl DigitaloceanAdapter {
    pub fn new(token: &str) -> Self {
        Self {
            token: token.to_string(),
            client: Client::new(),
        }
    }

    async fn api_get(&self, url: &str) -> Result<serde_json::Value, CloudError> {
        let resp = self
            .client
            .get(url)
            .header("Authorization", format!("Bearer {}", self.token))
            .send()
            .await
            .map_err(|e| CloudError::NetworkError(e.to_string()))?;

        let status = resp.status().as_u16();
        if status == 401 {
            return Err(CloudError::AuthError("Invalid DigitalOcean token".into()));
        }
        if status == 404 {
            return Err(CloudError::NotFound(
                "Resource not found on DigitalOcean".into(),
            ));
        }

        let body: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| CloudError::NetworkError(e.to_string()))?;

        if let Some(msg) = body.get("message").and_then(|m| m.as_str()) {
            if status >= 400 {
                return Err(CloudError::ApiError(format!(
                    "digitalocean ({status}): {msg}"
                )));
            }
        }

        Ok(body)
    }

    async fn api_request(
        &self,
        method: reqwest::Method,
        url: &str,
        body: Option<serde_json::Value>,
    ) -> Result<serde_json::Value, CloudError> {
        let mut req = self
            .client
            .request(method, url)
            .header("Authorization", format!("Bearer {}", self.token))
            .header("Content-Type", "application/json");

        if let Some(json_body) = body {
            req = req.json(&json_body);
        }

        let resp = req
            .send()
            .await
            .map_err(|e| CloudError::NetworkError(e.to_string()))?;

        let status = resp.status().as_u16();
        if status == 401 {
            return Err(CloudError::AuthError("Invalid DigitalOcean token".into()));
        }

        // DELETE may return 204 No Content
        if status == 204 {
            return Ok(serde_json::Value::Null);
        }

        let data: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| CloudError::NetworkError(e.to_string()))?;

        if let Some(msg) = data.get("message").and_then(|m| m.as_str()) {
            if status >= 400 {
                return Err(CloudError::ApiError(format!(
                    "digitalocean ({status}): {msg}"
                )));
            }
        }

        Ok(data)
    }

    fn parse_app(&self, app: &serde_json::Value) -> CloudService {
        let spec = &app["spec"];
        let name = spec["name"].as_str().unwrap_or("unknown").to_string();
        let id = app["id"].as_str().unwrap_or("").to_string();

        let phase = app["phase"].as_str().unwrap_or("UNKNOWN");
        let status = match phase {
            "ACTIVE" => ServiceStatus::Running,
            "DEPLOYING" | "BUILDING" => ServiceStatus::Deploying,
            "ERROR" => ServiceStatus::Failed,
            _ => ServiceStatus::Unknown,
        };

        let url = app["live_url"].as_str().map(String::from);
        let region = spec["region"].as_str().unwrap_or("nyc").to_string();

        let instance_count = spec["services"]
            .as_array()
            .and_then(|svcs| svcs.first())
            .and_then(|svc| svc["instance_count"].as_u64())
            .unwrap_or(1) as u32;

        CloudService {
            name,
            provider: "digitalocean".into(),
            service_type: ServiceType::Container,
            region,
            status,
            instances: instance_count,
            url,
            image: spec["services"]
                .as_array()
                .and_then(|svcs| svcs.first())
                .and_then(|svc| svc["image"]["repository"].as_str())
                .map(String::from),
            resources: Some(Resources {
                cpu: None,
                memory: None,
                disk: None,
            }),
            created_at: app["created_at"].as_str().map(String::from),
            provider_id: Some(id),
        }
    }

    fn parse_droplet(&self, droplet: &serde_json::Value) -> CloudService {
        let name = droplet["name"].as_str().unwrap_or("unknown").to_string();
        let id = droplet["id"].as_u64().unwrap_or(0);

        let droplet_status = droplet["status"].as_str().unwrap_or("unknown");
        let status = match droplet_status {
            "active" => ServiceStatus::Running,
            "new" => ServiceStatus::Deploying,
            "off" => ServiceStatus::Stopped,
            _ => ServiceStatus::Unknown,
        };

        let region = droplet["region"]["slug"]
            .as_str()
            .unwrap_or("nyc1")
            .to_string();

        let ip = droplet["networks"]["v4"]
            .as_array()
            .and_then(|nets| {
                nets.iter()
                    .find(|n| n["type"].as_str() == Some("public"))
                    .and_then(|n| n["ip_address"].as_str())
            })
            .map(|ip| format!("http://{ip}"));

        let vcpus = droplet["vcpus"].as_u64().map(|v| v.to_string());
        let memory_mb = droplet["memory"].as_u64().map(|m| format!("{m}Mi"));
        let disk_gb = droplet["disk"].as_u64().map(|d| format!("{d}Gi"));

        CloudService {
            name,
            provider: "digitalocean".into(),
            service_type: ServiceType::VM,
            region,
            status,
            instances: 1,
            url: ip,
            image: droplet["image"]["slug"].as_str().map(String::from),
            resources: Some(Resources {
                cpu: vcpus,
                memory: memory_mb,
                disk: disk_gb,
            }),
            created_at: droplet["created_at"].as_str().map(String::from),
            provider_id: Some(format!("droplet-{id}")),
        }
    }
}

#[async_trait]
impl CloudAdapter for DigitaloceanAdapter {
    fn provider_name(&self) -> &str {
        "digitalocean"
    }

    async fn list_services(&self) -> Result<Vec<CloudService>, CloudError> {
        let mut services = Vec::new();

        // List App Platform apps
        let apps_url = format!("{DO_API}/apps");
        if let Ok(data) = self.api_get(&apps_url).await {
            if let Some(apps) = data["apps"].as_array() {
                for app in apps {
                    services.push(self.parse_app(app));
                }
            }
        }

        // List Droplets
        let droplets_url = format!("{DO_API}/droplets");
        if let Ok(data) = self.api_get(&droplets_url).await {
            if let Some(droplets) = data["droplets"].as_array() {
                for droplet in droplets {
                    services.push(self.parse_droplet(droplet));
                }
            }
        }

        Ok(services)
    }

    async fn get_service(&self, name: &str) -> Result<CloudService, CloudError> {
        let services = self.list_services().await?;
        services
            .into_iter()
            .find(|s| s.name == name)
            .ok_or_else(|| {
                CloudError::NotFound(format!("Service '{name}' not found on DigitalOcean"))
            })
    }

    async fn provision(&self, spec: &ProvisionSpec) -> Result<CloudService, CloudError> {
        match spec.service_type {
            ServiceType::VM => self.provision_droplet(spec).await,
            _ => self.provision_app(spec).await,
        }
    }

    async fn destroy(&self, name: &str) -> Result<(), CloudError> {
        let service = self.get_service(name).await?;
        let provider_id = service.provider_id.unwrap_or_default();

        if provider_id.starts_with("droplet-") {
            let droplet_id = provider_id.trim_start_matches("droplet-");
            let url = format!("{DO_API}/droplets/{droplet_id}");
            self.api_request(reqwest::Method::DELETE, &url, None)
                .await?;
        } else {
            let url = format!("{DO_API}/apps/{provider_id}");
            self.api_request(reqwest::Method::DELETE, &url, None)
                .await?;
        }

        info!(provider = "digitalocean", service = %name, "Service destroyed");
        Ok(())
    }

    async fn deploy(&self, name: &str, config: &DeployConfig) -> Result<CloudService, CloudError> {
        let service = self.get_service(name).await?;
        let provider_id = service.provider_id.as_deref().unwrap_or_default();

        if provider_id.starts_with("droplet-") {
            return Err(CloudError::NotSupported(
                "Deploy is not supported for Droplets; use provision instead".into(),
            ));
        }

        // Update App Platform app
        let url = format!("{DO_API}/apps/{provider_id}");
        let body = serde_json::json!({
            "spec": {
                "name": name,
                "services": [{
                    "name": name,
                    "image": {
                        "registry_type": "DOCKER_HUB",
                        "repository": config.image,
                    },
                    "http_port": config.port.unwrap_or(8080),
                    "envs": config.env.iter().map(|(k, v)| {
                        serde_json::json!({ "key": k, "value": v })
                    }).collect::<Vec<_>>(),
                }],
            }
        });

        self.api_request(reqwest::Method::PUT, &url, Some(body))
            .await?;

        info!(provider = "digitalocean", service = %name, "App updated");
        self.get_service(name).await
    }

    async fn scale(&self, name: &str, instances: u32) -> Result<CloudService, CloudError> {
        let service = self.get_service(name).await?;
        let provider_id = service.provider_id.as_deref().unwrap_or_default();

        if provider_id.starts_with("droplet-") {
            return Err(CloudError::NotSupported(
                "Scaling Droplets requires creating/destroying instances manually".into(),
            ));
        }

        let url = format!("{DO_API}/apps/{provider_id}");
        let body = serde_json::json!({
            "spec": {
                "name": name,
                "services": [{
                    "name": name,
                    "instance_count": instances,
                }],
            }
        });

        self.api_request(reqwest::Method::PUT, &url, Some(body))
            .await?;

        info!(provider = "digitalocean", service = %name, instances, "App scaled");

        let mut updated = self.get_service(name).await?;
        updated.instances = instances;
        Ok(updated)
    }

    async fn restart(&self, name: &str) -> Result<(), CloudError> {
        let service = self.get_service(name).await?;
        let provider_id = service.provider_id.as_deref().unwrap_or_default();

        if provider_id.starts_with("droplet-") {
            let droplet_id = provider_id.trim_start_matches("droplet-");
            let url = format!("{DO_API}/droplets/{droplet_id}/actions");
            let body = serde_json::json!({ "type": "reboot" });
            self.api_request(reqwest::Method::POST, &url, Some(body))
                .await?;
        } else {
            // App Platform: create a new deployment to restart
            let url = format!("{DO_API}/apps/{provider_id}/deployments");
            let body = serde_json::json!({ "force_build": true });
            self.api_request(reqwest::Method::POST, &url, Some(body))
                .await?;
        }

        info!(provider = "digitalocean", service = %name, "Restarted");
        Ok(())
    }

    async fn logs(&self, name: &str, _lines: u32) -> Result<Vec<String>, CloudError> {
        let service = self.get_service(name).await?;
        let provider_id = service.provider_id.as_deref().unwrap_or_default();

        if provider_id.starts_with("droplet-") {
            return Err(CloudError::NotSupported(
                "Droplet logs require SSH access; not available via API".into(),
            ));
        }

        let url = format!("{DO_API}/apps/{provider_id}/logs?type=RUN");
        let data = self.api_get(&url).await?;

        let log_url = data["live_url"].as_str().unwrap_or("");
        Ok(vec![format!("Live logs available at: {log_url}")])
    }

    async fn status(&self, name: &str) -> Result<ServiceStatus, CloudError> {
        let service = self.get_service(name).await?;
        Ok(service.status)
    }

    async fn metrics(&self, _name: &str) -> Result<ServiceMetrics, CloudError> {
        Err(CloudError::NotSupported(
            "DigitalOcean metrics require the Monitoring API with agent installed".into(),
        ))
    }

    async fn estimate_cost(&self, spec: &ProvisionSpec) -> Result<CostEstimate, CloudError> {
        let (per_instance, label) = match spec.service_type {
            ServiceType::VM => (4.0, "Droplet (Basic, 512MB)"),
            _ => (5.0, "App Platform (Basic)"),
        };

        let monthly = per_instance * spec.instances as f64;
        Ok(CostEstimate {
            hourly: monthly / 730.0,
            monthly,
            currency: "USD".into(),
            breakdown: vec![CostLine {
                item: format!("{}x {label}", spec.instances),
                amount: monthly,
            }],
        })
    }
}

// Private helpers for provision
impl DigitaloceanAdapter {
    async fn provision_app(&self, spec: &ProvisionSpec) -> Result<CloudService, CloudError> {
        let url = format!("{DO_API}/apps");
        let image = spec.image.as_deref().unwrap_or("nginx");
        let body = serde_json::json!({
            "spec": {
                "name": spec.name,
                "region": spec.region,
                "services": [{
                    "name": spec.name,
                    "image": {
                        "registry_type": "DOCKER_HUB",
                        "repository": image,
                    },
                    "instance_count": spec.instances,
                    "instance_size_slug": "basic-xxs",
                    "http_port": 8080,
                    "envs": spec.env.iter().map(|(k, v)| {
                        serde_json::json!({ "key": k, "value": v })
                    }).collect::<Vec<_>>(),
                }],
            }
        });

        let data = self
            .api_request(reqwest::Method::POST, &url, Some(body))
            .await?;

        let app_id = data["app"]["id"].as_str().unwrap_or("").to_string();
        info!(provider = "digitalocean", service = %spec.name, "App provisioned");

        Ok(CloudService {
            name: spec.name.clone(),
            provider: "digitalocean".into(),
            service_type: ServiceType::Container,
            region: spec.region.clone(),
            status: ServiceStatus::Deploying,
            instances: spec.instances,
            url: None,
            image: Some(image.to_string()),
            resources: spec.resources.clone(),
            created_at: Some(Utc::now().to_rfc3339()),
            provider_id: Some(app_id),
        })
    }

    async fn provision_droplet(&self, spec: &ProvisionSpec) -> Result<CloudService, CloudError> {
        let url = format!("{DO_API}/droplets");
        let image = spec.image.as_deref().unwrap_or("ubuntu-24-04-x64");
        let body = serde_json::json!({
            "name": spec.name,
            "region": spec.region,
            "size": "s-1vcpu-512mb-10gb",
            "image": image,
        });

        let data = self
            .api_request(reqwest::Method::POST, &url, Some(body))
            .await?;

        let droplet_id = data["droplet"]["id"].as_u64().unwrap_or(0);
        info!(provider = "digitalocean", service = %spec.name, "Droplet provisioned");

        Ok(CloudService {
            name: spec.name.clone(),
            provider: "digitalocean".into(),
            service_type: ServiceType::VM,
            region: spec.region.clone(),
            status: ServiceStatus::Deploying,
            instances: 1,
            url: None,
            image: Some(image.to_string()),
            resources: spec.resources.clone(),
            created_at: Some(Utc::now().to_rfc3339()),
            provider_id: Some(format!("droplet-{droplet_id}")),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_digitalocean_adapter_creation() {
        let adapter = DigitaloceanAdapter::new("test-token");
        assert_eq!(adapter.provider_name(), "digitalocean");
    }

    #[test]
    fn test_estimate_cost_app() {
        let adapter = DigitaloceanAdapter::new("test");
        let spec = ProvisionSpec {
            name: "my-app".into(),
            instances: 3,
            service_type: ServiceType::Container,
            ..Default::default()
        };
        let rt = tokio::runtime::Runtime::new().unwrap();
        let cost = rt.block_on(adapter.estimate_cost(&spec)).unwrap();
        assert_eq!(cost.monthly, 15.0); // $5 * 3 instances
        assert_eq!(cost.currency, "USD");
    }

    #[test]
    fn test_estimate_cost_droplet() {
        let adapter = DigitaloceanAdapter::new("test");
        let spec = ProvisionSpec {
            name: "my-vm".into(),
            instances: 2,
            service_type: ServiceType::VM,
            ..Default::default()
        };
        let rt = tokio::runtime::Runtime::new().unwrap();
        let cost = rt.block_on(adapter.estimate_cost(&spec)).unwrap();
        assert_eq!(cost.monthly, 8.0); // $4 * 2 instances
        assert_eq!(cost.currency, "USD");
    }
}
