use async_trait::async_trait;
use chrono::Utc;
use reqwest::Client;
use tracing::info;

use super::{
    CloudAdapter, CloudError, CloudService, CostEstimate, CostLine, DeployConfig,
    ProvisionSpec, Resources, ServiceMetrics, ServiceStatus, ServiceType,
};

const FLY_API: &str = "https://api.machines.dev/v1";

pub struct FlyAdapter {
    token: String,
    client: Client,
    org: String,
}

impl FlyAdapter {
    pub fn new(token: &str) -> Self {
        Self {
            token: token.to_string(),
            client: Client::new(),
            org: "personal".to_string(),
        }
    }

    pub fn with_org(mut self, org: &str) -> Self {
        self.org = org.to_string();
        self
    }

    async fn api_get(
        &self,
        path: &str,
    ) -> Result<serde_json::Value, CloudError> {
        let url = format!("{FLY_API}{path}");
        let resp = self
            .client
            .get(&url)
            .header("Authorization", format!("Bearer {}", self.token))
            .send()
            .await
            .map_err(|e| CloudError::NetworkError(e.to_string()))?;

        let status = resp.status().as_u16();
        if status == 401 {
            return Err(CloudError::AuthError(
                "Invalid Fly.io token".into(),
            ));
        }
        if status == 404 {
            return Err(CloudError::NotFound(format!(
                "Not found: {path}"
            )));
        }

        let data: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| CloudError::NetworkError(e.to_string()))?;

        if status >= 400 {
            let msg =
                data["error"].as_str().unwrap_or("Unknown error");
            return Err(CloudError::ApiError(format!(
                "fly ({status}): {msg}"
            )));
        }
        Ok(data)
    }

    async fn api_post(
        &self,
        path: &str,
        body: serde_json::Value,
    ) -> Result<serde_json::Value, CloudError> {
        let url = format!("{FLY_API}{path}");
        let resp = self
            .client
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.token))
            .json(&body)
            .send()
            .await
            .map_err(|e| CloudError::NetworkError(e.to_string()))?;

        let status = resp.status().as_u16();
        if status == 401 {
            return Err(CloudError::AuthError(
                "Invalid Fly.io token".into(),
            ));
        }

        let data: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| CloudError::NetworkError(e.to_string()))?;

        if status >= 400 {
            let msg =
                data["error"].as_str().unwrap_or("Unknown error");
            return Err(CloudError::ApiError(format!(
                "fly ({status}): {msg}"
            )));
        }
        Ok(data)
    }

    async fn api_delete(&self, path: &str) -> Result<(), CloudError> {
        let url = format!("{FLY_API}{path}");
        let resp = self
            .client
            .delete(&url)
            .header("Authorization", format!("Bearer {}", self.token))
            .send()
            .await
            .map_err(|e| CloudError::NetworkError(e.to_string()))?;

        let status = resp.status().as_u16();
        if status == 401 {
            return Err(CloudError::AuthError(
                "Invalid Fly.io token".into(),
            ));
        }
        if status >= 400 && status != 404 {
            return Err(CloudError::ApiError(format!(
                "fly ({status}): Delete failed"
            )));
        }
        Ok(())
    }

    fn parse_machine(
        &self,
        app_name: &str,
        machine: &serde_json::Value,
    ) -> CloudService {
        let status = match machine["state"].as_str() {
            Some("started") | Some("running") => ServiceStatus::Running,
            Some("stopped") => ServiceStatus::Stopped,
            Some("created") | Some("starting") => {
                ServiceStatus::Deploying
            }
            _ => ServiceStatus::Unknown,
        };

        let config = &machine["config"];
        let image = config["image"].as_str().map(String::from);

        let resources = Some(Resources {
            cpu: config["guest"]["cpus"]
                .as_u64()
                .map(|c| c.to_string()),
            memory: config["guest"]["memory_mb"]
                .as_u64()
                .map(|m| format!("{m}MB")),
            disk: None,
        });

        CloudService {
            name: app_name.to_string(),
            provider: "fly".into(),
            service_type: ServiceType::Container,
            region: machine["region"]
                .as_str()
                .unwrap_or("")
                .to_string(),
            status,
            instances: 1,
            url: Some(format!("https://{app_name}.fly.dev")),
            image,
            resources,
            created_at: Some(Utc::now().to_rfc3339()),
            provider_id: machine["id"]
                .as_str()
                .map(String::from),
        }
    }
}

