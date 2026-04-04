use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use axum::middleware;
use axum::routing::{get, Router};
use tokio::sync::Mutex;
use tower_http::cors::{Any, CorsLayer};

use mhost_core::MhostPaths;

use crate::auth::{auth_middleware, AuthState};
use crate::event_bus::EventBus;
use crate::rate_limit::RateLimiter;
use crate::routes;
use crate::tokens::TokenStore;
use crate::webhook_dispatch::WebhookDispatcher;
use crate::websocket::ws_handler;

/// Trait that bridges the daemon's Supervisor into the API layer.
///
/// The daemon implements this trait so route handlers can interact with
/// managed processes without depending on supervisor internals.
#[async_trait]
pub trait SupervisorApi: Send + Sync + 'static {
    async fn list_processes(&self) -> Vec<serde_json::Value>;
    async fn get_process(&self, name: &str) -> Option<serde_json::Value>;
    async fn start_process(&self, config: serde_json::Value) -> Result<serde_json::Value, String>;
    async fn stop_process(&self, name: &str) -> Result<(), String>;
    async fn restart_process(&self, name: &str) -> Result<(), String>;
    async fn reload_process(&self, name: &str) -> Result<(), String>;
    async fn delete_process(&self, name: &str) -> Result<(), String>;
    async fn scale_process(&self, name: &str, instances: u32) -> Result<(), String>;
    async fn stop_all(&self) -> Result<(), String>;
    async fn restart_all(&self) -> Result<(), String>;
    async fn save(&self) -> Result<(), String>;
    async fn resurrect(&self) -> Result<serde_json::Value, String>;
    async fn health_status(&self, name: &str) -> Result<serde_json::Value, String>;
    async fn metrics(&self, name: &str) -> Result<serde_json::Value, String>;
    async fn all_metrics(&self) -> Result<serde_json::Value, String>;
    async fn get_logs(&self, name: &str, lines: usize, err: bool) -> Result<Vec<String>, String>;
    async fn search_logs(
        &self,
        name: &str,
        query: &str,
        since: Option<&str>,
    ) -> Result<Vec<String>, String>;
    fn version_info(&self) -> serde_json::Value;
}

/// Shared application state available to all route handlers.
#[derive(Clone)]
pub struct AppState {
    pub auth: AuthState,
    pub event_bus: EventBus,
    pub paths: MhostPaths,
    pub webhooks: Arc<WebhookDispatcher>,
    pub supervisor: Arc<dyn SupervisorApi>,
}

/// Starts the HTTP/WebSocket API server.
///
/// Builds the axum router with public and authenticated routes, enables CORS,
/// spawns a background task that forwards event bus events to webhooks, and
/// binds to the given socket address.
pub async fn start_api_server(
    bind: SocketAddr,
    paths: MhostPaths,
    supervisor: Arc<dyn SupervisorApi>,
    event_bus: EventBus,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let token_store = TokenStore::load(paths.api_tokens())
        .map_err(|e| format!("failed to load token store: {e}"))?;

    let webhooks = WebhookDispatcher::load(paths.webhooks_config(), paths.webhook_failures())
        .map_err(|e| format!("failed to load webhooks: {e}"))?;
    let webhooks = Arc::new(webhooks);

    let auth_state = AuthState {
        token_store: Arc::new(Mutex::new(token_store)),
        rate_limiter: Arc::new(Mutex::new(RateLimiter::new(100, Duration::from_secs(60)))),
    };

    let state = AppState {
        auth: auth_state.clone(),
        event_bus: event_bus.clone(),
        paths,
        webhooks: Arc::clone(&webhooks),
        supervisor,
    };

    // Spawn webhook event forwarder
    let webhook_ref = Arc::clone(&webhooks);
    let mut event_rx = event_bus.subscribe();
    tokio::spawn(async move {
        loop {
            match event_rx.recv().await {
                Ok(event) => {
                    webhook_ref.dispatch(&event);
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                    tracing::warn!("webhook forwarder lagged, skipped {n} events");
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                    tracing::info!("event bus closed, webhook forwarder exiting");
                    break;
                }
            }
        }
    });

    // Public routes (no auth required)
    let public_routes = Router::new()
        .route("/api/v1/health", get(routes::health::daemon_health))
        .route("/api/v1/version", get(routes::system::version));

    // Authenticated routes
    let authed_routes = Router::new()
        .merge(routes::processes::router())
        .merge(routes::logs::router())
        .merge(routes::health::router())
        .merge(routes::metrics::router())
        .merge(routes::system::router())
        .merge(routes::tokens::router())
        .merge(routes::webhooks::router())
        .route("/api/v1/ws", get(ws_handler))
        .layer(middleware::from_fn_with_state(auth_state, auth_middleware));

    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    let app = Router::new()
        .merge(public_routes)
        .merge(authed_routes)
        .layer(cors)
        .with_state(state);

    tracing::info!("API server listening on {bind}");

    let listener = tokio::net::TcpListener::bind(bind).await?;
    axum::serve(listener, app).await?;

    Ok(())
}
