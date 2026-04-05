use async_trait::async_trait;
use chrono::Utc;
use reqwest::Client;
use tracing::info;

use super::{
    CloudAdapter, CloudError, CloudService, CostEstimate, CostLine, DeployConfig, ProvisionSpec,
    Resources, ServiceMetrics, ServiceStatus, ServiceType,
};

const AZURE_MANAGEMENT_API: &str = "https://management.azure.com";
const API_VERSION_ACI: &str = "2023-05-01";
const API_VERSION_COMPUTE: &str = "2023-09-01";

/// Azure adapter supporting Container Instances (ACI), AKS, and Virtual Machines.
///
/// Authentication uses OAuth2 client credentials flow with a service principal.
/// The adapter exchanges `client_id` / `client_secret` for a bearer token via
/// the Microsoft identity platform endpoint.
pub struct AzureAdapter {
    client_id: String,
    client_secret: String,
    tenant_id: String,
    subscription_id: String,
    region: String,
    client: Client,
}

impl AzureAdapter {
    pub fn new(
        client_id: &str,
        client_secret: &str,
        tenant_id: &str,
        subscription_id: &str,
        region: &str,
    ) -> Self {
        Self {
            client_id: client_id.to_string(),
            client_secret: client_secret.to_string(),
            tenant_id: tenant_id.to_string(),
            subscription_id: subscription_id.to_string(),
            region: region.to_string(),
            client: Client::new(),
        }
    }

    /// Acquire a bearer token from Azure AD using the client credentials flow.
    async fn get_access_token(&self) -> Result<String, CloudError> {
        let token_url = format!(
            "https://login.microsoftonline.com/{}/oauth2/v2.0/token",
            self.tenant_id
        );

        let params = [
            ("grant_type", "client_credentials"),
            ("client_id", &self.client_id),
            ("client_secret", &self.client_secret),
            ("scope", "https://management.azure.com/.default"),
        ];

        let resp = self
            .client
            .post(&token_url)
            .form(&params)
            .send()
            .await
            .map_err(|e| CloudError::NetworkError(e.to_string()))?;

        let status = resp.status().as_u16();
        let data: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| CloudError::NetworkError(e.to_string()))?;

        if status >= 400 {
            let err_desc = data["error_description"]
                .as_str()
                .or_else(|| data["error"].as_str())
                .unwrap_or("Unknown auth error");
            return Err(CloudError::AuthError(format!(
                "Azure token exchange failed: {err_desc}"
            )));
        }

