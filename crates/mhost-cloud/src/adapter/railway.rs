use async_trait::async_trait;
use chrono::Utc;
use reqwest::Client;
use tracing::info;

use super::{
    CloudAdapter, CloudError, CloudService, CostEstimate, CostLine, DeployConfig,
    ProvisionSpec, Resources, ServiceMetrics, ServiceStatus, ServiceType,
};

const RAILWAY_API: &str = "https://backboard.railway.app/graphql/v2";

pub struct RailwayAdapter {
    token: String,
    client: Client,
    project_id: Option<String>,
}

impl RailwayAdapter {
    pub fn new(token: &str) -> Self {
        Self {
            token: token.to_string(),
            client: Client::new(),
            project_id: None,
        }
    }

    pub fn with_project(mut self, project_id: &str) -> Self {
        self.project_id = Some(project_id.to_string());
        self
    }

    async fn graphql(
        &self,
        query: &str,
        variables: serde_json::Value,
    ) -> Result<serde_json::Value, CloudError> {
        let body = serde_json::json!({
            "query": query,
            "variables": variables,
        });

        let resp = self
            .client
            .post(RAILWAY_API)
            .header("Authorization", format!("Bearer {}", self.token))
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|e| CloudError::NetworkError(e.to_string()))?;

        let status = resp.status().as_u16();
        if status == 401 {
            return Err(CloudError::AuthError("Invalid Railway token".into()));
        }

        let data: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| CloudError::NetworkError(e.to_string()))?;

        if let Some(errors) = data.get("errors") {
            let msg = errors[0]["message"]
                .as_str()
                .unwrap_or("Unknown error");
            return Err(CloudError::ApiError(format!(
                "railway ({status}): {msg}"
            )));
        }

        Ok(data["data"].clone())
    }

    fn parse_service(&self, svc: &serde_json::Value, project_id: &str) -> CloudService {
        let name = svc["name"].as_str().unwrap_or("unknown").to_string();
        let id = svc["id"].as_str().unwrap_or("").to_string();

        let status = match svc["status"].as_str() {
            Some("ACTIVE") | Some("SUCCESS") => ServiceStatus::Running,
            Some("BUILDING") | Some("DEPLOYING") => ServiceStatus::Deploying,
            Some("FAILED") | Some("CRASHED") => ServiceStatus::Failed,
            Some("REMOVED") => ServiceStatus::Stopped,
            _ => ServiceStatus::Unknown,
        };

        let url = svc["serviceInstances"]["edges"]
            .as_array()
            .and_then(|edges| edges.first())
            .and_then(|edge| {
                edge["node"]["domains"]["serviceDomains"].as_array()
            })
            .and_then(|domains| domains.first())
            .and_then(|d| d["domain"].as_str())
            .map(|d| format!("https://{d}"));

        CloudService {
            name,
            provider: "railway".into(),
            service_type: ServiceType::Container,
            region: svc["region"]
                .as_str()
                .unwrap_or("us-east-1")
                .to_string(),
            status,
            instances: 1,
            url,
            image: svc["source"]["image"].as_str().map(String::from),
            resources: Some(Resources {
                cpu: None,
                memory: None,
                disk: None,
            }),
            created_at: Some(Utc::now().to_rfc3339()),
            provider_id: Some(format!("{project_id}/{id}")),
        }
    }
}

#[async_trait]
impl CloudAdapter for RailwayAdapter {
    fn provider_name(&self) -> &str {
        "railway"
    }

