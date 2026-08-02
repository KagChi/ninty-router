use std::sync::Arc;

use axum::extract::{Path, State};
use axum::http::HeaderMap;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::json;

use crate::api::{require_session, ApiError};
use crate::repos::api_keys::{self, NewApiKey};
use crate::state::AppState;

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/", get(list_keys).post(create_key))
        .route("/{id}", axum::routing::put(update_key).delete(delete_key))
        .route("/{id}/reset", post(reset_key))
}

async fn list_keys(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, ApiError> {
    require_session(&state, &headers).await?;
    let keys = api_keys::list(&state.db).await?;
    Ok(Json(json!({"keys": keys})))
}

async fn create_key(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(body): Json<NewApiKey>,
) -> Result<Json<serde_json::Value>, ApiError> {
    require_session(&state, &headers).await?;
    let key = api_keys::create(&state.db, body).await?;
    Ok(Json(json!({"key": key})))
}

#[derive(Deserialize)]
struct UpdateKeyBody {
    #[serde(flatten)]
    patch: NewApiKey,
    is_active: Option<bool>,
}

async fn update_key(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(body): Json<UpdateKeyBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    require_session(&state, &headers).await?;
    api_keys::update(&state.db, &id, body.patch, body.is_active).await?;
    Ok(Json(json!({"ok": true})))
}

async fn delete_key(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    require_session(&state, &headers).await?;
    api_keys::delete(&state.db, &id).await?;
    Ok(Json(json!({"ok": true})))
}

async fn reset_key(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    require_session(&state, &headers).await?;
    api_keys::reset_limit(&state.db, &id).await?;
    Ok(Json(json!({"ok": true})))
}
