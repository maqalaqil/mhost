use axum::extract::{Path, State};
use axum::response::IntoResponse;
use axum::routing::{delete, get, post};
use axum::{Extension, Json, Router};
use serde::Deserialize;

use crate::auth::{require_role, AuthenticatedUser};
use crate::envelope::{ApiError, ApiResponse};
use crate::roles::Role;
use crate::server::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/v1/processes", get(list_processes))
        .route("/api/v1/processes/:name", get(get_process))
        .route("/api/v1/processes", post(start_process))
        .route("/api/v1/processes/:name/restart", post(restart_process))
        .route("/api/v1/processes/:name/stop", post(stop_process))
        .route("/api/v1/processes/:name/reload", post(reload_process))
        .route("/api/v1/processes/:name/scale", post(scale_process))
        .route("/api/v1/processes/:name", delete(delete_process))
        .route("/api/v1/processes/stop-all", post(stop_all))
        .route("/api/v1/processes/restart-all", post(restart_all))
}

async fn list_processes(
    State(state): State<AppState>,
    Extension(user): Extension<AuthenticatedUser>,
) -> Result<impl IntoResponse, ApiError> {
    require_role(&user, Role::Viewer)?;
    let processes = state.supervisor.list_processes().await;
    Ok(ApiResponse::new(processes))
}

async fn get_process(
    State(state): State<AppState>,
    Extension(user): Extension<AuthenticatedUser>,
    Path(name): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    require_role(&user, Role::Viewer)?;
    let process = state
        .supervisor
        .get_process(&name)
        .await
        .ok_or_else(|| ApiError::not_found(format!("process '{name}' not found")))?;
    Ok(ApiResponse::new(process))
}

async fn start_process(
    State(state): State<AppState>,
    Extension(user): Extension<AuthenticatedUser>,
    Json(config): Json<serde_json::Value>,
) -> Result<impl IntoResponse, ApiError> {
    require_role(&user, Role::Operator)?;
    let result = state
        .supervisor
        .start_process(config)
        .await
        .map_err(|e| ApiError::internal(e))?;
    Ok(ApiResponse::new(result))
}

async fn restart_process(
    State(state): State<AppState>,
    Extension(user): Extension<AuthenticatedUser>,
    Path(name): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    require_role(&user, Role::Operator)?;
    state
        .supervisor
        .restart_process(&name)
        .await
        .map_err(|e| ApiError::internal(e))?;
    Ok(ApiResponse::new(serde_json::json!({ "restarted": name })))
}

async fn stop_process(
    State(state): State<AppState>,
    Extension(user): Extension<AuthenticatedUser>,
    Path(name): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    require_role(&user, Role::Operator)?;
    state
        .supervisor
        .stop_process(&name)
        .await
        .map_err(|e| ApiError::internal(e))?;
    Ok(ApiResponse::new(serde_json::json!({ "stopped": name })))
}

async fn reload_process(
    State(state): State<AppState>,
    Extension(user): Extension<AuthenticatedUser>,
    Path(name): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    require_role(&user, Role::Operator)?;
    state
        .supervisor
        .reload_process(&name)
        .await
        .map_err(|e| ApiError::internal(e))?;
    Ok(ApiResponse::new(serde_json::json!({ "reloaded": name })))
}

#[derive(Deserialize)]
struct ScaleBody {
    instances: u32,
}

async fn scale_process(
    State(state): State<AppState>,
    Extension(user): Extension<AuthenticatedUser>,
    Path(name): Path<String>,
    Json(body): Json<ScaleBody>,
) -> Result<impl IntoResponse, ApiError> {
    require_role(&user, Role::Operator)?;
    state
        .supervisor
        .scale_process(&name, body.instances)
        .await
        .map_err(|e| ApiError::internal(e))?;
    Ok(ApiResponse::new(serde_json::json!({
        "scaled": name,
        "instances": body.instances,
    })))
}

async fn delete_process(
    State(state): State<AppState>,
    Extension(user): Extension<AuthenticatedUser>,
    Path(name): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    require_role(&user, Role::Operator)?;
    state
        .supervisor
        .delete_process(&name)
        .await
        .map_err(|e| ApiError::internal(e))?;
    Ok(ApiResponse::new(serde_json::json!({ "deleted": name })))
}

async fn stop_all(
    State(state): State<AppState>,
    Extension(user): Extension<AuthenticatedUser>,
) -> Result<impl IntoResponse, ApiError> {
    require_role(&user, Role::Operator)?;
    state
        .supervisor
        .stop_all()
        .await
        .map_err(|e| ApiError::internal(e))?;
    Ok(ApiResponse::new(serde_json::json!({ "stopped": "all" })))
}

async fn restart_all(
    State(state): State<AppState>,
    Extension(user): Extension<AuthenticatedUser>,
) -> Result<impl IntoResponse, ApiError> {
    require_role(&user, Role::Operator)?;
    state
        .supervisor
        .restart_all()
        .await
        .map_err(|e| ApiError::internal(e))?;
    Ok(ApiResponse::new(serde_json::json!({ "restarted": "all" })))
}
