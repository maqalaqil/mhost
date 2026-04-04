use axum::extract::{Path, State};
use axum::response::IntoResponse;
use axum::routing::get;
use axum::{Extension, Router};

use crate::auth::{require_role, AuthenticatedUser};
use crate::envelope::{ApiError, ApiResponse};
use crate::roles::Role;
use crate::server::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/v1/metrics", get(all_metrics))
        .route("/api/v1/processes/:name/metrics", get(process_metrics))
}

async fn all_metrics(
    State(state): State<AppState>,
    Extension(user): Extension<AuthenticatedUser>,
) -> Result<impl IntoResponse, ApiError> {
    require_role(&user, Role::Viewer)?;
    let metrics = state
        .supervisor
        .all_metrics()
        .await
        .map_err(|e| ApiError::internal(e))?;
    Ok(ApiResponse::new(metrics))
}

async fn process_metrics(
    State(state): State<AppState>,
    Extension(user): Extension<AuthenticatedUser>,
    Path(name): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    require_role(&user, Role::Viewer)?;
    let metrics = state
        .supervisor
        .metrics(&name)
        .await
        .map_err(|e| ApiError::internal(e))?;
    Ok(ApiResponse::new(metrics))
}
