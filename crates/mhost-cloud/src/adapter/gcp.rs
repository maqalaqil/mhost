use async_trait::async_trait;
use chrono::Utc;
use reqwest::Client;
use tracing::info;

use super::{
    CloudAdapter, CloudError, CloudService, CostEstimate, CostLine, DeployConfig, ProvisionSpec,
    Resources, ServiceMetrics, ServiceStatus, ServiceType,
};

/// GCP adapter supporting Cloud Run, GKE, and Compute Engine.
///
/// Authentication uses the `gcloud` CLI as a fallback, which handles service
/// account keys, application-default credentials, and workload identity.  The
/// adapter sets `GOOGLE_APPLICATION_CREDENTIALS` when a credentials file is
/// provided.
#[allow(dead_code)] // client, get_access_token, api_request are used by REST path
pub struct GcpAdapter {
    credentials_file: String,
    project_id: String,
    region: String,
    client: Client,
}

impl GcpAdapter {
    pub fn new(credentials_file: &str, region: &str) -> Self {
        // Attempt to extract project_id from the service account JSON
        let project_id = std::fs::read_to_string(credentials_file)
            .ok()
            .and_then(|content| {
                serde_json::from_str::<serde_json::Value>(&content)
                    .ok()
                    .and_then(|v| v["project_id"].as_str().map(String::from))
            })
            .unwrap_or_default();

        Self {
            credentials_file: credentials_file.to_string(),
            project_id,
            region: region.to_string(),
            client: Client::new(),
        }
    }

    pub fn with_project(self, project_id: &str) -> Self {
        Self {
            project_id: project_id.to_string(),
            ..self
        }
    }

    /// Get an access token via the gcloud CLI.
    #[allow(dead_code)]
    async fn get_access_token(&self) -> Result<String, CloudError> {
        let output = tokio::process::Command::new("gcloud")
            .args(["auth", "print-access-token", "--format=json"])
            .env("GOOGLE_APPLICATION_CREDENTIALS", &self.credentials_file)
            .output()
            .await
            .map_err(|e| {
                CloudError::NetworkError(format!("gcloud CLI not available: {e}"))
            })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(CloudError::AuthError(format!(
                "Failed to get GCP access token: {stderr}"
            )));
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let trimmed = stdout.trim().trim_matches('"');
        if trimmed.is_empty() {
            return Err(CloudError::AuthError(
                "Empty access token from gcloud".into(),
            ));
        }

        Ok(trimmed.to_string())
    }

    /// Execute a gcloud CLI command and return parsed JSON output.
    async fn gcloud_cli(
        &self,
        args: &[&str],
    ) -> Result<serde_json::Value, CloudError> {
        let mut cmd_args = args.to_vec();
        cmd_args.extend_from_slice(&["--format=json", "--project", &self.project_id]);

        let output = tokio::process::Command::new("gcloud")
            .args(&cmd_args)
            .env("GOOGLE_APPLICATION_CREDENTIALS", &self.credentials_file)
            .output()
            .await
            .map_err(|e| CloudError::NetworkError(format!("gcloud CLI not available: {e}")))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            if stderr.contains("PERMISSION_DENIED") || stderr.contains("UNAUTHENTICATED") {
                return Err(CloudError::AuthError(format!("GCP auth failed: {stderr}")));
            }
            return Err(CloudError::ApiError(format!("gcloud error: {stderr}")));
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        if stdout.trim().is_empty() {
            return Ok(serde_json::Value::Array(vec![]));
        }

        serde_json::from_str(&stdout)
            .map_err(|e| CloudError::ApiError(format!("Failed to parse GCP response: {e}")))
    }

