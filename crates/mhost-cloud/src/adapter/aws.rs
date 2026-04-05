use async_trait::async_trait;
use chrono::Utc;
use tracing::info;

use super::{
    CloudAdapter, CloudError, CloudService, CostEstimate, CostLine, DeployConfig, ProvisionSpec,
    Resources, ServiceMetrics, ServiceStatus, ServiceType,
};

/// AWS adapter supporting ECS Fargate, EC2, and Lambda via the AWS CLI.
///
/// Authentication is handled by shelling out to the `aws` CLI, which supports
/// IAM credentials, SSO, instance profiles, and every other mechanism that the
/// official toolchain provides.  The adapter also sets `AWS_ACCESS_KEY_ID`,
/// `AWS_SECRET_ACCESS_KEY`, and `AWS_DEFAULT_REGION` on every invocation so
/// that explicit key-based credentials work without a prior `aws configure`.
pub struct AwsAdapter {
    access_key_id: String,
    secret_access_key: String,
    region: String,
}

impl AwsAdapter {
    pub fn new(access_key_id: &str, secret_access_key: &str, region: &str) -> Self {
        Self {
            access_key_id: access_key_id.to_string(),
            secret_access_key: secret_access_key.to_string(),
            region: region.to_string(),
        }
    }

    /// Execute an AWS CLI command and return parsed JSON output.
    async fn aws_cli(
        &self,
        service: &str,
        args: &[&str],
    ) -> Result<serde_json::Value, CloudError> {
        let output = tokio::process::Command::new("aws")
            .arg(service)
            .args(args)
            .args(["--output", "json", "--region", &self.region])
            .env("AWS_ACCESS_KEY_ID", &self.access_key_id)
            .env("AWS_SECRET_ACCESS_KEY", &self.secret_access_key)
            .env("AWS_DEFAULT_REGION", &self.region)
            .output()
            .await
            .map_err(|e| CloudError::NetworkError(format!("AWS CLI not available: {e}")))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            if stderr.contains("UnauthorizedAccess")
                || stderr.contains("InvalidClientTokenId")
                || stderr.contains("SignatureDoesNotMatch")
            {
                return Err(CloudError::AuthError(format!("AWS auth failed: {stderr}")));
            }
            return Err(CloudError::ApiError(format!("AWS CLI error: {stderr}")));
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        if stdout.trim().is_empty() {
            return Ok(serde_json::Value::Null);
        }

        serde_json::from_str(&stdout)
            .map_err(|e| CloudError::ApiError(format!("Failed to parse AWS response: {e}")))
    }

    fn parse_ecs_service(&self, svc: &serde_json::Value) -> CloudService {
        let name = svc["serviceName"]
            .as_str()
            .unwrap_or("unknown")
            .to_string();

        let status = match svc["status"].as_str() {
            Some("ACTIVE") => ServiceStatus::Running,
            Some("DRAINING") => ServiceStatus::Deploying,
            Some("INACTIVE") => ServiceStatus::Stopped,
            _ => ServiceStatus::Unknown,
        };

        let running = svc["runningCount"].as_u64().unwrap_or(0) as u32;
        let desired = svc["desiredCount"].as_u64().unwrap_or(0) as u32;

        CloudService {
            name,
            provider: "aws".into(),
            service_type: ServiceType::Container,
            region: self.region.clone(),
            status,
            instances: if running > 0 { running } else { desired },
            url: None,
            image: None,
            resources: Some(Resources {
                cpu: None,
                memory: None,
                disk: None,
            }),
            created_at: svc["createdAt"].as_str().map(String::from),
            provider_id: svc["serviceArn"].as_str().map(String::from),
        }
    }

