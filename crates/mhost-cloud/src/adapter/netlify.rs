use async_trait::async_trait;
use chrono::Utc;
use reqwest::Client;
use tracing::info;

use super::{
    CloudAdapter, CloudError, CloudService, CostEstimate, CostLine, DeployConfig, ProvisionSpec,
    Resources, ServiceMetrics, ServiceStatus, ServiceType,
};

const NETLIFY_API: &str = "https://api.netlify.com/api/v1";

pub struct NetlifyAdapter {
    token: String,
    client: Client,
}

impl NetlifyAdapter {
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
            return Err(CloudError::AuthError("Invalid Netlify token".into()));
        }
        if status == 404 {
            return Err(CloudError::NotFound("Resource not found on Netlify".into()));
        }

        let body: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| CloudError::NetworkError(e.to_string()))?;

        if let Some(msg) = body.get("message").and_then(|m| m.as_str()) {
            if status >= 400 {
                return Err(CloudError::ApiError(format!("netlify ({status}): {msg}")));
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
            return Err(CloudError::AuthError("Invalid Netlify token".into()));
        }

        let data: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| CloudError::NetworkError(e.to_string()))?;

        if let Some(msg) = data.get("message").and_then(|m| m.as_str()) {
            if status >= 400 {
                return Err(CloudError::ApiError(format!("netlify ({status}): {msg}")));
            }
        }

        Ok(data)
    }

    fn parse_site(&self, site: &serde_json::Value) -> CloudService {
        let name = site["name"].as_str().unwrap_or("unknown").to_string();
        let id = site["id"].as_str().unwrap_or("").to_string();

        let has_functions = site["capabilities"]["functions"].is_object();
        let service_type = if has_functions {
            ServiceType::Serverless
        } else {
            ServiceType::StaticSite
        };

        let url = site["ssl_url"]
            .as_str()
            .or_else(|| site["url"].as_str())
            .map(String::from);

        let state = site["state"].as_str().unwrap_or("unknown");
        let status = match state {
            "current" => ServiceStatus::Running,
            "uploading" | "processing" => ServiceStatus::Deploying,
            "error" => ServiceStatus::Failed,
            _ => ServiceStatus::Unknown,
        };

        CloudService {
            name,
            provider: "netlify".into(),
            service_type,
            region: "global".to_string(),
            status,
            instances: 1,
            url,
            image: None,
            resources: Some(Resources {
                cpu: None,
                memory: None,
                disk: None,
            }),
            created_at: site["created_at"].as_str().map(String::from),
            provider_id: Some(id),
        }
    }
}

#[async_trait]
impl CloudAdapter for NetlifyAdapter {
    fn provider_name(&self) -> &str {
        "netlify"
    }

    async fn list_services(&self) -> Result<Vec<CloudService>, CloudError> {
        let url = format!("{NETLIFY_API}/sites");
        let data = self.api_get(&url).await?;

        let services = data
            .as_array()
            .map(|arr| arr.iter().map(|s| self.parse_site(s)).collect())
            .unwrap_or_default();

        Ok(services)
    }

    async fn get_service(&self, name: &str) -> Result<CloudService, CloudError> {
        // Netlify supports lookup by site name via subdomain
        let url = format!("{NETLIFY_API}/sites/{name}.netlify.app");
        let data = self.api_get(&url).await.map_err(|_| {
            CloudError::NotFound(format!("Site '{name}' not found on Netlify"))
        })?;
        Ok(self.parse_site(&data))
    }

