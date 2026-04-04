use axum::extract::{Path, Query, State};
use axum::response::IntoResponse;
use axum::routing::get;
use axum::{Extension, Router};
use serde::Deserialize;

use crate::auth::{require_role, AuthenticatedUser};
use crate::envelope::{ApiError, ApiResponse};
use crate::roles::Role;
use crate::server::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/v1/processes/:name/logs", get(get_logs))
        .route("/api/v1/processes/:name/logs/search", get(search_logs))
}

#[derive(Deserialize)]
struct LogsQuery {
    #[serde(default = "default_lines")]
    lines: usize,
    #[serde(default)]
    err: bool,
}

fn default_lines() -> usize {
    100
}

async fn get_logs(
    State(state): State<AppState>,
    Extension(user): Extension<AuthenticatedUser>,
    Path(name): Path<String>,
    Query(params): Query<LogsQuery>,
) -> Result<impl IntoResponse, ApiError> {
    require_role(&user, Role::Viewer)?;
    let lines = state
        .supervisor
        .get_logs(&name, params.lines, params.err)
        .await
        .map_err(ApiError::internal)?;
    Ok(ApiResponse::new(lines))
}

#[derive(Deserialize)]
struct SearchQuery {
    q: String,
    since: Option<String>,
}

async fn search_logs(
    State(state): State<AppState>,
    Extension(user): Extension<AuthenticatedUser>,
    Path(name): Path<String>,
    Query(params): Query<SearchQuery>,
) -> Result<impl IntoResponse, ApiError> {
    require_role(&user, Role::Viewer)?;
    let lines = state
        .supervisor
        .search_logs(&name, &params.q, params.since.as_deref())
        .await
        .map_err(ApiError::internal)?;
    Ok(ApiResponse::new(lines))
}
