use axum::extract::Request;
use axum::middleware::Next;
use axum::response::Response;
use std::sync::Arc;
use tokio::sync::Mutex;

use crate::envelope::ApiError;
use crate::rate_limit::RateLimiter;
use crate::roles::Role;
use crate::tokens::TokenStore;

/// Shared state passed to the auth middleware via axum's `State` extractor.
#[derive(Clone)]
pub struct AuthState {
    pub token_store: Arc<Mutex<TokenStore>>,
    pub rate_limiter: Arc<Mutex<RateLimiter>>,
}

/// Represents a successfully authenticated API user.
///
/// Inserted into request extensions by `auth_middleware` so that downstream
/// handlers can access identity and role information without re-verifying.
#[derive(Debug, Clone)]
pub struct AuthenticatedUser {
    pub token_id: String,
    pub name: String,
    pub role: Role,
}

/// Axum middleware that authenticates requests via bearer tokens and enforces
/// per-token rate limits.
///
/// Flow:
/// 1. Extract bearer token from the `Authorization` header.
/// 2. Verify the token against `TokenStore`.
/// 3. Check per-token rate limit via `RateLimiter`.
/// 4. Update the token's `last_used` timestamp.
/// 5. Insert `AuthenticatedUser` into request extensions.
pub async fn auth_middleware(
    axum::extract::State(state): axum::extract::State<AuthState>,
    mut req: Request,
    next: Next,
) -> Result<Response, ApiError> {
    let raw_token = extract_bearer(&req)?;

    let mut store = state.token_store.lock().await;

    let api_token = store
        .verify(&raw_token)
        .ok_or_else(|| ApiError::unauthorized("invalid or expired token"))?
        .clone();

    let mut limiter = state.rate_limiter.lock().await;
    if !limiter.check(&api_token.id) {
        return Err(ApiError::too_many_requests("rate limit exceeded"));
    }
    drop(limiter);

    // Best-effort timestamp update — do not fail the request on I/O error.
    let _ = store.update_last_used(&api_token.id);
    drop(store);

    let user = AuthenticatedUser {
        token_id: api_token.id,
        name: api_token.name,
        role: api_token.role,
    };

    req.extensions_mut().insert(user);

    Ok(next.run(req).await)
}

/// Checks that `user` has at least the permissions of `required`.
///
/// Returns `Err(ApiError)` with 403 Forbidden when the user's role is
/// insufficient.
pub fn require_role(user: &AuthenticatedUser, required: Role) -> Result<(), ApiError> {
    if user.role.has_permission(required) {
        Ok(())
    } else {
        Err(ApiError::forbidden(format!(
            "role '{}' does not have '{}' permission",
            user.role, required
        )))
    }
}

/// Extracts the bearer token from the `Authorization` header.
///
/// Expects the format `Bearer <token>`. Returns an error when the header is
/// missing or malformed.
fn extract_bearer(req: &Request) -> Result<String, ApiError> {
    let header = req
        .headers()
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| ApiError::unauthorized("missing authorization header"))?;

    let token = header
        .strip_prefix("Bearer ")
        .ok_or_else(|| ApiError::unauthorized("authorization header must use Bearer scheme"))?;

    if token.is_empty() {
        return Err(ApiError::unauthorized("bearer token is empty"));
    }

    Ok(token.to_string())
}
