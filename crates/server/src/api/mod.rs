mod auth_routes;
mod combos;
mod import;
mod keys;
mod oauth;
mod providers;
mod pxpipe;
mod quota;
mod settings_routes;

use std::sync::Arc;

use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use serde_json::json;

use crate::auth;
use crate::state::AppState;
use ninty_core::error::Error;

pub fn router(state: Arc<AppState>) -> Router {
    let api = Router::new()
        .route("/health", get(health))
        .nest("/auth", auth_routes::router())
        .nest("/settings", settings_routes::router())
        .nest("/combos", combos::router())
        .nest("/oauth", oauth::router())
        .nest("/import", import::router())
        .nest("/usage", quota::router())
        .nest("/keys", keys::router())
        .nest("/providers", providers::router())
        .nest("/pxpipe", pxpipe::router())
        .with_state(state.clone());

    Router::new()
        .nest("/api", api)
        .nest("/v1", crate::v1::router())
        .route(
            "/v1beta/models/{*path}",
            axum::routing::post(crate::v1::v1beta::models_action),
        )
        .with_state(state)
}

async fn health() -> Json<serde_json::Value> {
    Json(json!({"status": "ok", "service": "ninty-router"}))
}

/// Guard for protected routes: passes when login not required or session cookie valid.
pub(crate) async fn require_session(state: &AppState, headers: &HeaderMap) -> Result<(), ApiError> {
    let settings = crate::repos::settings::get(&state.db).await?;
    if !settings.require_login || settings.password_hash.is_empty() {
        return Ok(());
    }
    let cookie = headers
        .get(axum::http::header::COOKIE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    match auth::token_from_cookie_header(cookie) {
        Some(token) if auth::verify_session(&token) => Ok(()),
        _ => Err(ApiError(Error::Unauthorized)),
    }
}

pub struct ApiError(pub Error);

impl From<Error> for ApiError {
    fn from(e: Error) -> Self {
        ApiError(e)
    }
}

impl From<serde_json::Error> for ApiError {
    fn from(e: serde_json::Error) -> Self {
        ApiError(Error::from(e))
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, msg) = match &self.0 {
            Error::NotFound(m) => (StatusCode::NOT_FOUND, m.clone()),
            Error::BadRequest(m) => (StatusCode::BAD_REQUEST, m.clone()),
            Error::Unauthorized => (StatusCode::UNAUTHORIZED, "unauthorized".to_string()),
            Error::Upstream { status, message } => {
                return (
                    StatusCode::from_u16(*status).unwrap_or(StatusCode::BAD_GATEWAY),
                    Json(json!({"error": {"message": message}})),
                )
                    .into_response();
            }
            other => (StatusCode::INTERNAL_SERVER_ERROR, other.to_string()),
        };
        (status, Json(json!({"error": {"message": msg}}))).into_response()
    }
}
