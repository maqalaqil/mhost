use async_trait::async_trait;
use chrono::Utc;
use reqwest::Client;
use tracing::info;

use super::{
    CloudAdapter, CloudError, CloudService, CostEstimate, CostLine, DeployConfig, ProvisionSpec,
    Resources, ServiceMetrics, ServiceStatus, ServiceType,
};

const CLOUDFLARE_API: &str = "https://api.cloudflare.com/client/v4";

pub struct CloudflareAdapter {
    api_token: String,
    account_id: String,
    client: Client,
}

impl CloudflareAdapter {
    pub fn new(api_token: &str, account_id: &str) -> Self {
        Self {
            api_token: api_token.to_string(),
            account_id: account_id.to_string(),
            client: Client::new(),
        }
    }

    fn scripts_url(&self) -> String {
        format!(
            "{CLOUDFLARE_API}/accounts/{}/workers/scripts",
            self.account_id
        )
    }

    async fn api_get(&self, url: &str) -> Result<serde_json::Value, CloudError> {
        let resp = self
            .client
            .get(url)
            .header("Authorization", format!("Bearer {}", self.api_token))
            .send()
            .await
            .map_err(|e| CloudError::NetworkError(e.to_string()))?;

        let status = resp.status().as_u16();
        if status == 401 || status == 403 {
            return Err(CloudError::AuthError("Invalid Cloudflare API token".into()));
        }

        let body: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| CloudError::NetworkError(e.to_string()))?;

        if body["success"].as_bool() != Some(true) {
            let msg = body["errors"][0]["message"]
                .as_str()
                .unwrap_or("Unknown Cloudflare error");
            return Err(CloudError::ApiError(format!(
                "cloudflare ({status}): {msg}"
            )));
        }

        Ok(body["result"].clone())
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
            .header("Authorization", format!("Bearer {}", self.api_token))
            .header("Content-Type", "application/json");

        if let Some(json_body) = body {
            req = req.json(&json_body);
        }

        let resp = req
            .send()
            .await
            .map_err(|e| CloudError::NetworkError(e.to_string()))?;

        let status = resp.status().as_u16();
        if status == 401 || status == 403 {
            return Err(CloudError::AuthError("Invalid Cloudflare API token".into()));
        }

        let data: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| CloudError::NetworkError(e.to_string()))?;

        if data["success"].as_bool() != Some(true) {
            let msg = data["errors"][0]["message"]
                .as_str()
                .unwrap_or("Unknown Cloudflare error");
            return Err(CloudError::ApiError(format!(
                "cloudflare ({status}): {msg}"
            )));
        }

        Ok(data["result"].clone())
    }

    fn parse_worker(&self, worker: &serde_json::Value) -> CloudService {
        let name = worker["id"].as_str().unwrap_or("unknown").to_string();
        let created_on = worker["created_on"].as_str().map(String::from);

        CloudService {
            name,
            provider: "cloudflare".into(),
            service_type: ServiceType::EdgeFunction,
            region: "global".to_string(),
            status: ServiceStatus::Running,
            instances: 1,
            url: None,
            image: None,
            resources: Some(Resources {
                cpu: None,
                memory: None,
                disk: None,
            }),
            created_at: created_on,
            provider_id: worker["id"].as_str().map(String::from),
        }
    }
}

#[async_trait]
impl CloudAdapter for CloudflareAdapter {
    fn provider_name(&self) -> &str {
        "cloudflare"
    }

    async fn list_services(&self) -> Result<Vec<CloudService>, CloudError> {
        let data = self.api_get(&self.scripts_url()).await?;

        let services = data
            .as_array()
            .map(|arr| arr.iter().map(|w| self.parse_worker(w)).collect())
            .unwrap_or_default();

        Ok(services)
    }

    async fn get_service(&self, name: &str) -> Result<CloudService, CloudError> {
        let url = format!("{}/{name}", self.scripts_url());
        let data = self.api_get(&url).await?;
        Ok(self.parse_worker(&data))
    }

    async fn provision(&self, spec: &ProvisionSpec) -> Result<CloudService, CloudError> {
        // Workers are created by uploading a script
        let url = format!("{}/{}", self.scripts_url(), spec.name);
        let script = "export default { async fetch(request) { return new Response('Hello'); } }";

        let resp = self
            .client
            .put(&url)
            .header("Authorization", format!("Bearer {}", self.api_token))
            .header("Content-Type", "application/javascript")
            .body(script.to_string())
            .send()
            .await
            .map_err(|e| CloudError::NetworkError(e.to_string()))?;

        let status = resp.status().as_u16();
        if status == 401 || status == 403 {
            return Err(CloudError::AuthError("Invalid Cloudflare API token".into()));
        }

        let data: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| CloudError::NetworkError(e.to_string()))?;