    /// Make a direct REST API call to a GCP endpoint with bearer token auth.
    #[allow(dead_code)]
    async fn api_request(
        &self,
        method: reqwest::Method,
        url: &str,
        body: Option<serde_json::Value>,
    ) -> Result<serde_json::Value, CloudError> {
        let token = self.get_access_token().await?;

        let mut req = self
            .client
            .request(method, url)
            .header("Authorization", format!("Bearer {token}"))
            .header("Content-Type", "application/json");

        if let Some(b) = body {
            req = req.json(&b);
        }

        let resp = req
            .send()
            .await
            .map_err(|e| CloudError::NetworkError(e.to_string()))?;

        let status = resp.status().as_u16();
        if status == 401 || status == 403 {
            return Err(CloudError::AuthError("GCP authentication failed".into()));
        }
        if status == 404 {
            return Err(CloudError::NotFound("Resource not found".into()));
        }

        let data: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| CloudError::NetworkError(e.to_string()))?;

        if status >= 400 {
            let msg = data["error"]["message"]
                .as_str()
                .unwrap_or("Unknown error");
            return Err(CloudError::ApiError(format!("GCP ({status}): {msg}")));
        }

        Ok(data)
    }

    fn parse_cloud_run_service(&self, svc: &serde_json::Value) -> CloudService {
        let name = svc["metadata"]["name"]
            .as_str()
            .or_else(|| svc["name"].as_str())
            .unwrap_or("unknown")
            .to_string();

        // Strip the full resource name to just the service name
        let short_name = name
            .rsplit('/')
            .next()
            .unwrap_or(&name)
            .to_string();

        let status_conditions = svc["status"]["conditions"].as_array();
        let is_ready = status_conditions
            .and_then(|conditions| {
                conditions.iter().find(|c| c["type"].as_str() == Some("Ready"))
            })
            .and_then(|c| c["status"].as_str())
            == Some("True");

        let status = if is_ready {
            ServiceStatus::Running
        } else {
            ServiceStatus::Deploying
        };

        let url = svc["status"]["url"]
            .as_str()
            .or_else(|| svc["status"]["address"]["url"].as_str())
            .map(String::from);

        let image = svc["spec"]["template"]["spec"]["containers"]
            .as_array()
            .and_then(|c| c.first())
            .and_then(|c| c["image"].as_str())
            .map(String::from);

        CloudService {
            name: short_name,
            provider: "gcp".into(),
            service_type: ServiceType::Container,
            region: self.region.clone(),
            status,
            instances: 1,
            url,
            image,
            resources: Some(Resources {
                cpu: None,
                memory: None,
                disk: None,
            }),
            created_at: svc["metadata"]["creationTimestamp"]
                .as_str()
                .map(String::from),
            provider_id: svc["metadata"]["uid"]
                .as_str()
                .or_else(|| svc["metadata"]["name"].as_str())
                .map(String::from),
        }
    }

    fn parse_compute_instance(&self, inst: &serde_json::Value) -> CloudService {
        let name = inst["name"].as_str().unwrap_or("unknown").to_string();

        let status = match inst["status"].as_str() {
            Some("RUNNING") => ServiceStatus::Running,
            Some("TERMINATED") | Some("STOPPED") => ServiceStatus::Stopped,
            Some("STAGING") | Some("PROVISIONING") => ServiceStatus::Deploying,
            _ => ServiceStatus::Unknown,
        };

        let external_ip = inst["networkInterfaces"]
            .as_array()
            .and_then(|nics| nics.first())
            .and_then(|nic| nic["accessConfigs"].as_array())
            .and_then(|configs| configs.first())
            .and_then(|c| c["natIP"].as_str())
            .map(String::from);

        let machine_type = inst["machineType"]
            .as_str()
            .and_then(|mt| mt.rsplit('/').next())
            .unwrap_or("unknown");

        CloudService {
            name,
            provider: "gcp".into(),
            service_type: ServiceType::VM,
            region: self.region.clone(),
            status,
            instances: 1,
            url: external_ip.as_ref().map(|ip| format!("http://{ip}")),
            image: None,
            resources: Some(Resources {
                cpu: Some(machine_type.to_string()),
                memory: None,
                disk: None,
            }),
            created_at: inst["creationTimestamp"].as_str().map(String::from),
            provider_id: inst["id"].as_str().map(String::from),
        }
    }
}

