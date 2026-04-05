use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

use chrono::{DateTime, Utc};

use crate::adapter::{CloudService, CostEstimate};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CostReport {
    pub total_monthly: f64,
    pub currency: String,
    pub services: Vec<ServiceCost>,
    pub generated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceCost {
    pub name: String,
    pub provider: String,
    pub monthly: f64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct BudgetConfig {
    pub monthly_limit: Option<f64>,
    pub alert_threshold: Option<f64>,
}

impl CostReport {
    pub fn from_services(
        services: &[CloudService],
        estimates: &[(String, CostEstimate)],
    ) -> Self {
        let estimate_map: HashMap<&str, &CostEstimate> = estimates
            .iter()
            .map(|(name, est)| (name.as_str(), est))
            .collect();

        let mut service_costs = Vec::new();
        let mut total = 0.0;
        let mut currency = String::from("USD");

        for svc in services {
            let monthly = estimate_map
                .get(svc.name.as_str())
                .map(|e| {
                    currency.clone_from(&e.currency);
                    e.monthly
                })
                .unwrap_or(0.0);

            total += monthly;
            service_costs.push(ServiceCost {
                name: svc.name.clone(),
                provider: svc.provider.clone(),
                monthly,
            });
        }

        Self {
            total_monthly: total,
            currency,
            services: service_costs,
            generated_at: Utc::now(),
        }
    }

    pub fn save_cache(&self, path: &Path) -> Result<(), String> {
        let data = serde_json::to_string_pretty(self)
            .map_err(|e| format!("failed to serialize cost report: {e}"))?;
        std::fs::write(path, data)
            .map_err(|e| format!("failed to write cost cache: {e}"))
    }

    pub fn load_cache(path: &Path) -> Result<Self, String> {
        let data = std::fs::read_to_string(path)
            .map_err(|e| format!("failed to read cost cache: {e}"))?;
        serde_json::from_str(&data)
            .map_err(|e| format!("failed to parse cost cache: {e}"))
    }

    pub fn total_by_provider(&self) -> HashMap<String, f64> {
        let mut totals: HashMap<String, f64> = HashMap::new();
        for sc in &self.services {
            *totals.entry(sc.provider.clone()).or_default() += sc.monthly;
        }
        totals
    }
}

impl BudgetConfig {
    pub fn check_budget(&self, report: &CostReport) -> Option<String> {
        let limit = self.monthly_limit?;
        let threshold = self.alert_threshold.unwrap_or(0.8);
        let threshold_amount = limit * threshold;

        if report.total_monthly >= limit {
            Some(format!(
                "BUDGET EXCEEDED: ${:.2} / ${:.2} ({:.0}%)",
                report.total_monthly,
                limit,
                (report.total_monthly / limit) * 100.0
            ))
        } else if report.total_monthly >= threshold_amount {
            Some(format!(
                "BUDGET WARNING: ${:.2} / ${:.2} ({:.0}%) — threshold {:.0}%",
                report.total_monthly,
                limit,
                (report.total_monthly / limit) * 100.0,
                threshold * 100.0
            ))
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapter::{CostLine, ServiceStatus, ServiceType};

    fn make_service(name: &str, provider: &str) -> CloudService {
        CloudService {
            name: name.to_string(),
            provider: provider.to_string(),
            service_type: ServiceType::Container,
            region: "us-east-1".to_string(),
            status: ServiceStatus::Running,
            instances: 1,
            url: None,
            image: None,
            resources: None,
            created_at: None,
            provider_id: None,
        }
    }

    fn make_estimate(monthly: f64) -> CostEstimate {
        CostEstimate {
            hourly: monthly / 730.0,
            monthly,
            currency: "USD".to_string(),
            breakdown: vec![CostLine {
                item: "compute".to_string(),
                amount: monthly,
            }],
        }
    }

    #[test]
    fn test_from_services() {
        let services = vec![
            make_service("api", "aws"),
            make_service("worker", "gcp"),
        ];
        let estimates = vec![
            ("api".to_string(), make_estimate(50.0)),
            ("worker".to_string(), make_estimate(30.0)),
        ];

        let report = CostReport::from_services(&services, &estimates);
        assert!((report.total_monthly - 80.0).abs() < f64::EPSILON);
        assert_eq!(report.services.len(), 2);
        assert_eq!(report.currency, "USD");
    }

    #[test]
    fn test_total_by_provider() {
        let services = vec![
            make_service("a", "aws"),
            make_service("b", "aws"),
            make_service("c", "gcp"),
        ];
        let estimates = vec![
            ("a".to_string(), make_estimate(10.0)),
            ("b".to_string(), make_estimate(20.0)),
            ("c".to_string(), make_estimate(15.0)),
        ];

        let report = CostReport::from_services(&services, &estimates);
        let by_provider = report.total_by_provider();
        assert!((by_provider["aws"] - 30.0).abs() < f64::EPSILON);
        assert!((by_provider["gcp"] - 15.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_budget_alert() {
        let config = BudgetConfig {
            monthly_limit: Some(100.0),
            alert_threshold: Some(0.8),
        };

        let mut report = CostReport::default();

        report.total_monthly = 50.0;
        assert!(config.check_budget(&report).is_none());

        report.total_monthly = 85.0;
        let msg = config.check_budget(&report).unwrap();
        assert!(msg.contains("WARNING"));

        report.total_monthly = 120.0;
        let msg = config.check_budget(&report).unwrap();
        assert!(msg.contains("EXCEEDED"));
    }
}
