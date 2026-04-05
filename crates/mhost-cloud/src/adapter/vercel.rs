use async_trait::async_trait;
use chrono::Utc;
use reqwest::Client;
use tracing::info;

use super::{
    CloudAdapter, CloudError, CloudService, CostEstimate, CostLine, DeployConfig, ProvisionSpec,
    Resources, ServiceMetrics, ServiceStatus, ServiceType,
};

const VERCEL_API: &str = "https://api.vercel.com";

pub struct VercelAdapter {
    token: String,
    team_id: Option<String>,
    client: Client,
}

impl VercelAdapter {
    pub fn new(token: &str) -> Self {
        Self {
            token: token.to_string(),
            team_id: None,
            client: Client::new(),
        }
    }

    pub fn with_team(mut self, team_id: &str) -> Self {
        self.team_id = Some(team_id.to_string());
        self
    }

    fn build_url(&self, path: &str) -> String {
        let base = format!("{VERCEL_API}{path}");
        match &self.team_id {
            Some(id) => {
                let sep = if base.contains('?') { '&' } else { '?' };
                format!("{base}{sep}teamId={id}")
            }
            None => base,
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
        if status == 401 || status == 403 {
            return Err(CloudError::AuthError("Invalid Vercel token".into()));
        }
        if status == 404 {
            return Err(CloudError::NotFound("Resource not found on Vercel".into()));
        }

        let body: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| CloudError::NetworkError(e.to_string()))?;

        if let Some(err) = body.get("error") {
            let msg = err["message"].as_str().unwrap_or("Unknown error");
            return Err(CloudError::ApiError(format!("vercel ({status}): {msg}")));
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
        if status == 401 || status == 403 {
            return Err(CloudError::AuthError("Invalid Vercel token".into()));
        }

        let data: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| CloudError::NetworkError(e.to_string()))?;

        if let Some(err) = data.get("error") {
            let msg = err["message"].as_str().unwrap_or("Unknown error");
            return Err(CloudError::ApiError(format!("vercel ({status}): {msg}")));
        }

        Ok(data)
    }

    fn parse_project(&self, project: &serde_json::Value) -> CloudService {
        let name = project["name"].as_str().unwrap_or("unknown").to_string();
        let id = project["id"].as_str().unwrap_or("").to_string();
        let framework = project["framework"].as_str().unwrap_or("");

        let service_type = match framework {
            "nextjs" | "nuxtjs" | "remix" | "sveltekit" => ServiceType::Serverless,
            _ => ServiceType::StaticSite,
        };

        let url = project["targets"]["production"]["url"]
            .as_str()
            .or_else(|| project["alias"].as_array()?.first()?.as_str())
            .map(|u| {
                if u.starts_with("http") {
                    u.to_string()
                } else {
                    format!("https://{u}")
                }
            });

        CloudService {
            name,
            provider: "vercel".into(),
            service_type,
            region: "global".to_string(),
            status: ServiceStatus::Running,
            instances: 1,
            url,
            image: None,
            resources: Some(Resources {
                cpu: None,
                memory: None,
                disk: None,
            }),
            created_at: project["createdAt"]
                .as_u64()
                .map(|ts| {
                    chrono::DateTime::from_timestamp_millis(ts as i64)
                        .map(|dt| dt.to_rfc3339())
                        .unwrap_or_default()
                }),
            provider_id: Some(id),
        }
    }
}

#[async_trait]
impl CloudAdapter for VercelAdapter {
    fn provider_name(&self) -> &str {
        "vercel"
    }

    async fn list_services(&self) -> Result<Vec<CloudService>, CloudError> {
        let url = self.build_url("/v9/projects");
        let data = self.api_get(&url).await?;

        let services = data["projects"]
            .as_array()
            .map(|arr| arr.iter().map(|p| self.parse_project(p)).collect())
            .unwrap_or_default();

        Ok(services)
    }

    async fn get_service(&self, name: &str) -> Result<CloudService, CloudError> {
        let url = self.build_url(&format!("/v9/projects/{name}"));
        let data = self.api_get(&url).await?;
        Ok(self.parse_project(&data))
    }