#[async_trait]
impl CloudAdapter for GcpAdapter {
    fn provider_name(&self) -> &str {
        "gcp"
    }

    async fn list_services(&self) -> Result<Vec<CloudService>, CloudError> {
        let mut services = Vec::new();

        // List Cloud Run services
        let run_data = self
            .gcloud_cli(&["run", "services", "list", "--region", &self.region])
            .await?;

        if let Some(run_services) = run_data.as_array() {
            for svc in run_services {
                services.push(self.parse_cloud_run_service(svc));
            }
        }

        // List Compute Engine instances
        let zone = format!("{}-a", self.region);
        let compute_data = self
            .gcloud_cli(&[
                "compute",
                "instances",
                "list",
                "--zones",
                &zone,
            ])
            .await?;

        if let Some(instances) = compute_data.as_array() {
            for inst in instances {
                services.push(self.parse_compute_instance(inst));
            }
        }

        Ok(services)
    }

    async fn get_service(&self, name: &str) -> Result<CloudService, CloudError> {
        // Try Cloud Run first
        let run_result = self
            .gcloud_cli(&[
                "run",
                "services",
                "describe",
                name,
                "--region",
                &self.region,
            ])
            .await;

        if let Ok(data) = run_result {
            return Ok(self.parse_cloud_run_service(&data));
        }

        // Try Compute Engine
        let zone = format!("{}-a", self.region);
        let compute_result = self
            .gcloud_cli(&[
                "compute",
                "instances",
                "describe",
                name,
                "--zone",
                &zone,
            ])
            .await;

        if let Ok(data) = compute_result {
            return Ok(self.parse_compute_instance(&data));
        }

        Err(CloudError::NotFound(format!(
            "Service '{name}' not found on GCP"
        )))
    }

    async fn provision(&self, spec: &ProvisionSpec) -> Result<CloudService, CloudError> {
        match spec.service_type {
            ServiceType::Container => {
                let image = spec.image.as_deref().unwrap_or("nginx:latest");
                let mut args = vec![
                    "run",
                    "deploy",
                    &spec.name,
                    "--image",
                    image,
                    "--region",
                    &self.region,
                    "--platform",
                    "managed",
                ];

                if spec.public {
                    args.push("--allow-unauthenticated");
                }

                let cpu = spec
                    .resources
                    .as_ref()
                    .and_then(|r| r.cpu.as_deref())
                    .unwrap_or("1");
                let memory = spec
                    .resources
                    .as_ref()
                    .and_then(|r| r.memory.as_deref())
                    .unwrap_or("512Mi");

                args.extend_from_slice(&["--cpu", cpu, "--memory", memory]);

                self.gcloud_cli(&args).await?;

                info!(provider = "gcp", service = %spec.name, "Cloud Run service deployed");

                Ok(CloudService {
                    name: spec.name.clone(),
                    provider: "gcp".into(),
                    service_type: ServiceType::Container,
                    region: spec.region.clone(),
                    status: ServiceStatus::Deploying,
                    instances: spec.instances,
                    url: None,
                    image: Some(image.to_string()),
                    resources: Some(Resources {
                        cpu: Some(cpu.to_string()),
                        memory: Some(memory.to_string()),
                        disk: None,
                    }),
                    created_at: Some(Utc::now().to_rfc3339()),
                    provider_id: None,
                })
            }
            ServiceType::VM => {
                let machine_type = spec
                    .resources
                    .as_ref()
                    .and_then(|r| r.cpu.as_deref())
                    .unwrap_or("e2-micro");
                let zone = format!("{}-a", spec.region);
                let image_family = spec
                    .image
                    .as_deref()
                    .unwrap_or("debian-11");

                let result = self
                    .gcloud_cli(&[
                        "compute",
                        "instances",
                        "create",
                        &spec.name,
                        "--zone",
                        &zone,
                        "--machine-type",
                        machine_type,
                        "--image-family",
                        image_family,
                        "--image-project",
                        "debian-cloud",
                    ])
                    .await?;

                info!(provider = "gcp", service = %spec.name, "Compute instance created");

                Ok(CloudService {
                    name: spec.name.clone(),
                    provider: "gcp".into(),
                    service_type: ServiceType::VM,
                    region: spec.region.clone(),
                    status: ServiceStatus::Deploying,
                    instances: spec.instances,
                    url: None,
                    image: Some(image_family.to_string()),
                    resources: Some(Resources {
                        cpu: Some(machine_type.to_string()),
                        memory: None,
                        disk: None,
                    }),
                    created_at: Some(Utc::now().to_rfc3339()),
                    provider_id: result
                        .as_array()
                        .and_then(|a| a.first())
                        .and_then(|i| i["id"].as_str())
                        .map(String::from),
                })
            }
            _ => Err(CloudError::NotSupported(format!(
                "GCP adapter does not support service type '{}'",
                spec.service_type
            ))),
        }
    }