#[async_trait]
impl CloudAdapter for FlyAdapter {
    fn provider_name(&self) -> &str {
        "fly"
    }

    async fn list_services(
        &self,
    ) -> Result<Vec<CloudService>, CloudError> {
        let data = self
            .api_get(&format!("/apps?org_slug={}", self.org))
            .await?;
        let empty = vec![];
        let apps = data.as_array().unwrap_or(&empty);
        let mut services = Vec::new();

        for app in apps {
            let app_name = app["name"].as_str().unwrap_or("");
            if app_name.is_empty() {
                continue;
            }

            let status = match app["status"].as_str() {
                Some("deployed") => ServiceStatus::Running,
                Some("suspended") => ServiceStatus::Stopped,
                Some("pending") => ServiceStatus::Deploying,
                _ => ServiceStatus::Unknown,
            };

            services.push(CloudService {
                name: app_name.to_string(),
                provider: "fly".into(),
                service_type: ServiceType::Container,
                region: app["currentRelease"]["region"]
                    .as_str()
                    .unwrap_or("")
                    .to_string(),
                status,
                instances: app["machineCount"]
                    .as_u64()
                    .unwrap_or(0) as u32,
                url: Some(format!("https://{app_name}.fly.dev")),
                image: None,
                resources: Some(Resources {
                    cpu: None,
                    memory: None,
                    disk: None,
                }),
                created_at: Some(Utc::now().to_rfc3339()),
                provider_id: app["id"].as_str().map(String::from),
            });
        }
        Ok(services)
    }

    async fn get_service(
        &self,
        name: &str,
    ) -> Result<CloudService, CloudError> {
        let machines =
            self.api_get(&format!("/apps/{name}/machines")).await?;
        let machine_list = machines.as_array().ok_or_else(|| {
            CloudError::NotFound(format!(
                "App '{name}' not found on Fly.io"
            ))
        })?;

        if machine_list.is_empty() {
            return Err(CloudError::NotFound(format!(
                "No machines for app '{name}'"
            )));
        }

        let mut service =
            self.parse_machine(name, &machine_list[0]);
        service.instances = machine_list.len() as u32;
        Ok(service)
    }

    async fn provision(
        &self,
        spec: &ProvisionSpec,
    ) -> Result<CloudService, CloudError> {
        // Step 1: Create app
        let app_data = self
            .api_post(
                "/apps",
                serde_json::json!({
                    "app_name": spec.name,
                    "org_slug": self.org,
                }),
            )
            .await?;

        // Step 2: Create machine
        let cpu_str = spec
            .resources
            .as_ref()
            .and_then(|r| r.cpu.as_deref())
            .unwrap_or("1");
        let mem_str = spec
            .resources
            .as_ref()
            .and_then(|r| r.memory.as_deref())
            .unwrap_or("256MB");

        let machine_config = serde_json::json!({
            "config": {
                "image": spec.image.as_deref().unwrap_or("nginx:latest"),
                "guest": {
                    "cpus": cpu_str.parse::<u32>().unwrap_or(1),
                    "memory_mb": parse_memory_mb(mem_str),
                    "cpu_kind": "shared",
                },
                "env": spec.env,
            },
            "region": spec.region,
        });

        let machine = self
            .api_post(
                &format!("/apps/{}/machines", spec.name),
                machine_config,
            )
            .await?;

        info!(provider = "fly", app = %spec.name, "Machine provisioned");

        Ok(CloudService {
            name: spec.name.clone(),
            provider: "fly".into(),
            service_type: ServiceType::Container,
            region: spec.region.clone(),
            status: ServiceStatus::Deploying,
            instances: 1,
            url: Some(format!(
                "https://{}.fly.dev",
                app_data["name"].as_str().unwrap_or("")
            )),
            image: spec.image.clone(),
            resources: Some(Resources {
                cpu: spec
                    .resources
                    .as_ref()
                    .and_then(|r| r.cpu.clone()),
                memory: spec
                    .resources
                    .as_ref()
                    .and_then(|r| r.memory.clone()),
                disk: None,
            }),
            created_at: Some(Utc::now().to_rfc3339()),
            provider_id: machine["id"].as_str().map(String::from),
        })
    }

