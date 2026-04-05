use async_trait::async_trait;
use chrono::Utc;
use reqwest::Client;
use tracing::info;

use super::{
    CloudAdapter, CloudError, CloudService, CostEstimate, CostLine, DeployConfig, ProvisionSpec,
    Resources, ServiceMetrics, ServiceStatus, ServiceType,
};

const SUPABASE_API: &str = "https://api.supabase.com/v1";

pub struct SupabaseAdapter {
    access_token: String,
    project_ref: Option<String>,
    client: Client,
}

impl SupabaseAdapter {
    pub fn new(access_token: &str) -> Self {
        Self {
            access_token: access_token.to_string(),
            project_ref: None,
            client: Client::new(),
        }
    }

    pub fn with_project(mut self, project_ref: &str) -> Self {
        self.project_ref = Some(project_ref.to_string());
        self
    }

    fn require_project_ref(&self) -> Result<&str, CloudError> {
        self.project_ref.as_deref().ok_or_else(|| {
            CloudError::InvalidConfig(
                "Supabase project_ref is required; use with_project()".into(),
            )
        })
    }

    fn functions_url(&self, project_ref: &str) -> String {
        format!("{SUPABASE_API}/projects/{project_ref}/functions")
    }

    async fn api_get(&self, url: &str) -> Result<serde_json::Value, CloudError> {
        let resp = self
            .client
            .get(url)
            .header("Authorization", format!("Bearer {}", self.access_token))
            .send()
            .await
            .map_err(|e| CloudError::NetworkError(e.to_string()))?;

        let status = resp.status().as_u16();
        if status == 401 || status == 403 {
            return Err(CloudError::AuthError(
                "Invalid Supabase access token".into(),
            ));
        }
        if status == 404 {
            return Err(CloudError::NotFound(
                "Resource not found on Supabase".into(),
            ));
        }

        let body: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| CloudError::NetworkError(e.to_string()))?;

        if let Some(msg) = body.get("message").and_then(|m| m.as_str()) {
            if status >= 400 {
                return Err(CloudError::ApiError(format!(
                    "supabase ({status}): {msg}"
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
            .header("Authorization", format!("Bearer {}", self.access_token))
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
            return Err(CloudError::AuthError(
                "Invalid Supabase access token".into(),
            ));
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
                    "supabase ({status}): {msg}"
                )));
            }
        }

        Ok(data)
    }

    fn parse_function(
        &self,
        func: &serde_json::Value,
        project_ref: &str,
    ) -> CloudService {
        let name = func["name"].as_str().unwrap_or("unknown").to_string();
        let slug = func["slug"].as_str().unwrap_or(&name);
        let id = func["id"].as_str().unwrap_or("").to_string();

        let func_status = func["status"].as_str().unwrap_or("ACTIVE");
        let status = match func_status {
            "ACTIVE" => ServiceStatus::Running,
            "INACTIVE" | "THROTTLED" => ServiceStatus::Stopped,
            _ => ServiceStatus::Unknown,
        };

        let url = Some(format!(
            "https://{project_ref}.supabase.co/functions/v1/{slug}"
        ));

        CloudService {
            name,
            provider: "supabase".into(),
            service_type: ServiceType::EdgeFunction,
            region: func["region"]
                .as_str()
                .unwrap_or("us-east-1")
                .to_string(),
            status,
            instances: 1,
            url,
            image: None,
            resources: Some(Resources {
                cpu: None,
                memory: None,
                disk: None,
            }),
            created_at: func["created_at"].as_str().map(String::from),
            provider_id: Some(id),
        }
    }
}

#[async_trait]
impl CloudAdapter for SupabaseAdapter {
    fn provider_name(&self) -> &str {
        "supabase"
    }

    async fn list_services(&self) -> Result<Vec<CloudService>, CloudError> {
        let project_ref = self.require_project_ref()?;
        let url = self.functions_url(project_ref);
        let data = self.api_get(&url).await?;

        let project_ref_owned = project_ref.to_string();
        let services = data
            .as_array()
            .map(|arr| {
                arr.iter()
                    .map(|f| self.parse_function(f, &project_ref_owned))
                    .collect()
            })
            .unwrap_or_default();

        Ok(services)
    }

    async fn get_service(&self, name: &str) -> Result<CloudService, CloudError> {
        let project_ref = self.require_project_ref()?;
        let url = format!("{}/{name}", self.functions_url(project_ref));
        let project_ref_owned = project_ref.to_string();
        let data = self.api_get(&url).await?;
        Ok(self.parse_function(&data, &project_ref_owned))
    }