        data["access_token"]
            .as_str()
            .map(String::from)
            .ok_or_else(|| CloudError::AuthError("No access_token in response".into()))
    }

    /// Make an authenticated request to the Azure Management REST API.
    async fn api_request(
        &self,
        method: reqwest::Method,
        path: &str,
        api_version: &str,
        body: Option<serde_json::Value>,
    ) -> Result<serde_json::Value, CloudError> {
        let token = self.get_access_token().await?;
        let url = format!("{AZURE_MANAGEMENT_API}{path}?api-version={api_version}");

        let mut req = self
            .client
            .request(method, &url)
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
            return Err(CloudError::AuthError(
                "Azure authentication/authorization failed".into(),
            ));
        }
        if status == 404 {
            return Err(CloudError::NotFound("Azure resource not found".into()));
        }

        // 204 No Content (e.g., successful DELETE)
        if status == 204 {
            return Ok(serde_json::Value::Null);
        }

        let data: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| CloudError::NetworkError(e.to_string()))?;

        if status >= 400 {
            let msg = data["error"]["message"].as_str().unwrap_or("Unknown error");
            return Err(CloudError::ApiError(format!("Azure ({status}): {msg}")));
        }

        Ok(data)
    }

    /// Construct the subscription-level resource path prefix.
    fn sub_path(&self) -> String {
        format!("/subscriptions/{}", self.subscription_id)
    }

    fn parse_container_group(&self, cg: &serde_json::Value) -> CloudService {
        let name = cg["name"].as_str().unwrap_or("unknown").to_string();

        let state = cg["properties"]["instanceView"]["state"]
            .as_str()
            .or_else(|| cg["properties"]["provisioningState"].as_str());

        let status = match state {
            Some("Running") | Some("Succeeded") => ServiceStatus::Running,
            Some("Stopped") | Some("Terminated") => ServiceStatus::Stopped,
            Some("Pending") | Some("Creating") => ServiceStatus::Deploying,
            Some("Failed") => ServiceStatus::Failed,
            _ => ServiceStatus::Unknown,
        };

        let containers = cg["properties"]["containers"].as_array();
        let first_container = containers.and_then(|c| c.first());

        let image = first_container
            .and_then(|c| c["properties"]["image"].as_str())
            .map(String::from);

        let cpu = first_container
            .and_then(|c| c["properties"]["resources"]["requests"]["cpu"].as_f64())
            .map(|c| c.to_string());

        let memory = first_container
            .and_then(|c| c["properties"]["resources"]["requests"]["memoryInGB"].as_f64())
            .map(|m| format!("{m}GB"));

        let ip = cg["properties"]["ipAddress"]["ip"]
            .as_str()
            .map(String::from);

        let instance_count = containers.map(|c| c.len() as u32).unwrap_or(1);

        CloudService {
            name,
            provider: "azure".into(),
            service_type: ServiceType::Container,
            region: cg["location"].as_str().unwrap_or(&self.region).to_string(),
            status,
            instances: instance_count,
            url: ip.map(|addr| format!("http://{addr}")),
            image,
            resources: Some(Resources {
                cpu,
                memory,
                disk: None,
            }),
            created_at: Some(Utc::now().to_rfc3339()),
            provider_id: cg["id"].as_str().map(String::from),
        }
    }

    fn parse_vm(&self, vm: &serde_json::Value) -> CloudService {
        let name = vm["name"].as_str().unwrap_or("unknown").to_string();

        let provisioning_state = vm["properties"]["provisioningState"].as_str();
        let status = match provisioning_state {
            Some("Succeeded") => ServiceStatus::Running,
            Some("Creating") | Some("Updating") => ServiceStatus::Deploying,
            Some("Failed") => ServiceStatus::Failed,
            Some("Deleting") | Some("Deallocating") => ServiceStatus::Stopped,
            _ => ServiceStatus::Unknown,
        };

        let vm_size = vm["properties"]["hardwareProfile"]["vmSize"]
            .as_str()
            .unwrap_or("unknown");

        let image_ref = &vm["properties"]["storageProfile"]["imageReference"];
        let image = image_ref["offer"].as_str().map(|offer| {
            let sku = image_ref["sku"].as_str().unwrap_or("");
            format!("{offer}/{sku}")
        });

        CloudService {
            name,
            provider: "azure".into(),
            service_type: ServiceType::VM,
            region: vm["location"].as_str().unwrap_or(&self.region).to_string(),
            status,
            instances: 1,
            url: None,
            image,
            resources: Some(Resources {
                cpu: Some(vm_size.to_string()),
                memory: None,
                disk: None,
            }),
            created_at: Some(Utc::now().to_rfc3339()),
            provider_id: vm["id"].as_str().map(String::from),
        }
    }
}

#[async_trait]
impl CloudAdapter for AzureAdapter {
    fn provider_name(&self) -> &str {
        "azure"
    }

    async fn list_services(&self) -> Result<Vec<CloudService>, CloudError> {
        let mut services = Vec::new();

        // List Container Instances across all resource groups
        let aci_path = format!(
            "{}/providers/Microsoft.ContainerInstance/containerGroups",
            self.sub_path()
        );
        let aci_data = self
            .api_request(reqwest::Method::GET, &aci_path, API_VERSION_ACI, None)
            .await?;

        if let Some(groups) = aci_data["value"].as_array() {
            for cg in groups {
                services.push(self.parse_container_group(cg));
            }
        }

        // List VMs across all resource groups
        let vm_path = format!(
            "{}/providers/Microsoft.Compute/virtualMachines",
            self.sub_path()
        );
        let vm_data = self
            .api_request(reqwest::Method::GET, &vm_path, API_VERSION_COMPUTE, None)
            .await?;

        if let Some(vms) = vm_data["value"].as_array() {
            for vm in vms {
                services.push(self.parse_vm(vm));
            }
        }

        Ok(services)
    }

