use std::net::SocketAddr;
use std::sync::Arc;

use axum::{extract::State, response::IntoResponse, routing::get, Router};
use mhost_core::ProcessInfo;
use tokio::net::TcpListener;
use tracing::{error, info};

// ---------------------------------------------------------------------------
// PrometheusExporter
// ---------------------------------------------------------------------------

/// Generates Prometheus text-format metrics and optionally exposes them over
/// an HTTP endpoint at `GET /metrics`.
pub struct PrometheusExporter;

impl PrometheusExporter {
    /// Render a slice of `ProcessInfo` records into the Prometheus text format.
    ///
    /// Emitted metrics:
    /// * `mhost_process_cpu_percent`    — CPU usage percentage (gauge)
    /// * `mhost_process_memory_bytes`   — RSS memory in bytes (gauge)
    /// * `mhost_process_uptime_seconds` — seconds since process started (gauge)
    /// * `mhost_process_restart_total`  — total restart count (counter)
    pub fn render_metrics(processes: &[ProcessInfo]) -> String {
        let mut out = String::with_capacity(512 * processes.len().max(1));

        // --- cpu_percent ---
        out.push_str("# HELP mhost_process_cpu_percent CPU usage percentage\n");
        out.push_str("# TYPE mhost_process_cpu_percent gauge\n");
        for p in processes {
            let cpu = p.cpu_percent.unwrap_or(0.0);
            out.push_str(&format!(
                "mhost_process_cpu_percent{{name=\"{}\",instance=\"{}\"}} {}\n",
                p.config.name, p.instance, cpu
            ));
        }

        // --- memory_bytes ---
        out.push_str("# HELP mhost_process_memory_bytes Memory usage in bytes\n");
        out.push_str("# TYPE mhost_process_memory_bytes gauge\n");
        for p in processes {
            let mem = p.memory_bytes.unwrap_or(0);
            out.push_str(&format!(
                "mhost_process_memory_bytes{{name=\"{}\",instance=\"{}\"}} {}\n",
                p.config.name, p.instance, mem
            ));
        }

        // --- uptime_seconds ---
        out.push_str("# HELP mhost_process_uptime_seconds Process uptime in seconds\n");
        out.push_str("# TYPE mhost_process_uptime_seconds gauge\n");
        for p in processes {
            let uptime = p.uptime_seconds().unwrap_or(0).max(0) as u64;
            out.push_str(&format!(
                "mhost_process_uptime_seconds{{name=\"{}\",instance=\"{}\"}} {}\n",
                p.config.name, p.instance, uptime
            ));
        }

        // --- restart_total ---
        out.push_str("# HELP mhost_process_restart_total Total process restart count\n");
        out.push_str("# TYPE mhost_process_restart_total counter\n");
        for p in processes {
            out.push_str(&format!(
                "mhost_process_restart_total{{name=\"{}\",instance=\"{}\"}} {}\n",
                p.config.name, p.instance, p.restart_count
            ));
        }

        out
    }

    /// Spawn an Axum HTTP server that exposes `GET /metrics`.
    ///
    /// `data_fn` is called on every request to obtain the current set of
    /// process records; it must be `Send + Sync + 'static`.
    pub async fn start<F>(listen: SocketAddr, data_fn: F)
    where
        F: Fn() -> Vec<ProcessInfo> + Send + Sync + 'static,
    {
        let data_fn = Arc::new(data_fn);

        let app = Router::new()
            .route("/metrics", get(metrics_handler))
            .with_state(data_fn);

        info!(%listen, "Prometheus exporter listening");

        let listener = match TcpListener::bind(listen).await {
            Ok(l) => l,
            Err(e) => {
                error!(%e, "failed to bind Prometheus exporter");
                return;
            }
        };

        if let Err(e) = axum::serve(listener, app).await {
            error!(%e, "Prometheus exporter server error");
        }
    }
}

// ---------------------------------------------------------------------------
// Handler
// ---------------------------------------------------------------------------

async fn metrics_handler(
    State(data_fn): State<Arc<dyn Fn() -> Vec<ProcessInfo> + Send + Sync>>,
) -> impl IntoResponse {
    let processes = data_fn();
    let body = PrometheusExporter::render_metrics(&processes);
    (
        [(
            axum::http::header::CONTENT_TYPE,
            "text/plain; version=0.0.4; charset=utf-8",
        )],
        body,
    )
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use mhost_core::{ProcessConfig, ProcessInfo, ProcessStatus};

    fn make_process(name: &str, instance: u32) -> ProcessInfo {
        let cfg = ProcessConfig {
            name: name.to_string(),
            command: "test".to_string(),
            ..Default::default()
        };
        let mut p = ProcessInfo::new(cfg, instance);
        p.cpu_percent = Some(42.5);
        p.memory_bytes = Some(134_217_728);
        p.uptime_started = Some(Utc::now() - chrono::Duration::seconds(86400));
        p.restart_count = 2;
        p.status = ProcessStatus::Online;
        p
    }

    #[test]
    fn render_metrics_contains_expected_lines() {
        let processes = vec![make_process("api", 0)];
        let output = PrometheusExporter::render_metrics(&processes);

        assert!(
            output.contains("mhost_process_cpu_percent{name=\"api\",instance=\"0\"} 42.5"),
            "missing cpu_percent line in:\n{output}"
        );
        assert!(
            output.contains("mhost_process_memory_bytes{name=\"api\",instance=\"0\"} 134217728"),
            "missing memory_bytes line in:\n{output}"
        );
        assert!(
            output.contains("mhost_process_restart_total{name=\"api\",instance=\"0\"} 2"),
            "missing restart_total line in:\n{output}"
        );
    }

    #[test]
    fn render_metrics_has_help_and_type_headers() {
        let processes = vec![make_process("worker", 1)];
        let output = PrometheusExporter::render_metrics(&processes);

        for metric in &[
            "mhost_process_cpu_percent",
            "mhost_process_memory_bytes",
            "mhost_process_uptime_seconds",
            "mhost_process_restart_total",
        ] {
            assert!(
                output.contains(&format!("# HELP {metric}")),
                "missing # HELP for {metric}"
            );
            assert!(
                output.contains(&format!("# TYPE {metric}")),
                "missing # TYPE for {metric}"
            );
        }
    }

    #[test]
    fn render_metrics_empty_slice_produces_headers_only() {
        let output = PrometheusExporter::render_metrics(&[]);
        assert!(output.contains("# HELP mhost_process_cpu_percent"));
        // No data lines (no `{name=` entries)
        assert!(!output.contains("{name="));
    }

    #[test]
    fn render_metrics_uptime_is_non_negative() {
        let processes = vec![make_process("api", 0)];
        let output = PrometheusExporter::render_metrics(&processes);
        // Extract the uptime line and verify value is >= 0
        for line in output.lines() {
            if line.starts_with("mhost_process_uptime_seconds{") {
                let value_str = line.rsplit_once(' ').map(|(_, v)| v).unwrap_or("0");
                let value: u64 = value_str.parse().expect("uptime should be an integer");
                assert!(value > 0, "uptime should be > 0 for a 24h-old process");
            }
        }
    }

    #[test]
    fn render_metrics_multiple_processes() {
        let processes = vec![make_process("api", 0), make_process("api", 1)];
        let output = PrometheusExporter::render_metrics(&processes);

        assert!(output.contains("instance=\"0\""));
        assert!(output.contains("instance=\"1\""));
    }
}
