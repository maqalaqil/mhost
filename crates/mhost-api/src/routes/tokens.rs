use axum::extract::{Path, State};
use axum::response::IntoResponse;
use axum::routing::{delete, get, post};
use axum::{Extension, Json, Router};
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};

use crate::auth::{require_role, AuthenticatedUser};
use crate::envelope::{ApiError, ApiResponse};
use crate::roles::Role;
use crate::server::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/v1/tokens", get(list_tokens))
        .route("/api/v1/tokens", post(create_token))
        .route("/api/v1/tokens/:id", delete(revoke_token))
}

#[derive(Serialize)]
struct TokenInfo {
    id: String,
    name: String,
    role: Role,
    created_at: DateTime<Utc>,
    last_used: Option<DateTime<Utc>>,
    expires_at: Option<DateTime<Utc>>,
}

async fn list_tokens(
    State(state): State<AppState>,
    Extension(user): Extension<AuthenticatedUser>,
) -> Result<impl IntoResponse, ApiError> {
    require_role(&user, Role::Admin)?;
    let store = state.auth.token_store.lock().await;
    let tokens: Vec<TokenInfo> = store
        .list()
        .iter()
        .map(|t| TokenInfo {
            id: t.id.clone(),
            name: t.name.clone(),
            role: t.role,
            created_at: t.created_at,
            last_used: t.last_used,
            expires_at: t.expires_at,
        })
        .collect();
    Ok(ApiResponse::new(tokens))
}

#[derive(Deserialize)]
struct CreateTokenBody {
    name: String,
    role: Role,
    expires: Option<String>,
}

#[derive(Serialize)]
struct CreatedTokenResponse {
    id: String,
    name: String,
    role: Role,
    secret: String,
    expires_at: Option<DateTime<Utc>>,
}

async fn create_token(
    State(state): State<AppState>,
    Extension(user): Extension<AuthenticatedUser>,
    Json(body): Json<CreateTokenBody>,
) -> Result<impl IntoResponse, ApiError> {
    require_role(&user, Role::Admin)?;

    if body.name.is_empty() {
        return Err(ApiError::bad_request("token name must not be empty"));
    }

    let expires_at = match &body.expires {
        Some(s) => Some(
            parse_duration_from_now(s)
                .map_err(|e| ApiError::bad_request(format!("invalid expires value: {e}")))?,
        ),
        None => None,
    };

    let mut store = state.auth.token_store.lock().await;
    let created = store
        .create(body.name, body.role, expires_at)
        .map_err(|e| ApiError::internal(format!("failed to create token: {e}")))?;

    Ok(ApiResponse::new(CreatedTokenResponse {
        id: created.token.id,
        name: created.token.name,
        role: created.token.role,
        secret: created.raw_secret,
        expires_at: created.token.expires_at,
    }))
}

async fn revoke_token(
    State(state): State<AppState>,
    Extension(user): Extension<AuthenticatedUser>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    require_role(&user, Role::Admin)?;
    let mut store = state.auth.token_store.lock().await;
    store
        .revoke(&id)
        .map_err(|e| ApiError::not_found(format!("token not found: {e}")))?;
    Ok(ApiResponse::new(serde_json::json!({ "revoked": id })))
}

/// Parses a human-readable duration string (e.g. "30d", "1h", "2w")
/// and returns a `DateTime<Utc>` that many units in the future.
fn parse_duration_from_now(s: &str) -> Result<DateTime<Utc>, String> {
    let s = s.trim();
    if s.is_empty() {
        return Err("empty duration string".to_string());
    }

    let (num_str, unit) = s.split_at(s.len() - 1);
    let num: i64 = num_str
        .parse()
        .map_err(|_| format!("invalid number in duration: '{num_str}'"))?;

    if num <= 0 {
        return Err("duration must be positive".to_string());
    }

    let duration = match unit {
        "s" => Duration::seconds(num),
        "m" => Duration::minutes(num),
        "h" => Duration::hours(num),
        "d" => Duration::days(num),
        "w" => Duration::weeks(num),
        _ => return Err(format!("unknown duration unit: '{unit}' (expected s/m/h/d/w)")),
    };

    Ok(Utc::now() + duration)
}
