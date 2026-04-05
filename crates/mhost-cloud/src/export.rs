use crate::adapter::CloudService;

pub fn export_terraform(services: &[CloudService]) -> String {
    let mut blocks = Vec::new();

    blocks.push("terraform {\n  required_version = \">= 1.0\"\n}".to_string());

    for svc in services {
        let resource_name = svc.name.replace('-', "_");
        let image = svc.image.as_deref().unwrap_or("latest");
        let region = &svc.region;
        let instances = svc.instances;

        let mut resource_block = format!(
            "resource \"cloud_service\" \"{resource_name}\" {{\n\
             \x20 name      = \"{}\"\n\
             \x20 provider  = \"{}\"\n\
             \x20 region    = \"{region}\"\n\
             \x20 image     = \"{image}\"\n\
             \x20 instances = {instances}\n",
            svc.name, svc.provider
        );

        if let Some(ref res) = svc.resources {
            resource_block.push_str("\n  resources {\n");
            if let Some(ref cpu) = res.cpu {
                resource_block.push_str(&format!("    cpu    = \"{cpu}\"\n"));
            }
            if let Some(ref mem) = res.memory {
                resource_block.push_str(&format!("    memory = \"{mem}\"\n"));
            }
            if let Some(ref disk) = res.disk {
                resource_block.push_str(&format!("    disk   = \"{disk}\"\n"));
            }
            resource_block.push_str("  }\n");
        }

        resource_block.push('}');
        blocks.push(resource_block);
    }

    blocks.join("\n\n")
}

pub fn export_docker_compose(services: &[CloudService]) -> String {
    let mut lines = Vec::new();
    lines.push("services:".to_string());

    for svc in services {
        let image = svc.image.as_deref().unwrap_or("latest");
        lines.push(format!("  {}:", svc.name));
        lines.push(format!("    image: {image}"));

        if let Some(ref res) = svc.resources {
            lines.push("    deploy:".to_string());
            lines.push("      resources:".to_string());
            lines.push("        limits:".to_string());
            if let Some(ref cpu) = res.cpu {
                lines.push(format!("          cpus: \"{cpu}\""));
            }
            if let Some(ref mem) = res.memory {
                lines.push(format!("          memory: {mem}"));
            }
        }

        if svc.instances > 1 {
            lines.push(format!("    scale: {}", svc.instances));
        }

        lines.push("    restart: unless-stopped".to_string());
    }

    lines.join("\n")
}

pub fn export_kubernetes(services: &[CloudService]) -> String {
    let mut manifests = Vec::new();

    for svc in services {
        let image = svc.image.as_deref().unwrap_or("latest");
        let replicas = svc.instances;

        let mut deployment = format!(
            "apiVersion: apps/v1\n\
             kind: Deployment\n\
             metadata:\n\
             \x20 name: {}\n\
             spec:\n\
             \x20 replicas: {replicas}\n\
             \x20 selector:\n\
             \x20   matchLabels:\n\
             \x20     app: {}\n\
             \x20 template:\n\
             \x20   metadata:\n\
             \x20     labels:\n\
             \x20       app: {}\n\
             \x20   spec:\n\
             \x20     containers:\n\
             \x20       - name: {}\n\
             \x20         image: {image}\n",
            svc.name, svc.name, svc.name, svc.name
        );

        if let Some(ref res) = svc.resources {
            deployment.push_str("          resources:\n");
            deployment.push_str("            limits:\n");
            if let Some(ref cpu) = res.cpu {
                deployment.push_str(&format!("              cpu: \"{cpu}\"\n"));
            }
            if let Some(ref mem) = res.memory {
                deployment.push_str(&format!("              memory: {mem}\n"));
            }
        }

        let k8s_service = format!(
            "apiVersion: v1\n\
             kind: Service\n\
             metadata:\n\
             \x20 name: {}\n\
             spec:\n\
             \x20 selector:\n\
             \x20   app: {}\n\
             \x20 ports:\n\
             \x20   - port: 80\n\
             \x20     targetPort: 8080\n",
            svc.name, svc.name
        );

        manifests.push(deployment);
        manifests.push(k8s_service);
    }

    manifests.join("---\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapter::{Resources, ServiceStatus, ServiceType};

    fn sample_services() -> Vec<CloudService> {
        vec![CloudService {
            name: "web-app".to_string(),
            provider: "aws".to_string(),
            service_type: ServiceType::Container,
            region: "us-east-1".to_string(),
            status: ServiceStatus::Running,
            instances: 2,
            url: None,
            image: Some("web-app:v1".to_string()),
            resources: Some(Resources {
                cpu: Some("0.5".to_string()),
                memory: Some("256Mi".to_string()),
                disk: None,
            }),
            created_at: None,
            provider_id: None,
        }]
    }

    #[test]
    fn test_export_terraform() {
        let output = export_terraform(&sample_services());
        assert!(output.contains("resource \"cloud_service\" \"web_app\""));
        assert!(output.contains("name      = \"web-app\""));
        assert!(output.contains("region    = \"us-east-1\""));
        assert!(output.contains("instances = 2"));
        assert!(output.contains("cpu    = \"0.5\""));
        assert!(output.contains("memory = \"256Mi\""));
    }

    #[test]
    fn test_export_docker_compose() {
        let output = export_docker_compose(&sample_services());
        assert!(output.contains("services:"));
        assert!(output.contains("web-app:"));
        assert!(output.contains("image: web-app:v1"));
        assert!(output.contains("cpus: \"0.5\""));
        assert!(output.contains("memory: 256Mi"));
        assert!(output.contains("scale: 2"));
    }

    #[test]
    fn test_export_kubernetes() {
        let output = export_kubernetes(&sample_services());
        assert!(output.contains("kind: Deployment"));
        assert!(output.contains("kind: Service"));
        assert!(output.contains("name: web-app"));
        assert!(output.contains("replicas: 2"));
        assert!(output.contains("image: web-app:v1"));
        assert!(output.contains("app: web-app"));
    }
}