    async fn destroy(&self, name: &str) -> Result<(), CloudError> {
        self.api_delete(&format!("/apps/{name}")).await?;
        info!(provider = "fly", app = %name, "App destroyed");
        Ok(())
    }

    async fn deploy(
        &self,
        name: &str,
        config: &DeployConfig,
    ) -> Result<CloudService, CloudError> {
        let machines =
            self.api_get(&format!("/apps/{name}/machines")).await?;
        let machine_list = machines.as_array().ok_or_else(|| {
            CloudError::NotFound(format!("App '{name}' not found"))
        })?;

        for machine in machine_list {
            let machine_id =
                machine["id"].as_str().unwrap_or("");
            let mut update = machine["config"].clone();
            update["image"] = serde_json::json!(config.image);

            if !config.env.is_empty() {
                let existing_env = update["env"]
                    .as_object()
                    .cloned()
                    .unwrap_or_default();
                let mut merged = existing_env;
                for (k, v) in &config.env {
                    merged.insert(k.clone(), serde_json::json!(v));
                }
                update["env"] = serde_json::json!(merged);
            }

            self.api_post(
                &format!("/apps/{name}/machines/{machine_id}"),
                serde_json::json!({ "config": update }),
            )
            .await?;
        }

        info!(provider = "fly", app = %name, "Deploy complete");
        self.get_service(name).await
    }

    async fn scale(
        &self,
        name: &str,
        instances: u32,
    ) -> Result<CloudService, CloudError> {
        let machines =
            self.api_get(&format!("/apps/{name}/machines")).await?;
        let machine_list = machines.as_array().ok_or_else(|| {
            CloudError::NotFound(format!("App '{name}' not found"))
        })?;

        let current = machine_list.len() as u32;
        if instances > current {
            // Scale up: clone first machine config
            if let Some(template) = machine_list.first() {
                let config = template["config"].clone();
                let region = template["region"]
                    .as_str()
                    .unwrap_or("iad");
                for _ in 0..(instances - current) {
                    self.api_post(
                        &format!("/apps/{name}/machines"),
                        serde_json::json!({
                            "config": config,
                            "region": region,
                        }),
                    )
                    .await?;
                }
            }
        } else if instances < current {
            // Scale down: destroy excess machines
            for machine in
                machine_list.iter().skip(instances as usize)
            {
                let id = machine["id"].as_str().unwrap_or("");
                let _ = self
                    .api_delete(&format!(
                        "/apps/{name}/machines/{id}"
                    ))
                    .await;
            }
        }

        info!(provider = "fly", app = %name, from = current, to = instances, "Scaled");

        let mut service = self.get_service(name).await?;
        service.instances = instances;
        Ok(service)
    }

    async fn restart(&self, name: &str) -> Result<(), CloudError> {
        let machines =
            self.api_get(&format!("/apps/{name}/machines")).await?;
        let machine_list = machines.as_array().ok_or_else(|| {
            CloudError::NotFound(format!("App '{name}' not found"))
        })?;

        for machine in machine_list {
            let id = machine["id"].as_str().unwrap_or("");
            let _ = self
                .api_post(
                    &format!(
                        "/apps/{name}/machines/{id}/restart"
                    ),
                    serde_json::json!({}),
                )
                .await;
        }

        info!(provider = "fly", app = %name, "Restarted all machines");
        Ok(())
    }