    async fn get_service(&self, name: &str) -> Result<CloudService, CloudError> {
        let services = self.list_services().await?;
        services
            .into_iter()
            .find(|s| s.name == name)
            .ok_or_else(|| CloudError::NotFound(format!("Service '{name}' not found on Azure")))
    }

    async fn provision(&self, spec: &ProvisionSpec) -> Result<CloudService, CloudError> {
        let resource_group = "mhost-rg";

        match spec.service_type {
            ServiceType::Container => {
                let image = spec.image.as_deref().unwrap_or("nginx:latest");
                let cpu = spec
                    .resources
                    .as_ref()
                    .and_then(|r| r.cpu.as_deref())
                    .unwrap_or("1.0");
                let memory = spec
                    .resources
                    .as_ref()
                    .and_then(|r| r.memory.as_deref())
                    .unwrap_or("1.5");

                let cpu_val: f64 = cpu.parse().unwrap_or(1.0);
                let mem_val: f64 = parse_memory_gb(memory);

                let env_vars: Vec<serde_json::Value> = spec
                    .env
                    .iter()
                    .map(|(k, v)| {
                        serde_json::json!({
                            "name": k,
                            "value": v,
                        })
                    })
                    .collect();

                let body = serde_json::json!({
                    "location": self.region,
                    "properties": {
                        "containers": [{
                            "name": spec.name,
                            "properties": {
                                "image": image,
                                "resources": {
                                    "requests": {
                                        "cpu": cpu_val,
                                        "memoryInGB": mem_val,
                                    }
                                },
                                "environmentVariables": env_vars,
                                "ports": [{"port": 80}],
                            }
                        }],
                        "osType": "Linux",
                        "ipAddress": {
                            "type": if spec.public { "Public" } else { "Private" },
                            "ports": [{"protocol": "TCP", "port": 80}],
                        },
                        "restartPolicy": "Always",
                    }
                });

                let path = format!(
                    "{}/resourceGroups/{resource_group}/providers/Microsoft.ContainerInstance/containerGroups/{}",
                    self.sub_path(),
                    spec.name
                );

                let result = self
                    .api_request(reqwest::Method::PUT, &path, API_VERSION_ACI, Some(body))
                    .await?;

                info!(provider = "azure", service = %spec.name, "Container group created");

                Ok(CloudService {
                    name: spec.name.clone(),
                    provider: "azure".into(),
                    service_type: ServiceType::Container,
                    region: self.region.clone(),
                    status: ServiceStatus::Deploying,
                    instances: spec.instances,
                    url: result["properties"]["ipAddress"]["ip"]
                        .as_str()
                        .map(|ip| format!("http://{ip}")),
                    image: Some(image.to_string()),
                    resources: Some(Resources {
                        cpu: Some(cpu_val.to_string()),
                        memory: Some(format!("{mem_val}GB")),
                        disk: None,
                    }),
                    created_at: Some(Utc::now().to_rfc3339()),
                    provider_id: result["id"].as_str().map(String::from),
                })
            }
            ServiceType::VM => {
                let vm_size = spec
                    .resources
                    .as_ref()
                    .and_then(|r| r.cpu.as_deref())
                    .unwrap_or("Standard_B1s");

                let body = serde_json::json!({
                    "location": self.region,
                    "properties": {
                        "hardwareProfile": {
                            "vmSize": vm_size,
                        },
                        "storageProfile": {
                            "imageReference": {
                                "publisher": "Canonical",
                                "offer": "0001-com-ubuntu-server-jammy",
                                "sku": "22_04-lts-gen2",
                                "version": "latest",
                            },
                            "osDisk": {
                                "createOption": "FromImage",
                                "managedDisk": {
                                    "storageAccountType": "Standard_LRS",
                                },
                            },
                        },
                        "osProfile": {
                            "computerName": spec.name,
                            "adminUsername": "azureuser",
                        },
                        "networkProfile": {
                            "networkInterfaces": [],
                        },
                    }
                });

                let path = format!(
                    "{}/resourceGroups/{resource_group}/providers/Microsoft.Compute/virtualMachines/{}",
                    self.sub_path(),
                    spec.name
                );

                let result = self
                    .api_request(reqwest::Method::PUT, &path, API_VERSION_COMPUTE, Some(body))
                    .await?;

                info!(provider = "azure", service = %spec.name, "VM created");

                Ok(CloudService {
                    name: spec.name.clone(),
                    provider: "azure".into(),
                    service_type: ServiceType::VM,
                    region: self.region.clone(),
                    status: ServiceStatus::Deploying,
                    instances: 1,
                    url: None,
                    image: Some("ubuntu-22.04".to_string()),
                    resources: Some(Resources {
                        cpu: Some(vm_size.to_string()),
                        memory: None,
                        disk: None,
                    }),
                    created_at: Some(Utc::now().to_rfc3339()),
                    provider_id: result["id"].as_str().map(String::from),
                })
            }
            _ => Err(CloudError::NotSupported(format!(
                "Azure adapter does not support service type '{}'",
                spec.service_type
            ))),
        }
    }

