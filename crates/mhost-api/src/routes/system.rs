use axum::extract::State;
use axum::response::IntoResponse;
use axum::routing::post;
use axum::{Extension, Router};

use crate::auth::{require_role, AuthenticatedUser};
use crate::envelope::{ApiError, ApiResponse};
use crate::roles::Role;
use crate::server::AppState;

/// Returns the authenticated system routes (save, resurrect, kill).
pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/v1/system/save", post(save))
        .route("/api/v1/system/resurrect", post(resurrect))
        .route("/api/v1/system/kill", post(kill_daemon))
}

/// Public version endpoint (no auth required). Used directly in server.rs.
pub async fn version(State(state): State<AppState>) -> impl IntoResponse {
    ApiResponse::new(state.supervisor.version_info())
}

async fn save(
    State(state): State<AppState>,
    Extension(user): Extension<AuthenticatedUser>,
) -> Result<impl IntoResponse, ApiError> {
    require_role(&user, Role::Operator)?;
    state.supervisor.save().await.map_err(ApiError::internal)?;
    Ok(ApiResponse::new(serde_json::json!({ "saved": true })))
}

async fn resurrect(
    State(state): State<AppState>,
    Extension(user): Extension<AuthenticatedUser>,
) -> Result<impl IntoResponse, ApiError> {
    require_role(&user, Role::Operator)?;
    let result = state
        .supervisor
        .resurrect()
        .await
        .map_err(ApiError::internal)?;
    Ok(ApiResponse::new(result))
}

async fn kill_daemon(
    State(state): State<AppState>,
    Extension(user): Extension<AuthenticatedUser>,
) -> Result<impl IntoResponse, ApiError> {
    require_role(&user, Role::Admin)?;

    // Attempt graceful shutdown: stop all processes then exit.
    let _ = state.supervisor.stop_all().await;

    // Schedule process exit after response is sent.
    tokio::spawn(async {
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        std::process::exit(0);
    });

    Ok(ApiResponse::new(serde_json::json!({ "killing": true })))
}