    async fn provision(&self, spec: &ProvisionSpec) -> Result<CloudService, CloudError> {
        let url = self.build_url("/v9/projects");
        let body = serde_json::json!({
            "name": spec.name,
            "framework": match spec.service_type {
                ServiceType::Serverless => "nextjs",
                _ => "static",
            },
            "environmentVariables": spec.env.iter().map(|(k, v)| {
                serde_json::json!({
                    "key": k,
                    "value": v,
                    "target": ["production", "preview", "development"],
                    "type": "plain",
                })
            }).collect::<Vec<_>>(),
        });

        let data = self
            .api_request(reqwest::Method::POST, &url, Some(body))
            .await?;

        info!(provider = "vercel", service = %spec.name, "Project provisioned");

        Ok(CloudService {
            name: spec.name.clone(),
            provider: "vercel".into(),
            service_type: spec.service_type.clone(),
            region: "global".to_string(),
            status: ServiceStatus::Deploying,
            instances: 1,
            url: None,
            image: None,
            resources: Some(Resources {
                cpu: None,
                memory: None,
                disk: None,
            }),
            created_at: Some(Utc::now().to_rfc3339()),
            provider_id: data["id"].as_str().map(String::from),
        })
    }

    async fn destroy(&self, name: &str) -> Result<(), CloudError> {
        let url = self.build_url(&format!("/v9/projects/{name}"));
        self.api_request(reqwest::Method::DELETE, &url, None)
            .await?;
        info!(provider = "vercel", service = %name, "Project destroyed");
        Ok(())
    }

    async fn deploy(
        &self,
        name: &str,
        config: &DeployConfig,
    ) -> Result<CloudService, CloudError> {
        let url = self.build_url("/v13/deployments");
        let body = serde_json::json!({
            "name": name,
            "target": "production",
            "gitSource": {
                "type": "github",
                "ref": "main",
            },
        });

        let _ = self
            .api_request(reqwest::Method::POST, &url, Some(body))
            .await?;

        // Set env vars if provided
        if !config.env.is_empty() {
            let project_url = self.build_url(&format!("/v9/projects/{name}"));
            let env_body = serde_json::json!({
                "environmentVariables": config.env.iter().map(|(k, v)| {
                    serde_json::json!({
                        "key": k,
                        "value": v,
                        "target": ["production"],
                        "type": "plain",
                    })
                }).collect::<Vec<_>>(),
            });
            let _ = self
                .api_request(reqwest::Method::PATCH, &project_url, Some(env_body))
                .await;
        }

        info!(provider = "vercel", service = %name, "Deployment triggered");
        self.get_service(name).await
    }

    async fn scale(&self, _name: &str, _instances: u32) -> Result<CloudService, CloudError> {
        Err(CloudError::NotSupported(
            "Vercel serverless functions auto-scale; manual scaling is not supported".into(),
        ))
    }

    async fn restart(&self, _name: &str) -> Result<(), CloudError> {
        Err(CloudError::NotSupported(
            "Vercel deployments are immutable; redeploy instead".into(),
        ))
    }

    async fn logs(&self, name: &str, _lines: u32) -> Result<Vec<String>, CloudError> {
        // Vercel has a log drains API but real-time logs require SSE
        Err(CloudError::NotSupported(
            format!("Vercel logs for '{name}' require Log Drains or the dashboard"),
        ))
    }

    async fn status(&self, name: &str) -> Result<ServiceStatus, CloudError> {
        let service = self.get_service(name).await?;
        Ok(service.status)
    }

    async fn metrics(&self, _name: &str) -> Result<ServiceMetrics, CloudError> {
        Err(CloudError::NotSupported(
            "Vercel metrics require the Web Analytics or Speed Insights API".into(),
        ))
    }

    async fn estimate_cost(&self, spec: &ProvisionSpec) -> Result<CostEstimate, CloudError> {
        // Vercel: Hobby free, Pro $20/mo per member, usage-based beyond limits
        let monthly = 20.0 * spec.instances as f64;
        Ok(CostEstimate {
            hourly: monthly / 730.0,
            monthly,
            currency: "USD".into(),
            breakdown: vec![
                CostLine {
                    item: "Vercel Pro plan per team member".into(),
                    amount: 20.0,
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
    fn test_vercel_adapter_creation() {
        let adapter = VercelAdapter::new("test-token");
        assert_eq!(adapter.provider_name(), "vercel");
    }

    #[test]
    fn test_vercel_with_team() {
        let adapter = VercelAdapter::new("test-token").with_team("team-123");
        assert_eq!(adapter.team_id.as_deref(), Some("team-123"));
    }

    #[test]
    fn test_estimate_cost() {
        let adapter = VercelAdapter::new("test");
        let spec = ProvisionSpec {
            name: "my-site".into(),
            instances: 3,
            service_type: ServiceType::StaticSite,
            ..Default::default()
        };
        let rt = tokio::runtime::Runtime::new().unwrap();
        let cost = rt.block_on(adapter.estimate_cost(&spec)).unwrap();
        assert_eq!(cost.monthly, 60.0); // $20 * 3 members
        assert_eq!(cost.currency, "USD");
    }
}