    async fn destroy(&self, name: &str) -> Result<(), CloudError> {
        let service = self.get_service(name).await?;
        let resource_id = service
            .provider_id
            .ok_or_else(|| CloudError::NotFound("No resource ID for service".into()))?;

        let api_version = match service.service_type {
            ServiceType::Container => API_VERSION_ACI,
            ServiceType::VM => API_VERSION_COMPUTE,
            _ => {
                return Err(CloudError::NotSupported(format!(
                    "Cannot destroy service type '{}'",
                    service.service_type
                )));
            }
        };

        self.api_request(reqwest::Method::DELETE, &resource_id, api_version, None)
            .await?;

        info!(provider = "azure", service = %name, "Resource destroyed");
        Ok(())
    }

    async fn deploy(&self, name: &str, config: &DeployConfig) -> Result<CloudService, CloudError> {
        let service = self.get_service(name).await?;
        let resource_id = service
            .provider_id
            .ok_or_else(|| CloudError::NotFound("No resource ID for service".into()))?;

        let port = config.port.unwrap_or(80);
        let env_vars: Vec<serde_json::Value> = config
            .env
            .iter()
            .map(|(k, v)| serde_json::json!({"name": k, "value": v}))
            .collect();

        let body = serde_json::json!({
            "location": self.region,
            "properties": {
                "containers": [{
                    "name": name,
                    "properties": {
                        "image": config.image,
                        "resources": {
                            "requests": {
                                "cpu": 1.0,
                                "memoryInGB": 1.5,
                            }
                        },
                        "environmentVariables": env_vars,
                        "ports": [{"port": port}],
                    }
                }],
                "osType": "Linux",
                "restartPolicy": "Always",
            }
        });

        self.api_request(
            reqwest::Method::PUT,
            &resource_id,
            API_VERSION_ACI,
            Some(body),
        )
        .await?;

        info!(provider = "azure", service = %name, "Deploy triggered");
        self.get_service(name).await
    }

    async fn scale(&self, name: &str, instances: u32) -> Result<CloudService, CloudError> {
        // ACI doesn't support scaling a single container group; return informational error
        // For real scaling, use AKS or Container Apps
        if instances > 1 {
            return Err(CloudError::NotSupported(
                "ACI does not support horizontal scaling. Use AKS or Container Apps.".into(),
            ));
        }

        let mut service = self.get_service(name).await?;
        service.instances = instances;
        Ok(service)
    }

    async fn restart(&self, name: &str) -> Result<(), CloudError> {
        let service = self.get_service(name).await?;
        let resource_id = service
            .provider_id
            .ok_or_else(|| CloudError::NotFound("No resource ID for service".into()))?;

        let restart_path = format!("{resource_id}/restart");
        self.api_request(reqwest::Method::POST, &restart_path, API_VERSION_ACI, None)
            .await?;

        info!(provider = "azure", service = %name, "Container group restarted");
        Ok(())
    }

    async fn logs(&self, name: &str, lines: u32) -> Result<Vec<String>, CloudError> {
        let service = self.get_service(name).await?;
        let resource_id = service
            .provider_id
            .ok_or_else(|| CloudError::NotFound("No resource ID for service".into()))?;

        let logs_path = format!("{resource_id}/containers/{name}/logs");
        let data = self
            .api_request(reqwest::Method::GET, &logs_path, API_VERSION_ACI, None)
            .await?;

        let content = data["content"].as_str().unwrap_or("");
        let log_lines: Vec<String> = content
            .lines()
            .rev()
            .take(lines as usize)
            .map(String::from)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect();

        Ok(log_lines)
    }