    async fn destroy(&self, name: &str) -> Result<(), CloudError> {
        // Try Cloud Run first
        let run_result = self
            .gcloud_cli(&[
                "run",
                "services",
                "delete",
                name,
                "--region",
                &self.region,
                "--quiet",
            ])
            .await;

        if run_result.is_ok() {
            info!(provider = "gcp", service = %name, "Cloud Run service destroyed");
            return Ok(());
        }

        // Try Compute Engine
        let zone = format!("{}-a", self.region);
        self.gcloud_cli(&[
            "compute",
            "instances",
            "delete",
            name,
            "--zone",
            &zone,
            "--quiet",
        ])
        .await?;

        info!(provider = "gcp", service = %name, "Compute instance destroyed");
        Ok(())
    }

    async fn deploy(
        &self,
        name: &str,
        config: &DeployConfig,
    ) -> Result<CloudService, CloudError> {
        let mut args = vec![
            "run",
            "deploy",
            name,
            "--image",
            &config.image,
            "--region",
            &self.region,
            "--platform",
            "managed",
        ];

        // Build env var string for the CLI
        let env_string: String = config
            .env
            .iter()
            .map(|(k, v)| format!("{k}={v}"))
            .collect::<Vec<_>>()
            .join(",");

        if !config.env.is_empty() {
            args.extend_from_slice(&["--set-env-vars", &env_string]);
        }

        self.gcloud_cli(&args).await?;

        info!(provider = "gcp", service = %name, "Deploy triggered");
        self.get_service(name).await
    }

    async fn scale(&self, name: &str, instances: u32) -> Result<CloudService, CloudError> {
        let max_str = instances.to_string();
        self.gcloud_cli(&[
            "run",
            "services",
            "update",
            name,
            "--region",
            &self.region,
            "--max-instances",
            &max_str,
        ])
        .await?;

        info!(provider = "gcp", service = %name, instances, "Scaled");

        let mut service = self.get_service(name).await?;
        service.instances = instances;
        Ok(service)
    }

    async fn restart(&self, name: &str) -> Result<(), CloudError> {
        // Cloud Run doesn't have a direct restart; redeploy the same revision
        self.gcloud_cli(&[
            "run",
            "services",
            "update",
            name,
            "--region",
            &self.region,
            "--no-traffic",
        ])
        .await?;

        // Restore traffic
        self.gcloud_cli(&[
            "run",
            "services",
            "update-traffic",
            name,
            "--region",
            &self.region,
            "--to-latest",
        ])
        .await?;

        info!(provider = "gcp", service = %name, "Restarted via traffic shift");
        Ok(())
    }

    async fn logs(&self, name: &str, lines: u32) -> Result<Vec<String>, CloudError> {
        let limit_str = lines.to_string();
        let data = self
            .gcloud_cli(&[
                "logging",
                "read",
                &format!("resource.labels.service_name=\"{name}\""),
                "--limit",
                &limit_str,
            ])
            .await?;

        let log_lines = data
            .as_array()
            .map(|entries| {
                entries
                    .iter()
                    .filter_map(|e| {
                        e["textPayload"]
                            .as_str()
                            .or_else(|| e["jsonPayload"]["message"].as_str())
                            .map(String::from)
                    })
                    .collect()
            })
            .unwrap_or_default();

        Ok(log_lines)
    }