        if data["success"].as_bool() != Some(true) {
            let msg = data["errors"][0]["message"]
                .as_str()
                .unwrap_or("Failed to create worker");
            return Err(CloudError::ApiError(format!(
                "cloudflare ({status}): {msg}"
            )));
        }

        info!(provider = "cloudflare", service = %spec.name, "Worker provisioned");

        Ok(CloudService {
            name: spec.name.clone(),
            provider: "cloudflare".into(),
            service_type: ServiceType::EdgeFunction,
            region: "global".to_string(),
            status: ServiceStatus::Running,
            instances: 1,
            url: Some(format!(
                "https://{}.{}.workers.dev",
                spec.name, self.account_id
            )),
            image: None,
            resources: Some(Resources {
                cpu: None,
                memory: None,
                disk: None,
            }),
            created_at: Some(Utc::now().to_rfc3339()),
            provider_id: Some(spec.name.clone()),
        })
    }

    async fn destroy(&self, name: &str) -> Result<(), CloudError> {
        let url = format!("{}/{name}", self.scripts_url());
        self.api_request(reqwest::Method::DELETE, &url, None)
            .await?;
        info!(provider = "cloudflare", service = %name, "Worker destroyed");
        Ok(())
    }

    async fn deploy(&self, name: &str, _config: &DeployConfig) -> Result<CloudService, CloudError> {
        // Re-upload worker script to deploy update
        let url = format!("{}/{name}", self.scripts_url());
        let script =
            format!("export default {{ async fetch(request) {{ return new Response('Updated {name}'); }} }}");

        let resp = self
            .client
            .put(&url)
            .header("Authorization", format!("Bearer {}", self.api_token))
            .header("Content-Type", "application/javascript")
            .body(script)
            .send()
            .await
            .map_err(|e| CloudError::NetworkError(e.to_string()))?;

        let status = resp.status().as_u16();
        if status == 401 || status == 403 {
            return Err(CloudError::AuthError("Invalid Cloudflare API token".into()));
        }

        info!(provider = "cloudflare", service = %name, "Worker deployed");
        self.get_service(name).await
    }

    async fn scale(&self, _name: &str, _instances: u32) -> Result<CloudService, CloudError> {
        Err(CloudError::NotSupported(
            "Cloudflare Workers scale automatically at the edge".into(),
        ))
    }

    async fn restart(&self, _name: &str) -> Result<(), CloudError> {
        Err(CloudError::NotSupported(
            "Cloudflare Workers are stateless and cannot be restarted".into(),
        ))
    }

    async fn logs(&self, _name: &str, _lines: u32) -> Result<Vec<String>, CloudError> {
        Err(CloudError::NotSupported(
            "Cloudflare Workers logs require wrangler tail or Logpush".into(),
        ))
    }

    async fn status(&self, name: &str) -> Result<ServiceStatus, CloudError> {
        let service = self.get_service(name).await?;
        Ok(service.status)
    }

    async fn metrics(&self, _name: &str) -> Result<ServiceMetrics, CloudError> {
        Err(CloudError::NotSupported(
            "Cloudflare Workers analytics requires GraphQL Analytics API".into(),
        ))
    }

    async fn estimate_cost(&self, spec: &ProvisionSpec) -> Result<CostEstimate, CloudError> {
        // Workers free: 100K req/day. Paid: $5/mo for 10M requests/mo
        let instances = spec.instances as f64;
        let monthly = 5.0 * instances;
        Ok(CostEstimate {
            hourly: monthly / 730.0,
            monthly,
            currency: "USD".into(),
            breakdown: vec![
                CostLine {
                    item: "Workers Paid plan (10M requests/mo included)".into(),
                    amount: 5.0 * instances,
                },
                CostLine {
                    item: "Free tier: 100K requests/day".into(),
                    amount: 0.0,
                },
            ],
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cloudflare_adapter_creation() {
        let adapter = CloudflareAdapter::new("test-token", "account-123");
        assert_eq!(adapter.provider_name(), "cloudflare");
    }

    #[test]
    fn test_estimate_cost() {
        let adapter = CloudflareAdapter::new("test", "acc");
        let spec = ProvisionSpec {
            name: "my-worker".into(),
            instances: 2,
            service_type: ServiceType::EdgeFunction,
            ..Default::default()
        };
        let rt = tokio::runtime::Runtime::new().unwrap();
        let cost = rt.block_on(adapter.estimate_cost(&spec)).unwrap();
        assert_eq!(cost.monthly, 10.0); // $5 * 2 instances
        assert_eq!(cost.currency, "USD");
        assert_eq!(cost.breakdown.len(), 2);
    }
}
