use crate::provider::{CloudInstance, CloudProvider, ImportFilters};
use async_trait::async_trait;
use tokio::process::Command;

pub struct AwsProvider {
    pub region: String,
    pub access_key: Option<String>,
    pub secret_key: Option<String>,
}

impl AwsProvider {
    pub fn new(region: &str) -> Self {
        Self {
            region: region.into(),
            access_key: std::env::var("AWS_ACCESS_KEY_ID").ok(),
            secret_key: std::env::var("AWS_SECRET_ACCESS_KEY").ok(),
        }
    }

    async fn list_via_cli(
        &self,
        filters: &ImportFilters,
    ) -> Result<Vec<CloudInstance>, String> {
        let mut args = vec![
            "ec2".to_string(),
            "describe-instances".to_string(),
            "--region".to_string(),
            self.region.clone(),
            "--output".to_string(),
            "json".to_string(),
        ];

        if !filters.tags.is_empty() {
            let filter_strs: Vec<String> = filters
                .tags
                .iter()
                .map(|(k, v)| format!("Name=tag:{k},Values={v}"))
                .collect();
            args.push("--filters".to_string());
            for f in filter_strs {
                args.push(f);
            }
        }

        let output = Command::new("aws")
            .args(&args)
            .output()
            .await
            .map_err(|e| format!("AWS CLI spawn failed: {e}"))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(format!("AWS CLI error: {stderr}"));
        }

        let json: serde_json::Value = serde_json::from_slice(&output.stdout)
            .map_err(|e| format!("Failed to parse AWS CLI output: {e}"))?;

        parse_aws_instances(&json, &self.region)
    }
}

fn parse_aws_instances(
    json: &serde_json::Value,
    region: &str,
) -> Result<Vec<CloudInstance>, String> {
    let reservations = json["Reservations"]
        .as_array()
        .ok_or("Missing Reservations in AWS response")?;

    let mut instances = Vec::new();

    for reservation in reservations {
        let raw_instances = match reservation["Instances"].as_array() {
            Some(list) => list,
            None => continue,
        };

        for inst in raw_instances {
            let state = inst["State"]["Name"].as_str().unwrap_or("");
            if state != "running" {
                continue;
            }

            let instance_id = inst["InstanceId"].as_str().unwrap_or("").to_string();
            let ip = inst["PublicIpAddress"]
                .as_str()
                .unwrap_or("")
                .to_string();

            if ip.is_empty() {
                continue;
            }

            // Extract Name tag
            let name = inst["Tags"]
                .as_array()
                .and_then(|tags| {
                    tags.iter().find(|t| t["Key"].as_str() == Some("Name"))
                })
                .and_then(|t| t["Value"].as_str())
                .unwrap_or(&instance_id)
                .to_string();

            let tags: Vec<String> = inst["Tags"]
                .as_array()
                .map(|ts| {
                    ts.iter()
                        .filter_map(|t| {
                            let k = t["Key"].as_str()?;
                            let v = t["Value"].as_str()?;
                            Some(format!("{k}={v}"))
                        })
                        .collect()
                })
                .unwrap_or_default();

            instances.push(CloudInstance {
                name,
                host: ip,
                user: "ec2-user".to_string(),
                region: Some(region.to_string()),
                instance_id: Some(instance_id),
                provider: "aws".to_string(),
                tags,
            });
        }
    }

    Ok(instances)
}

#[async_trait]
impl CloudProvider for AwsProvider {
    async fn list_instances(&self, filters: &ImportFilters) -> Result<Vec<CloudInstance>, String> {
        self.list_via_cli(filters).await
    }

    fn provider_name(&self) -> &str {
        "aws"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_aws_provider_new() {
        let provider = AwsProvider::new("eu-west-1");
        assert_eq!(provider.region, "eu-west-1");
        assert_eq!(provider.provider_name(), "aws");
    }

    #[test]
    fn test_parse_aws_instances_running() {
        let json = serde_json::json!({
            "Reservations": [{
                "Instances": [{
                    "InstanceId": "i-0abc1234",
                    "State": { "Name": "running" },
                    "PublicIpAddress": "54.0.0.1",
                    "Tags": [
                        { "Key": "Name", "Value": "web-server" },
                        { "Key": "env", "Value": "prod" }
                    ]
                }]
            }]
        });

        let instances = parse_aws_instances(&json, "us-east-1").unwrap();
        assert_eq!(instances.len(), 1);
        assert_eq!(instances[0].name, "web-server");
        assert_eq!(instances[0].host, "54.0.0.1");
        assert_eq!(instances[0].instance_id, Some("i-0abc1234".to_string()));
        assert_eq!(instances[0].region, Some("us-east-1".to_string()));
        assert_eq!(instances[0].provider, "aws");
    }

    #[test]
    fn test_parse_aws_instances_skips_stopped() {
        let json = serde_json::json!({
            "Reservations": [{
                "Instances": [{
                    "InstanceId": "i-stopped",
                    "State": { "Name": "stopped" },
                    "PublicIpAddress": "1.2.3.4",
                    "Tags": []
                }]
            }]
        });

        let instances = parse_aws_instances(&json, "us-east-1").unwrap();
        assert!(instances.is_empty());
    }

    #[test]
    fn test_parse_aws_instances_skips_no_ip() {
        let json = serde_json::json!({
            "Reservations": [{
                "Instances": [{
                    "InstanceId": "i-noip",
                    "State": { "Name": "running" },
                    "Tags": []
                }]
            }]
        });

        let instances = parse_aws_instances(&json, "us-east-1").unwrap();
        assert!(instances.is_empty());
    }

    #[test]
    fn test_parse_aws_instances_empty_reservations() {
        let json = serde_json::json!({ "Reservations": [] });
        let instances = parse_aws_instances(&json, "us-east-1").unwrap();
        assert!(instances.is_empty());
    }

    #[test]
    fn test_parse_aws_instances_uses_instance_id_as_name_fallback() {
        let json = serde_json::json!({
            "Reservations": [{
                "Instances": [{
                    "InstanceId": "i-fallback",
                    "State": { "Name": "running" },
                    "PublicIpAddress": "5.5.5.5",
                    "Tags": []
                }]
            }]
        });

        let instances = parse_aws_instances(&json, "us-east-1").unwrap();
        assert_eq!(instances.len(), 1);
        assert_eq!(instances[0].name, "i-fallback");
    }
}