    fn parse_ec2_instance(&self, inst: &serde_json::Value) -> CloudService {
        let instance_id = inst["InstanceId"]
            .as_str()
            .unwrap_or("unknown")
            .to_string();

        let name = inst["Tags"]
            .as_array()
            .and_then(|tags| {
                tags.iter().find_map(|t| {
                    if t["Key"].as_str() == Some("Name") {
                        t["Value"].as_str().map(String::from)
                    } else {
                        None
                    }
                })
            })
            .unwrap_or_else(|| instance_id.clone());

        let status = match inst["State"]["Name"].as_str() {
            Some("running") => ServiceStatus::Running,
            Some("stopped") => ServiceStatus::Stopped,
            Some("pending") | Some("shutting-down") => ServiceStatus::Deploying,
            Some("terminated") => ServiceStatus::Stopped,
            _ => ServiceStatus::Unknown,
        };

        let instance_type = inst["InstanceType"].as_str().unwrap_or("unknown");
        let public_ip = inst["PublicIpAddress"].as_str().map(String::from);

        CloudService {
            name,
            provider: "aws".into(),
            service_type: ServiceType::VM,
            region: self.region.clone(),
            status,
            instances: 1,
            url: public_ip.as_ref().map(|ip| format!("http://{ip}")),
            image: inst["ImageId"].as_str().map(String::from),
            resources: Some(Resources {
                cpu: Some(instance_type.to_string()),
                memory: None,
                disk: None,
            }),
            created_at: inst["LaunchTime"].as_str().map(String::from),
            provider_id: Some(instance_id),
        }
    }
}

#[async_trait]
impl CloudAdapter for AwsAdapter {
    fn provider_name(&self) -> &str {
        "aws"
    }

    async fn list_services(&self) -> Result<Vec<CloudService>, CloudError> {
        let mut services = Vec::new();

        // List ECS services across clusters
        let clusters_data = self
            .aws_cli("ecs", &["list-clusters"])
            .await?;

        let cluster_arns = clusters_data["clusterArns"]
            .as_array()
            .cloned()
            .unwrap_or_default();

        for cluster_arn in &cluster_arns {
            let cluster = cluster_arn.as_str().unwrap_or("");
            let svc_data = self
                .aws_cli(
                    "ecs",
                    &["list-services", "--cluster", cluster],
                )
                .await?;

            let svc_arns = svc_data["serviceArns"]
                .as_array()
                .cloned()
                .unwrap_or_default();

            if svc_arns.is_empty() {
                continue;
            }

            let arn_strings: Vec<&str> = svc_arns
                .iter()
                .filter_map(|a| a.as_str())
                .collect();

            let mut describe_args = vec!["describe-services", "--cluster", cluster, "--services"];
            describe_args.extend(arn_strings.iter());

            let described = self.aws_cli("ecs", &describe_args).await?;
            if let Some(svcs) = described["services"].as_array() {
                for svc in svcs {
                    services.push(self.parse_ecs_service(svc));
                }
            }
        }

        // List EC2 instances
        let ec2_data = self
            .aws_cli("ec2", &["describe-instances"])
            .await?;

        if let Some(reservations) = ec2_data["Reservations"].as_array() {
            for reservation in reservations {
                if let Some(instances) = reservation["Instances"].as_array() {
                    for inst in instances {
                        let state = inst["State"]["Name"].as_str().unwrap_or("");
                        if state != "terminated" {
                            services.push(self.parse_ec2_instance(inst));
                        }
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
            .find(|s| s.name == name || s.provider_id.as_deref() == Some(name))
            .ok_or_else(|| CloudError::NotFound(format!("Service '{name}' not found on AWS")))
    }

    async fn provision(&self, spec: &ProvisionSpec) -> Result<CloudService, CloudError> {
        match spec.service_type {
            ServiceType::Container => {
                // Register a Fargate task definition and create an ECS service
                let image = spec.image.as_deref().unwrap_or("nginx:latest");
                let cpu = spec
                    .resources
                    .as_ref()
                    .and_then(|r| r.cpu.as_deref())
                    .unwrap_or("256");
                let memory = spec
                    .resources
                    .as_ref()
                    .and_then(|r| r.memory.as_deref())
                    .unwrap_or("512");

                // Register task definition
                let task_def = format!(
                    r#"{{
                        "family": "{name}",
                        "networkMode": "awsvpc",
                        "requiresCompatibilities": ["FARGATE"],
                        "cpu": "{cpu}",
                        "memory": "{memory}",
                        "containerDefinitions": [{{
                            "name": "{name}",
                            "image": "{image}",
                            "essential": true,
                            "portMappings": [{{"containerPort": 80, "protocol": "tcp"}}]
                        }}]
                    }}"#,
                    name = spec.name
                );

                self.aws_cli(
                    "ecs",
                    &["register-task-definition", "--cli-input-json", &task_def],
                )
                .await?;

                // Create service (requires a cluster — use "default")
                let svc_def = format!(
                    r#"{{
                        "cluster": "default",
                        "serviceName": "{name}",
                        "taskDefinition": "{name}",
                        "desiredCount": {instances},
                        "launchType": "FARGATE",
                        "networkConfiguration": {{
                            "awsvpcConfiguration": {{
                                "assignPublicIp": "{public_ip}",
                                "subnets": []
                            }}
                        }}
                    }}"#,
                    name = spec.name,
                    instances = spec.instances,
                    public_ip = if spec.public { "ENABLED" } else { "DISABLED" },
                );

                let result = self
                    .aws_cli(
                        "ecs",
                        &["create-service", "--cli-input-json", &svc_def],
                    )
                    .await?;

                info!(provider = "aws", service = %spec.name, "ECS service provisioned");

                Ok(CloudService {
                    name: spec.name.clone(),
                    provider: "aws".into(),
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
                    provider_id: result["service"]["serviceArn"]
                        .as_str()
                        .map(String::from),
                })
            }
            ServiceType::VM => {
                let image_id = spec
                    .image
                    .as_deref()
                    .unwrap_or("ami-0c02fb55956c7d316"); // Amazon Linux 2
                let instance_type = spec
                    .resources
                    .as_ref()
                    .and_then(|r| r.cpu.as_deref())
                    .unwrap_or("t3.micro");

                let result = self
                    .aws_cli(
                        "ec2",
                        &[
                            "run-instances",
                            "--image-id",
                            image_id,
                            "--instance-type",
                            instance_type,
                            "--count",
                            &spec.instances.to_string(),
                            "--tag-specifications",
                            &format!(
                                "ResourceType=instance,Tags=[{{Key=Name,Value={}}}]",
                                spec.name
                            ),
                        ],
                    )
                    .await?;

                let instance_id = result["Instances"]
                    .as_array()
                    .and_then(|arr| arr.first())
                    .and_then(|i| i["InstanceId"].as_str())
                    .map(String::from);

                info!(provider = "aws", service = %spec.name, "EC2 instance launched");

                Ok(CloudService {
                    name: spec.name.clone(),
                    provider: "aws".into(),
                    service_type: ServiceType::VM,
                    region: spec.region.clone(),
                    status: ServiceStatus::Deploying,
                    instances: spec.instances,
                    url: None,
                    image: Some(image_id.to_string()),
                    resources: Some(Resources {
                        cpu: Some(instance_type.to_string()),
                        memory: None,
                        disk: None,
                    }),
                    created_at: Some(Utc::now().to_rfc3339()),
                    provider_id: instance_id,
                })
            }
            _ => Err(CloudError::NotSupported(format!(
                "AWS adapter does not support service type '{}'",
                spec.service_type
            ))),
        }
    }

