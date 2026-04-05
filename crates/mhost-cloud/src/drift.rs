use serde::{Deserialize, Serialize};

use crate::adapter::CloudService;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DriftResult {
    pub service: String,
    pub provider: String,
    pub drifted: bool,
    pub differences: Vec<DriftDifference>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DriftDifference {
    pub field: String,
    pub expected: String,
    pub actual: String,
}

pub fn detect_drift(desired: &CloudService, actual: &CloudService) -> DriftResult {
    let mut differences = Vec::new();

    if desired.instances != actual.instances {
        differences.push(DriftDifference {
            field: "instances".to_string(),
            expected: desired.instances.to_string(),
            actual: actual.instances.to_string(),
        });
    }

    if desired.status != actual.status {
        differences.push(DriftDifference {
            field: "status".to_string(),
            expected: desired.status.to_string(),
            actual: actual.status.to_string(),
        });
    }

    if desired.image != actual.image {
        differences.push(DriftDifference {
            field: "image".to_string(),
            expected: desired.image.clone().unwrap_or_default(),
            actual: actual.image.clone().unwrap_or_default(),
        });
    }

    let desired_cpu = desired.resources.as_ref().and_then(|r| r.cpu.clone());
    let actual_cpu = actual.resources.as_ref().and_then(|r| r.cpu.clone());
    if desired_cpu != actual_cpu {
        differences.push(DriftDifference {
            field: "resources.cpu".to_string(),
            expected: desired_cpu.unwrap_or_default(),
            actual: actual_cpu.unwrap_or_default(),
        });
    }

    let desired_mem = desired.resources.as_ref().and_then(|r| r.memory.clone());
    let actual_mem = actual.resources.as_ref().and_then(|r| r.memory.clone());
    if desired_mem != actual_mem {
        differences.push(DriftDifference {
            field: "resources.memory".to_string(),
            expected: desired_mem.unwrap_or_default(),
            actual: actual_mem.unwrap_or_default(),
        });
    }

    let desired_disk = desired.resources.as_ref().and_then(|r| r.disk.clone());
    let actual_disk = actual.resources.as_ref().and_then(|r| r.disk.clone());
    if desired_disk != actual_disk {
        differences.push(DriftDifference {
            field: "resources.disk".to_string(),
            expected: desired_disk.unwrap_or_default(),
            actual: actual_disk.unwrap_or_default(),
        });
    }

    let drifted = !differences.is_empty();

    DriftResult {
        service: desired.name.clone(),
        provider: desired.provider.clone(),
        drifted,
        differences,
    }
}

impl DriftResult {
    pub fn summary(&self) -> String {
        if !self.drifted {
            return format!("{}: no drift detected", self.service);
        }

        let diffs: Vec<String> = self
            .differences
            .iter()
            .map(|d| format!("  {} : expected '{}', actual '{}'", d.field, d.expected, d.actual))
            .collect();

        format!(
            "{}: {} drift(s) detected\n{}",
            self.service,
            self.differences.len(),
            diffs.join("\n")
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapter::{Resources, ServiceStatus, ServiceType};

    fn base_service() -> CloudService {
        CloudService {
            name: "my-app".to_string(),
            provider: "aws".to_string(),
            service_type: ServiceType::Container,
            region: "us-east-1".to_string(),
            status: ServiceStatus::Running,
            instances: 2,
            url: None,
            image: Some("my-app:v1".to_string()),
            resources: Some(Resources {
                cpu: Some("1".to_string()),
                memory: Some("512Mi".to_string()),
                disk: None,
            }),
            created_at: None,
            provider_id: None,
        }
    }

    #[test]
    fn test_no_drift() {
        let desired = base_service();
        let actual = desired.clone();
        let result = detect_drift(&desired, &actual);

        assert!(!result.drifted);
        assert!(result.differences.is_empty());
        assert!(result.summary().contains("no drift"));
    }

    #[test]
    fn test_instance_drift() {
        let desired = base_service();
        let mut actual = desired.clone();
        actual.instances = 1;

        let result = detect_drift(&desired, &actual);
        assert!(result.drifted);
        assert_eq!(result.differences.len(), 1);
        assert_eq!(result.differences[0].field, "instances");
        assert_eq!(result.differences[0].expected, "2");
        assert_eq!(result.differences[0].actual, "1");
    }

    #[test]
    fn test_multiple_drifts() {
        let desired = base_service();
        let mut actual = desired.clone();
        actual.instances = 5;
        actual.image = Some("my-app:v2".to_string());
        actual.status = ServiceStatus::Stopped;

        let result = detect_drift(&desired, &actual);
        assert!(result.drifted);
        assert_eq!(result.differences.len(), 3);

        let fields: Vec<&str> = result.differences.iter().map(|d| d.field.as_str()).collect();
        assert!(fields.contains(&"instances"));
        assert!(fields.contains(&"image"));
        assert!(fields.contains(&"status"));

        let summary = result.summary();
        assert!(summary.contains("3 drift(s)"));
    }
}