    async fn list_services(&self) -> Result<Vec<CloudService>, CloudError> {
        let query = r#"
            query { me { projects { edges { node {
                id name services { edges { node {
                    id name status region
                    source { image }
                    serviceInstances { edges { node {
                        domains { serviceDomains { domain } }
                    }}}
                }}}
            }}}}}
        "#;

        let data = self.graphql(query, serde_json::json!({})).await?;
        let mut services = Vec::new();

        if let Some(projects) = data["me"]["projects"]["edges"].as_array() {
            for project in projects {
                let project_id =
                    project["node"]["id"].as_str().unwrap_or("");
                if let Some(filter_id) = &self.project_id {
                    if project_id != filter_id {
                        continue;
                    }
                }
                if let Some(svcs) =
                    project["node"]["services"]["edges"].as_array()
                {
                    for svc in svcs {
                        services.push(
                            self.parse_service(&svc["node"], project_id),
                        );
                    }
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
                CloudError::NotFound(format!(
                    "Service '{name}' not found on Railway"
                ))
            })
    }

    async fn provision(
        &self,
        spec: &ProvisionSpec,
    ) -> Result<CloudService, CloudError> {
        // Step 1: Create project if needed
        let project_id = if let Some(id) = &self.project_id {
            id.clone()
        } else {
            let query = r#"mutation($input: ProjectCreateInput!) {
                projectCreate(input: $input) { id }
            }"#;
            let data = self
                .graphql(
                    query,
                    serde_json::json!({
                        "input": { "name": spec.name }
                    }),
                )
                .await?;
            data["projectCreate"]["id"]
                .as_str()
                .ok_or_else(|| {
                    CloudError::ApiError(
                        "Failed to create project".into(),
                    )
                })?
                .to_string()
        };

        // Step 2: Create service
        let query = r#"mutation($input: ServiceCreateInput!) {
            serviceCreate(input: $input) { id name }
        }"#;
        let mut input = serde_json::json!({
            "projectId": project_id,
            "name": spec.name,
        });
        if let Some(ref image) = spec.image {
            input["source"] = serde_json::json!({ "image": image });
        }
        let data = self
            .graphql(query, serde_json::json!({ "input": input }))
            .await?;

        let service_id = data["serviceCreate"]["id"]
            .as_str()
            .unwrap_or("")
            .to_string();
        info!(provider = "railway", service = %spec.name, "Service provisioned");