    async fn status(&self, name: &str) -> Result<ServiceStatus, CloudError> {
        let service = self.get_service(name).await?;
        Ok(service.status)
    }

    async fn metrics(&self, _name: &str) -> Result<ServiceMetrics, CloudError> {
        Err(CloudError::NotSupported(
            "Use Google Cloud Monitoring for detailed metrics".into(),
        ))
    }

    async fn estimate_cost(&self, spec: &ProvisionSpec) -> Result<CostEstimate, CloudError> {
        // Cloud Run pricing: $0.00002400/vCPU-second, $0.0000025/GiB-second
        let cpu_str = spec
            .resources
            .as_ref()
            .and_then(|r| r.cpu.as_deref())
            .unwrap_or("1");
        let mem_str = spec
            .resources
            .as_ref()
            .and_then(|r| r.memory.as_deref())
            .unwrap_or("512Mi");

        let cpus = cpu_str.parse::<f64>().unwrap_or(1.0);
        let mem_gib = parse_memory_gib(mem_str);

        let seconds_per_hour = 3600.0;
        let hours_per_month = 730.0;

        let cpu_hourly = cpus * 0.0000240 * seconds_per_hour * spec.instances as f64;
        let mem_hourly = mem_gib * 0.0000025 * seconds_per_hour * spec.instances as f64;
        let total_hourly = cpu_hourly + mem_hourly;
        let monthly = total_hourly * hours_per_month;

        Ok(CostEstimate {
            hourly: total_hourly,
            monthly,
            currency: "USD".into(),
            breakdown: vec![
                CostLine {
                    item: format!("{}x {cpus} vCPU (Cloud Run)", spec.instances),
                    amount: cpu_hourly * hours_per_month,
                },
                CostLine {
                    item: format!("{}x {mem_gib:.2} GiB RAM (Cloud Run)", spec.instances),
                    amount: mem_hourly * hours_per_month,
                },
            ],
        })
    }
}

fn parse_memory_gib(s: &str) -> f64 {
    let s = s.trim();
    if let Some(gb) = s
        .strip_suffix("Gi")
        .or_else(|| s.strip_suffix("GB"))
        .or_else(|| s.strip_suffix("gb"))
    {
        gb.trim().parse::<f64>().unwrap_or(0.5)
    } else if let Some(mb) = s
        .strip_suffix("Mi")
        .or_else(|| s.strip_suffix("MB"))
        .or_else(|| s.strip_suffix("mb"))
    {
        mb.trim().parse::<f64>().unwrap_or(512.0) / 1024.0
    } else {
        // Assume MB if no suffix
        s.parse::<f64>().unwrap_or(512.0) / 1024.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_gcp_adapter_creation() {
        let adapter = GcpAdapter::new("/tmp/nonexistent-sa.json", "us-central1");
        assert_eq!(adapter.provider_name(), "gcp");
        assert_eq!(adapter.region, "us-central1");
    }

    #[test]
    fn test_estimate_cost() {
        let adapter = GcpAdapter::new("/tmp/sa.json", "us-central1");
        let spec = ProvisionSpec {
            name: "api".into(),
            instances: 2,
            resources: Some(Resources {
                cpu: Some("2".into()),
                memory: Some("1Gi".into()),
                disk: None,
            }),
            ..Default::default()
        };
        let rt = tokio::runtime::Runtime::new().unwrap();
        let cost = rt.block_on(adapter.estimate_cost(&spec)).unwrap();
        // 2 instances * (2 vCPU * $0.0864/hr + 1 GiB * $0.009/hr)
        assert!(cost.hourly > 0.0);
        assert!(cost.monthly > 0.0);
        assert_eq!(cost.currency, "USD");
        assert_eq!(cost.breakdown.len(), 2);
    }
}
