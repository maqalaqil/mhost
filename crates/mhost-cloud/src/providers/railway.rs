use crate::provider::{CloudInstance, CloudProvider, ImportFilters};
use async_trait::async_trait;

pub struct RailwayProvider {
    pub token: String,
}

impl RailwayProvider {
    pub fn new() -> Result<Self, String> {
        let token = std::env::var("RAILWAY_TOKEN")
            .map_err(|_| "RAILWAY_TOKEN environment variable not set".to_string())?;
        Ok(Self { token })
    }

    async fn list_via_api(
        &self,
        filters: &ImportFilters,
    ) -> Result<Vec<CloudInstance>, String> {
        let client = reqwest::Client::new();

        // Railway GraphQL API to list services
        let query = r#"
            query {
                projects {
                    edges {
                        node {
                            id
                            name
                            services {
                                edges {
                                    node {
                                        id
                                        name
                                        serviceInstances {
                                            edges {
                                                node {
                                                    id
                                                    domains {
                                                        serviceDomains {
                                                            domain
                                                        }
                                                    }
                                                    region
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        "#;

        let resp = client
            .post("https://backboard.railway.app/graphql/v2")
            .header("Authorization", format!("Bearer {}", self.token))
            .header("Content-Type", "application/json")
            .json(&serde_json::json!({ "query": query }))
            .send()
            .await
            .map_err(|e| format!("Railway API error: {e}"))?;

        let body: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| format!("Parse error: {e}"))?;

        parse_railway_services(&body, filters)
    }
}

fn parse_railway_services(
    body: &serde_json::Value,
    filters: &ImportFilters,
) -> Result<Vec<CloudInstance>, String> {
    let projects = body
        .pointer("/data/projects/edges")
        .and_then(|v| v.as_array())
        .ok_or("Invalid Railway API response: missing projects")?;

    let mut instances = Vec::new();

    for project_edge in projects {
        let project_name = project_edge
            .pointer("/node/name")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown");

        let services = project_edge
            .pointer("/node/services/edges")
            .and_then(|v| v.as_array());

        let services = match services {
            Some(s) => s,
            None => continue,
        };

        for service_edge in services {
            let service_name = service_edge
                .pointer("/node/name")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown");
            let service_id = service_edge
                .pointer("/node/id")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();

            let service_instances = service_edge
                .pointer("/node/serviceInstances/edges")
                .and_then(|v| v.as_array());

            let service_instances = match service_instances {
                Some(si) => si,
                None => continue,
            };

            for inst_edge in service_instances {
                let domain = inst_edge
                    .pointer("/node/domains/serviceDomains")
                    .and_then(|v| v.as_array())
                    .and_then(|arr| arr.first())
                    .and_then(|d| d["domain"].as_str());

                let host = match domain {
                    Some(d) => d.to_string(),
                    None => continue,
                };

                let region = inst_edge
                    .pointer("/node/region")
                    .and_then(|v| v.as_str())
                    .map(String::from);

                let instance_id = inst_edge
                    .pointer("/node/id")
                    .and_then(|v| v.as_str())
                    .unwrap_or(&service_id)
                    .to_string();

                // Apply region filter
                if let Some(ref required_region) = filters.region {
                    if region.as_deref() != Some(required_region.as_str()) {
                        continue;
                    }
                }

                let name = format!("{project_name}/{service_name}");

                instances.push(CloudInstance {
                    name,
                    host,
                    user: "root".to_string(),
                    region,
                    instance_id: Some(instance_id),
                    provider: "railway".to_string(),
                    tags: vec![
                        format!("project={project_name}"),
                        format!("service={service_name}"),
                    ],
                });
            }
        }
    }

    Ok(instances)
}

#[async_trait]
impl CloudProvider for RailwayProvider {
    async fn list_instances(&self, filters: &ImportFilters) -> Result<Vec<CloudInstance>, String> {
        self.list_via_api(filters).await
    }

    fn provider_name(&self) -> &str {
        "railway"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider::ImportFilters;

    fn sample_railway_json() -> serde_json::Value {
        serde_json::json!({
            "data": {
                "projects": {
                    "edges": [
                        {
                            "node": {
                                "id": "proj-1",
                                "name": "my-app",
                                "services": {
                                    "edges": [
                                        {
                                            "node": {
                                                "id": "svc-1",
                                                "name": "web",
                                                "serviceInstances": {
                                                    "edges": [
                                                        {
                                                            "node": {
                                                                "id": "inst-1",
                                                                "domains": {
                                                                    "serviceDomains": [
                                                                        { "domain": "web.railway.app" }
                                                                    ]
                                                                },
                                                                "region": "us-west2"
                                                            }
                                                        }
                                                    ]
                                                }
                                            }
                                        }
                                    ]
                                }
                            }
                        }
                    ]
                }
            }
        })
    }

    #[test]
    fn test_parse_railway_services_all() {
        let body = sample_railway_json();
        let filters = ImportFilters::default();
        let instances = parse_railway_services(&body, &filters).unwrap();

        assert_eq!(instances.len(), 1);
        assert_eq!(instances[0].name, "my-app/web");
        assert_eq!(instances[0].host, "web.railway.app");
        assert_eq!(instances[0].provider, "railway");
        assert_eq!(instances[0].region, Some("us-west2".to_string()));
        assert_eq!(instances[0].instance_id, Some("inst-1".to_string()));
    }

    #[test]
    fn test_parse_railway_services_tags_contain_project_and_service() {
        let body = sample_railway_json();
        let filters = ImportFilters::default();
        let instances = parse_railway_services(&body, &filters).unwrap();

        assert!(instances[0].tags.contains(&"project=my-app".to_string()));
        assert!(instances[0].tags.contains(&"service=web".to_string()));
    }

    #[test]
    fn test_parse_railway_services_region_filter() {
        let body = sample_railway_json();
        let filters = ImportFilters::default().with_region("eu-west1");
        let instances = parse_railway_services(&body, &filters).unwrap();
        assert!(instances.is_empty());
    }

    #[test]
    fn test_parse_railway_services_invalid_response() {
        let body = serde_json::json!({ "errors": [{ "message": "Unauthorized" }] });
        let filters = ImportFilters::default();
        let result = parse_railway_services(&body, &filters);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_railway_services_skips_no_domain() {
        let body = serde_json::json!({
            "data": {
                "projects": {
                    "edges": [{
                        "node": {
                            "id": "p1",
                            "name": "app",
                            "services": {
                                "edges": [{
                                    "node": {
                                        "id": "s1",
                                        "name": "worker",
                                        "serviceInstances": {
                                            "edges": [{
                                                "node": {
                                                    "id": "i1",
                                                    "domains": { "serviceDomains": [] },
                                                    "region": "us-west2"
                                                }
                                            }]
                                        }
                                    }
                                }]
                            }
                        }
                    }]
                }
            }
        });
        let filters = ImportFilters::default();
        let instances = parse_railway_services(&body, &filters).unwrap();
        assert!(instances.is_empty());
    }
}