        // Step 3: Set env vars
        if !spec.env.is_empty() {
            let env_query = r#"mutation($input: VariableCollectionUpsertInput!) {
                variableCollectionUpsert(input: $input)
            }"#;
            let _ = self
                .graphql(
                    env_query,
                    serde_json::json!({
                        "input": {
                            "projectId": project_id,
                            "serviceId": service_id,
                            "environmentId": serde_json::Value::Null,
                            "variables": spec.env,
                        }
                    }),
                )
                .await;
        }

        let cpu = spec
            .resources
            .as_ref()
            .and_then(|r| r.cpu.clone());
        let memory = spec
            .resources
            .as_ref()
            .and_then(|r| r.memory.clone());

        Ok(CloudService {
            name: spec.name.clone(),
            provider: "railway".into(),
            service_type: ServiceType::Container,
            region: spec.region.clone(),
            status: ServiceStatus::Deploying,
            instances: spec.instances,
            url: None,
            image: spec.image.clone(),
            resources: Some(Resources {
                cpu,
                memory,
                disk: None,
            }),
            created_at: Some(Utc::now().to_rfc3339()),
            provider_id: Some(format!("{project_id}/{service_id}")),
        })
    }

    async fn destroy(&self, name: &str) -> Result<(), CloudError> {
        let service = self.get_service(name).await?;
        let provider_id = service.provider_id.unwrap_or_default();
        let parts: Vec<&str> = provider_id.split('/').collect();
        if parts.len() != 2 {
            return Err(CloudError::InvalidConfig(
                "Invalid provider_id format".into(),
            ));
        }
        let query =
            r#"mutation($id: String!) { serviceDelete(id: $id) }"#;
        self.graphql(query, serde_json::json!({ "id": parts[1] }))
            .await?;
        info!(provider = "railway", service = %name, "Service destroyed");
        Ok(())
    }

    async fn deploy(
        &self,
        name: &str,
        config: &DeployConfig,
    ) -> Result<CloudService, CloudError> {
        let service = self.get_service(name).await?;
        let provider_id = service.provider_id.unwrap_or_default();
        let parts: Vec<&str> = provider_id.split('/').collect();
        if parts.len() != 2 {
            return Err(CloudError::InvalidConfig(
                "Invalid provider_id".into(),
            ));
        }

        let query = r#"mutation($input: ServiceUpdateInput!) {
            serviceUpdate(input: $input) { id }
        }"#;
        self.graphql(
            query,
            serde_json::json!({
                "input": {
                    "id": parts[1],
                    "source": { "image": config.image },
                }
            }),
        )
        .await?;

        if !config.env.is_empty() {
            let env_query = r#"mutation($input: VariableCollectionUpsertInput!) {
                variableCollectionUpsert(input: $input)
            }"#;
            let _ = self
                .graphql(
                    env_query,
                    serde_json::json!({
                        "input": {
                            "projectId": parts[0],
                            "serviceId": parts[1],
                            "environmentId": serde_json::Value::Null,
                            "variables": config.env,
                        }
                    }),
                )
                .await;
        }

        info!(provider = "railway", service = %name, "Deploy triggered");
        self.get_service(name).await
    }

    async fn scale(
        &self,
        name: &str,
        instances: u32,
    ) -> Result<CloudService, CloudError> {
        let service = self.get_service(name).await?;
        let provider_id = service
            .provider_id
            .as_deref()
            .unwrap_or_default();
        let parts: Vec<&str> = provider_id.split('/').collect();
        if parts.len() != 2 {
            return Err(CloudError::InvalidConfig(
                "Invalid provider_id".into(),
            ));
        }
        let query = r#"mutation($input: ServiceUpdateInput!) {
            serviceUpdate(input: $input) { id }
        }"#;
        self.graphql(
            query,
            serde_json::json!({
                "input": {
                    "id": parts[1],
                    "numReplicas": instances,
                }
            }),
        )
        .await?;
        info!(provider = "railway", service = %name, instances, "Scaled");

        let mut updated = self.get_service(name).await?;
        updated.instances = instances;
        Ok(updated)
    }

    async fn restart(&self, name: &str) -> Result<(), CloudError> {
        let service = self.get_service(name).await?;
        let provider_id = service.provider_id.unwrap_or_default();
        let parts: Vec<&str> = provider_id.split('/').collect();
        if parts.len() != 2 {
            return Err(CloudError::InvalidConfig(
                "Invalid provider_id".into(),
            ));
        }
        let query =
            r#"mutation($id: String!) { serviceRestart(id: $id) }"#;
        self.graphql(query, serde_json::json!({ "id": parts[1] }))
            .await?;
        info!(provider = "railway", service = %name, "Restarted");
        Ok(())
    }

    async fn logs(
        &self,
        name: &str,
        lines: u32,
    ) -> Result<Vec<String>, CloudError> {
        let service = self.get_service(name).await?;
        let provider_id = service.provider_id.unwrap_or_default();
        let parts: Vec<&str> = provider_id.split('/').collect();
        if parts.len() != 2 {
            return Err(CloudError::InvalidConfig(
                "Invalid provider_id".into(),
            ));
        }
        let query = r#"query($input: DeploymentLogsInput!) {
            deploymentLogs(input: $input) { message timestamp }
        }"#;
        let data = self
            .graphql(
                query,
                serde_json::json!({
                    "input": {
                        "serviceId": parts[1],
                        "limit": lines,
                    }
                }),
            )
            .await?;

        let logs = data["deploymentLogs"]
            .as_array()
            .map(|arr| {
                arr.iter()
                    .filter_map(|l| l["message"].as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default();
        Ok(logs)
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
            "Railway does not expose metrics via API".into(),
        ))
    }

    async fn estimate_cost(
        &self,
        spec: &ProvisionSpec,
    ) -> Result<CostEstimate, CloudError> {
        // Railway pricing: ~$5/month per 0.5 vCPU + 512MB
        let cpu_str = spec
            .resources
            .as_ref()
            .and_then(|r| r.cpu.as_deref())
            .unwrap_or("0.5");
        let cpu_factor = cpu_str.parse::<f64>().unwrap_or(0.5);
        let monthly = cpu_factor * 10.0 * spec.instances as f64;
        Ok(CostEstimate {
            hourly: monthly / 730.0,
            monthly,
            currency: "USD".into(),
            breakdown: vec![CostLine {
                item: format!(
                    "{}x container ({} vCPU)",
                    spec.instances, cpu_factor
                ),
                amount: monthly,
            }],
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_railway_adapter_creation() {
        let adapter = RailwayAdapter::new("test-token");
        assert_eq!(adapter.provider_name(), "railway");
    }

    #[test]
    fn test_estimate_cost() {
        let adapter = RailwayAdapter::new("test");
        let spec = ProvisionSpec {
            name: "api".into(),
            instances: 2,
            resources: Some(Resources {
                cpu: Some("1".into()),
                memory: None,
                disk: None,
            }),
            ..Default::default()
        };
        let rt = tokio::runtime::Runtime::new().unwrap();
        let cost = rt.block_on(adapter.estimate_cost(&spec)).unwrap();
        assert_eq!(cost.monthly, 20.0); // 1 cpu * 10 * 2 instances
        assert_eq!(cost.currency, "USD");
    }
}