    async fn destroy(&self, name: &str) -> Result<(), CloudError> {
        let service = self.get_service(name).await?;
        match service.service_type {
            ServiceType::Container => {
                self.aws_cli(
                    "ecs",
                    &[
                        "delete-service",
                        "--cluster",
                        "default",
                        "--service",
                        name,
                        "--force",
                    ],
                )
                .await?;
                info!(provider = "aws", service = %name, "ECS service destroyed");
            }
            ServiceType::VM => {
                let instance_id = service.provider_id.as_deref().unwrap_or(name);
                self.aws_cli(
                    "ec2",
                    &["terminate-instances", "--instance-ids", instance_id],
                )
                .await?;
                info!(provider = "aws", service = %name, "EC2 instance terminated");
            }
            _ => {
                return Err(CloudError::NotSupported(format!(
                    "Cannot destroy service type '{}'",
                    service.service_type
                )));
            }
        }
        Ok(())
    }

    async fn deploy(
        &self,
        name: &str,
        config: &DeployConfig,
    ) -> Result<CloudService, CloudError> {
        // Update the ECS service with a new image via a fresh task definition
        let cpu = "256";
        let memory = "512";
        let port = config.port.unwrap_or(80);

        let task_def = format!(
            r#"{{
                "family": "{name}",
                "networkMode": "awsvpc",
                "requiresCompatibilities": ["FARGATE"],
                "cpu": "{cpu}",
                "memory": "{memory}",
                "containerDefinitions": [{{
                    "name": "{name}",
                    "image": "{image}",
                    "essential": true,
                    "portMappings": [{{"containerPort": {port}, "protocol": "tcp"}}]
                }}]
            }}"#,
            image = config.image,
        );

        self.aws_cli(
            "ecs",
            &["register-task-definition", "--cli-input-json", &task_def],
        )
        .await?;

        self.aws_cli(
            "ecs",
            &[
                "update-service",
                "--cluster",
                "default",
                "--service",
                name,
                "--task-definition",
                name,
                "--force-new-deployment",
            ],
        )
        .await?;

        info!(provider = "aws", service = %name, "Deploy triggered");
        self.get_service(name).await
    }

    async fn scale(&self, name: &str, instances: u32) -> Result<CloudService, CloudError> {
        self.aws_cli(
            "ecs",
            &[
                "update-service",
                "--cluster",
                "default",
                "--service",
                name,
                "--desired-count",
                &instances.to_string(),
            ],
        )
        .await?;

        info!(provider = "aws", service = %name, instances, "Scaled");

        let mut service = self.get_service(name).await?;
        service.instances = instances;
        Ok(service)
    }

    async fn restart(&self, name: &str) -> Result<(), CloudError> {
        self.aws_cli(
            "ecs",
            &[
                "update-service",
                "--cluster",
                "default",
                "--service",
                name,
                "--force-new-deployment",
            ],
        )
        .await?;
        info!(provider = "aws", service = %name, "Restarted via force deployment");
        Ok(())
    }

    async fn logs(&self, name: &str, lines: u32) -> Result<Vec<String>, CloudError> {
        let log_group = format!("/ecs/{name}");
        let data = self
            .aws_cli(
                "logs",
                &[
                    "get-log-events",
                    "--log-group-name",
                    &log_group,
                    "--log-stream-name",
                    "latest",
                    "--limit",
                    &lines.to_string(),
                ],
            )
            .await?;

        let log_lines = data["events"]
            .as_array()
            .map(|events| {
                events
                    .iter()
                    .filter_map(|e| e["message"].as_str().map(String::from))
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
            "Use AWS CloudWatch for detailed metrics".into(),
        ))
    }

    async fn estimate_cost(&self, spec: &ProvisionSpec) -> Result<CostEstimate, CloudError> {
        // ECS Fargate pricing: ~$0.04048/vCPU/hr + $0.004445/GB/hr
        let vcpu_str = spec
            .resources
            .as_ref()
            .and_then(|r| r.cpu.as_deref())
            .unwrap_or("256");
        let mem_str = spec
            .resources
            .as_ref()
            .and_then(|r| r.memory.as_deref())
            .unwrap_or("512");

        // CPU units: 256 = 0.25 vCPU, 512 = 0.5, 1024 = 1, etc.
        let vcpus = vcpu_str.parse::<f64>().unwrap_or(256.0) / 1024.0;
        // Memory in MB
        let mem_gb = mem_str.parse::<f64>().unwrap_or(512.0) / 1024.0;

        let cpu_hourly = vcpus * 0.04048 * spec.instances as f64;
        let mem_hourly = mem_gb * 0.004445 * spec.instances as f64;
        let total_hourly = cpu_hourly + mem_hourly;
        let monthly = total_hourly * 730.0;

        Ok(CostEstimate {
            hourly: total_hourly,
            monthly,
            currency: "USD".into(),
            breakdown: vec![
                CostLine {
                    item: format!("{}x {vcpus:.2} vCPU (Fargate)", spec.instances),
                    amount: cpu_hourly * 730.0,
                },
                CostLine {
                    item: format!("{}x {mem_gb:.2} GB RAM (Fargate)", spec.instances),
                    amount: mem_hourly * 730.0,
                },
            ],
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_aws_adapter_creation() {
        let adapter = AwsAdapter::new("AKIAIOSFODNN7EXAMPLE", "secret", "us-east-1");
        assert_eq!(adapter.provider_name(), "aws");
        assert_eq!(adapter.region, "us-east-1");
    }

    #[test]
    fn test_estimate_cost() {
        let adapter = AwsAdapter::new("key", "secret", "us-east-1");
        let spec = ProvisionSpec {
            name: "api".into(),
            instances: 2,
            resources: Some(Resources {
                cpu: Some("1024".into()),  // 1 vCPU
                memory: Some("2048".into()), // 2 GB
                disk: None,
            }),
            ..Default::default()
        };
        let rt = tokio::runtime::Runtime::new().unwrap();
        let cost = rt.block_on(adapter.estimate_cost(&spec)).unwrap();
        // 2 instances * (1 vCPU * $0.04048 + 2 GB * $0.004445) = ~$0.08985/hr
        assert!(cost.hourly > 0.0);
        assert!(cost.monthly > 0.0);
        assert_eq!(cost.currency, "USD");
        assert_eq!(cost.breakdown.len(), 2);
    }
}