    async fn provision(&self, spec: &ProvisionSpec) -> Result<CloudService, CloudError> {
        let project_ref = self.require_project_ref()?;
        let url = self.functions_url(project_ref);

        let body = serde_json::json!({
            "name": spec.name,
            "slug": spec.name,
            "verify_jwt": true,
        });

        let data = self
            .api_request(reqwest::Method::POST, &url, Some(body))
            .await?;

        let func_id = data["id"].as_str().unwrap_or("").to_string();
        info!(provider = "supabase", service = %spec.name, "Edge function provisioned");

        Ok(CloudService {
            name: spec.name.clone(),
            provider: "supabase".into(),
            service_type: ServiceType::EdgeFunction,
            region: spec.region.clone(),
            status: ServiceStatus::Deploying,
            instances: 1,
            url: Some(format!(
                "https://{project_ref}.supabase.co/functions/v1/{}",
                spec.name
            )),
            image: None,
            resources: Some(Resources {
                cpu: None,
                memory: None,
                disk: None,
            }),
            created_at: Some(Utc::now().to_rfc3339()),
            provider_id: Some(func_id),
        })
    }

    async fn destroy(&self, name: &str) -> Result<(), CloudError> {
        let project_ref = self.require_project_ref()?;
        let url = format!("{}/{name}", self.functions_url(project_ref));
        self.api_request(reqwest::Method::DELETE, &url, None)
            .await?;
        info!(provider = "supabase", service = %name, "Edge function destroyed");
        Ok(())
    }

    async fn deploy(
        &self,
        name: &str,
        _config: &DeployConfig,
    ) -> Result<CloudService, CloudError> {
        let project_ref = self.require_project_ref()?;
        let url = format!("{}/{name}", self.functions_url(project_ref));

        let body = serde_json::json!({
            "name": name,
            "verify_jwt": true,
        });

        let _ = self
            .api_request(reqwest::Method::PATCH, &url, Some(body))
            .await?;

        info!(provider = "supabase", service = %name, "Edge function deployed");
        self.get_service(name).await
    }

    async fn scale(&self, _name: &str, _instances: u32) -> Result<CloudService, CloudError> {
        Err(CloudError::NotSupported(
            "Supabase Edge Functions auto-scale; manual scaling is not supported".into(),
        ))
    }

    async fn restart(&self, _name: &str) -> Result<(), CloudError> {
        Err(CloudError::NotSupported(
            "Supabase Edge Functions are stateless; restart is not applicable".into(),
        ))
    }

    async fn logs(&self, _name: &str, _lines: u32) -> Result<Vec<String>, CloudError> {
        Err(CloudError::NotSupported(
            "Supabase Edge Function logs are available via the dashboard or Logflare".into(),
        ))
    }

    async fn status(&self, name: &str) -> Result<ServiceStatus, CloudError> {
        let service = self.get_service(name).await?;
        Ok(service.status)
    }

    async fn metrics(&self, _name: &str) -> Result<ServiceMetrics, CloudError> {
        Err(CloudError::NotSupported(
            "Supabase Edge Function metrics are not available via API".into(),
        ))
    }

    async fn estimate_cost(&self, spec: &ProvisionSpec) -> Result<CostEstimate, CloudError> {
        // Supabase: Free tier 500K invocations/mo
        // Pro plan: $25/mo (includes 2M invocations)
        let monthly = 25.0 * spec.instances as f64;
        Ok(CostEstimate {
            hourly: monthly / 730.0,
            monthly,
            currency: "USD".into(),
            breakdown: vec![
                CostLine {
                    item: "Supabase Pro plan (2M invocations included)".into(),
                    amount: 25.0,
                },
                CostLine {
                    item: "Free tier: 500K invocations/mo".into(),
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
    fn test_supabase_adapter_creation() {
        let adapter = SupabaseAdapter::new("test-token");
        assert_eq!(adapter.provider_name(), "supabase");
    }

    #[test]
    fn test_supabase_with_project() {
        let adapter = SupabaseAdapter::new("test-token").with_project("proj-abc");
        assert_eq!(adapter.project_ref.as_deref(), Some("proj-abc"));
    }

    #[test]
    fn test_estimate_cost() {
        let adapter = SupabaseAdapter::new("test");
        let spec = ProvisionSpec {
            name: "my-func".into(),
            instances: 2,
            service_type: ServiceType::EdgeFunction,
            ..Default::default()
        };
        let rt = tokio::runtime::Runtime::new().unwrap();
        let cost = rt.block_on(adapter.estimate_cost(&spec)).unwrap();
        assert_eq!(cost.monthly, 50.0); // $25 * 2 instances
        assert_eq!(cost.currency, "USD");
        assert_eq!(cost.breakdown.len(), 2);
    }
}
