use axum::extract::{Path, State};
use axum::response::IntoResponse;
use axum::routing::{delete, get, post};
use axum::{Extension, Json, Router};
use serde::Deserialize;

use crate::auth::{require_role, AuthenticatedUser};
use crate::envelope::{ApiError, ApiResponse};
use crate::event_bus::ProcessEvent;
use crate::roles::Role;
use crate::server::AppState;
use crate::webhook_dispatch::WebhookConfig;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/v1/webhooks", get(list_webhooks))
        .route("/api/v1/webhooks", post(create_webhook))
        .route("/api/v1/webhooks/:id", delete(remove_webhook))
        .route("/api/v1/webhooks/:id/test", post(test_webhook))
}

async fn list_webhooks(
    State(state): State<AppState>,
    Extension(user): Extension<AuthenticatedUser>,
) -> Result<impl IntoResponse, ApiError> {
    require_role(&user, Role::Admin)?;
    let hooks = state.webhooks.list();
    Ok(ApiResponse::new(hooks))
}

#[derive(Deserialize)]
struct CreateWebhookBody {
    url: String,
    events: Vec<String>,
    secret: Option<String>,
}

async fn create_webhook(
    State(state): State<AppState>,
    Extension(user): Extension<AuthenticatedUser>,
    Json(body): Json<CreateWebhookBody>,
) -> Result<impl IntoResponse, ApiError> {
    require_role(&user, Role::Admin)?;

    if body.url.is_empty() {
        return Err(ApiError::bad_request("webhook url must not be empty"));
    }
    if body.events.is_empty() {
        return Err(ApiError::bad_request(
            "webhook must subscribe to at least one event",
        ));
    }

    let config = WebhookConfig {
        id: String::new(),
        url: body.url,
        events: body.events,
        secret: body.secret,
        enabled: true,
        failure_count: 0,
    };

    let id = state.webhooks.add(config).map_err(ApiError::internal)?;

    Ok(ApiResponse::new(serde_json::json!({ "id": id })))
}

async fn remove_webhook(
    State(state): State<AppState>,
    Extension(user): Extension<AuthenticatedUser>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    require_role(&user, Role::Admin)?;
    state.webhooks.remove(&id).map_err(ApiError::not_found)?;
    Ok(ApiResponse::new(serde_json::json!({ "removed": id })))
}

async fn test_webhook(
    State(state): State<AppState>,
    Extension(user): Extension<AuthenticatedUser>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    require_role(&user, Role::Admin)?;

    let test_event = ProcessEvent::new("test", "mhost-api");

    state
        .webhooks
        .dispatch_to(&id, &test_event)
        .map_err(ApiError::not_found)?;

    Ok(ApiResponse::new(serde_json::json!({
        "tested": id,
        "event": "test",
    })))
}