    async fn status(&self, name: &str) -> Result<ServiceStatus, CloudError> {
        let service = self.get_service(name).await?;
        Ok(service.status)
    }

    async fn metrics(&self, _name: &str) -> Result<ServiceMetrics, CloudError> {
        Err(CloudError::NotSupported(
            "Use Azure Monitor for detailed metrics".into(),
        ))
    }

    async fn estimate_cost(&self, spec: &ProvisionSpec) -> Result<CostEstimate, CloudError> {
        // ACI pricing: ~$0.0000125/second per vCPU, ~$0.0000015/second per GB
        let cpu_str = spec
            .resources
            .as_ref()
            .and_then(|r| r.cpu.as_deref())
            .unwrap_or("1.0");
        let mem_str = spec
            .resources
            .as_ref()
            .and_then(|r| r.memory.as_deref())
            .unwrap_or("1.5");

        let cpus = cpu_str.parse::<f64>().unwrap_or(1.0);
        let mem_gb = parse_memory_gb(mem_str);

        let seconds_per_hour = 3600.0;
        let hours_per_month = 730.0;

        let cpu_hourly = cpus * 0.0000125 * seconds_per_hour * spec.instances as f64;
        let mem_hourly = mem_gb * 0.0000015 * seconds_per_hour * spec.instances as f64;
        let total_hourly = cpu_hourly + mem_hourly;
        let monthly = total_hourly * hours_per_month;

        Ok(CostEstimate {
            hourly: total_hourly,
            monthly,
            currency: "USD".into(),
            breakdown: vec![
                CostLine {
                    item: format!("{}x {cpus} vCPU (ACI)", spec.instances),
                    amount: cpu_hourly * hours_per_month,
                },
                CostLine {
                    item: format!("{}x {mem_gb:.1} GB RAM (ACI)", spec.instances),
                    amount: mem_hourly * hours_per_month,
                },
            ],
        })
    }
}

fn parse_memory_gb(s: &str) -> f64 {
    let s = s.trim();
    if let Some(gb) = s
        .strip_suffix("GB")
        .or_else(|| s.strip_suffix("Gi"))
        .or_else(|| s.strip_suffix("gb"))
    {
        gb.trim().parse::<f64>().unwrap_or(1.5)
    } else if let Some(mb) = s
        .strip_suffix("MB")
        .or_else(|| s.strip_suffix("Mi"))
        .or_else(|| s.strip_suffix("mb"))
    {
        mb.trim().parse::<f64>().unwrap_or(1536.0) / 1024.0
    } else {
        // Assume raw GB value
        s.parse::<f64>().unwrap_or(1.5)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_azure_adapter_creation() {
        let adapter = AzureAdapter::new(
            "client-id",
            "client-secret",
            "tenant-id",
            "sub-id",
            "eastus",
        );
        assert_eq!(adapter.provider_name(), "azure");
        assert_eq!(adapter.region, "eastus");
        assert_eq!(adapter.subscription_id, "sub-id");
    }

    #[test]
    fn test_estimate_cost() {
        let adapter = AzureAdapter::new(
            "client-id",
            "client-secret",
            "tenant-id",
            "sub-id",
            "eastus",
        );
        let spec = ProvisionSpec {
            name: "api".into(),
            instances: 2,
            resources: Some(Resources {
                cpu: Some("2".into()),
                memory: Some("4GB".into()),
                disk: None,
            }),
            ..Default::default()
        };
        let rt = tokio::runtime::Runtime::new().unwrap();
        let cost = rt.block_on(adapter.estimate_cost(&spec)).unwrap();
        // 2 instances * (2 vCPU * $0.045/hr + 4 GB * $0.0054/hr)
        assert!(cost.hourly > 0.0);
        assert!(cost.monthly > 0.0);
        assert_eq!(cost.currency, "USD");
        assert_eq!(cost.breakdown.len(), 2);
    }
}