    async fn provision(&self, spec: &ProvisionSpec) -> Result<CloudService, CloudError> {
        let url = format!("{NETLIFY_API}/sites");
        let body = serde_json::json!({
            "name": spec.name,
            "custom_domain": serde_json::Value::Null,
        });

        let data = self
            .api_request(reqwest::Method::POST, &url, Some(body))
            .await?;

        let site_id = data["id"].as_str().unwrap_or("").to_string();

        // Set environment variables if provided
        if !spec.env.is_empty() {
            let env_url = format!("{NETLIFY_API}/accounts/me/env");
            for (key, value) in &spec.env {
                let env_body = serde_json::json!({
                    "key": key,
                    "values": [{
                        "value": value,
                        "context": "all",
                    }],
                    "scopes": ["builds", "functions"],
                });
                let _ = self
                    .api_request(reqwest::Method::POST, &env_url, Some(env_body))
                    .await;
            }
        }

        info!(provider = "netlify", service = %spec.name, "Site provisioned");

        Ok(CloudService {
            name: spec.name.clone(),
            provider: "netlify".into(),
            service_type: spec.service_type.clone(),
            region: "global".to_string(),
            status: ServiceStatus::Deploying,
            instances: 1,
            url: Some(format!("https://{}.netlify.app", spec.name)),
            image: None,
            resources: Some(Resources {
                cpu: None,
                memory: None,
                disk: None,
            }),
            created_at: Some(Utc::now().to_rfc3339()),
            provider_id: Some(site_id),
        })
    }

    async fn destroy(&self, name: &str) -> Result<(), CloudError> {
        let service = self.get_service(name).await?;
        let site_id = service.provider_id.unwrap_or_default();
        let url = format!("{NETLIFY_API}/sites/{site_id}");
        self.api_request(reqwest::Method::DELETE, &url, None)
            .await?;
        info!(provider = "netlify", service = %name, "Site destroyed");
        Ok(())
    }

    async fn deploy(
        &self,
        name: &str,
        _config: &DeployConfig,
    ) -> Result<CloudService, CloudError> {
        let service = self.get_service(name).await?;
        let site_id = service.provider_id.as_deref().unwrap_or_default();

        // Create a new deploy (file digest deploy)
        let url = format!("{NETLIFY_API}/sites/{site_id}/deploys");
        let body = serde_json::json!({
            "production": true,
        });

        let _ = self
            .api_request(reqwest::Method::POST, &url, Some(body))
            .await?;

        info!(provider = "netlify", service = %name, "Deploy triggered");
        self.get_service(name).await
    }

    async fn scale(&self, _name: &str, _instances: u32) -> Result<CloudService, CloudError> {
        Err(CloudError::NotSupported(
            "Netlify auto-scales; manual scaling is not supported".into(),
        ))
    }

    async fn restart(&self, _name: &str) -> Result<(), CloudError> {
        Err(CloudError::NotSupported(
            "Netlify sites are static or serverless; restart is not applicable".into(),
        ))
    }

    async fn logs(&self, name: &str, _lines: u32) -> Result<Vec<String>, CloudError> {
        Err(CloudError::NotSupported(format!(
            "Netlify function logs for '{name}' are only available via the dashboard or Log Drains"
        )))
    }

    async fn status(&self, name: &str) -> Result<ServiceStatus, CloudError> {
        let service = self.get_service(name).await?;
        Ok(service.status)
    }

    async fn metrics(&self, _name: &str) -> Result<ServiceMetrics, CloudError> {
        Err(CloudError::NotSupported(
            "Netlify analytics require the Analytics add-on".into(),
        ))
    }

    async fn estimate_cost(&self, spec: &ProvisionSpec) -> Result<CostEstimate, CloudError> {
        // Netlify: Free tier (100GB bandwidth, 300 build min/mo)
        // Pro: $19/mo per member
        let monthly = 19.0 * spec.instances as f64;
        Ok(CostEstimate {
            hourly: monthly / 730.0,
            monthly,
            currency: "USD".into(),
            breakdown: vec![
                CostLine {
                    item: "Netlify Pro plan per member".into(),
                    amount: 19.0,
                },
                CostLine {
                    item: format!("{}x member(s)", spec.instances),
                    amount: monthly,
                },
            ],
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_netlify_adapter_creation() {
        let adapter = NetlifyAdapter::new("test-token");
        assert_eq!(adapter.provider_name(), "netlify");
    }

    #[test]
    fn test_estimate_cost() {
        let adapter = NetlifyAdapter::new("test");
        let spec = ProvisionSpec {
            name: "my-site".into(),
            instances: 2,
            service_type: ServiceType::StaticSite,
            ..Default::default()
        };
        let rt = tokio::runtime::Runtime::new().unwrap();
        let cost = rt.block_on(adapter.estimate_cost(&spec)).unwrap();
        assert_eq!(cost.monthly, 38.0); // $19 * 2 members
        assert_eq!(cost.currency, "USD");
    }
}