    async fn logs(
        &self,
        _name: &str,
        _lines: u32,
    ) -> Result<Vec<String>, CloudError> {
        // Fly logs require the Nats-based log stream, not available via REST
        Err(CloudError::NotSupported(
            "Use 'flyctl logs' for Fly.io log streaming".into(),
        ))
    }

    async fn status(
        &self,
        name: &str,
    ) -> Result<ServiceStatus, CloudError> {
        let service = self.get_service(name).await?;
        Ok(service.status)
    }

    async fn metrics(
        &self,
        _name: &str,
    ) -> Result<ServiceMetrics, CloudError> {
        Err(CloudError::NotSupported(
            "Fly.io metrics available via Prometheus endpoint only"
                .into(),
        ))
    }

    async fn estimate_cost(
        &self,
        spec: &ProvisionSpec,
    ) -> Result<CostEstimate, CloudError> {
        // Fly.io pricing: shared-cpu-1x ~$1.94/mo, performance-1x ~$29/mo
        let cpu_str = spec
            .resources
            .as_ref()
            .and_then(|r| r.cpu.as_deref())
            .unwrap_or("1");
        let mem_str = spec
            .resources
            .as_ref()
            .and_then(|r| r.memory.as_deref())
            .unwrap_or("256MB");

        let cpus = cpu_str.parse::<f64>().unwrap_or(1.0);
        let memory_mb = parse_memory_mb(mem_str) as f64;
        let cpu_cost = cpus * 1.94 * spec.instances as f64;
        let mem_cost =
            (memory_mb / 256.0) * 1.94 * spec.instances as f64;
        let monthly = cpu_cost + mem_cost;
        Ok(CostEstimate {
            hourly: monthly / 730.0,
            monthly,
            currency: "USD".into(),
            breakdown: vec![
                CostLine {
                    item: format!(
                        "{}x {cpus} vCPU",
                        spec.instances
                    ),
                    amount: cpu_cost,
                },
                CostLine {
                    item: format!(
                        "{}x {memory_mb}MB RAM",
                        spec.instances
                    ),
                    amount: mem_cost,
                },
            ],
        })
    }
}

fn parse_memory_mb(s: &str) -> u32 {
    let s = s.trim().to_uppercase();
    if let Some(gb) = s.strip_suffix("GB") {
        gb.trim().parse::<u32>().unwrap_or(1) * 1024
    } else if let Some(mb) = s.strip_suffix("MB") {
        mb.trim().parse::<u32>().unwrap_or(256)
    } else {
        s.parse::<u32>().unwrap_or(256)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fly_adapter_creation() {
        let adapter = FlyAdapter::new("test-token");
        assert_eq!(adapter.provider_name(), "fly");
    }

    #[test]
    fn test_parse_memory_mb() {
        assert_eq!(parse_memory_mb("256MB"), 256);
        assert_eq!(parse_memory_mb("1GB"), 1024);
        assert_eq!(parse_memory_mb("512mb"), 512);
        assert_eq!(parse_memory_mb("2GB"), 2048);
        assert_eq!(parse_memory_mb("256"), 256);
    }

    #[test]
    fn test_estimate_cost() {
        let adapter = FlyAdapter::new("test");
        let spec = ProvisionSpec {
            name: "api".into(),
            instances: 2,
            resources: Some(Resources {
                cpu: Some("1".into()),
                memory: Some("512MB".into()),
                disk: None,
            }),
            ..Default::default()
        };
        let rt = tokio::runtime::Runtime::new().unwrap();
        let cost = rt.block_on(adapter.estimate_cost(&spec)).unwrap();
        assert!(cost.monthly > 0.0);
        assert_eq!(cost.currency, "USD");
        assert_eq!(cost.breakdown.len(), 2);
    }
}
