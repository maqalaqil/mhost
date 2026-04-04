use axum::extract::{Path, State};
use axum::response::IntoResponse;
use axum::routing::get;
use axum::{Extension, Router};

use crate::auth::{require_role, AuthenticatedUser};
use crate::envelope::{ApiError, ApiResponse};
use crate::roles::Role;
use crate::server::AppState;

/// Returns the authenticated router for process-level health checks.
pub fn router() -> Router<AppState> {
    Router::new().route("/api/v1/processes/:name/health", get(process_health))
}

/// Public health endpoint (no auth required). Used directly in server.rs.
pub async fn daemon_health() -> impl IntoResponse {
    ApiResponse::new(serde_json::json!({
        "status": "ok",
    }))
}

/// Returns the health status of a specific process.
async fn process_health(
    State(state): State<AppState>,
    Extension(user): Extension<AuthenticatedUser>,
    Path(name): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    require_role(&user, Role::Viewer)?;
    let health = state
        .supervisor
        .health_status(&name)
        .await
        .map_err(ApiError::internal)?;
    Ok(ApiResponse::new(health))
}
