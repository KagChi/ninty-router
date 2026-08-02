use std::sync::Arc;

use axum::extract::State;
use axum::http::HeaderMap;
use axum::routing::get;
use axum::{Json, Router};

use crate::api::{require_session, ApiError};
use crate::state::AppState;

pub fn router() -> Router<Arc<AppState>> {
    Router::new().route("/", get(get_settings).patch(patch_settings))
}

async fn get_settings(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, ApiError> {
    require_session(&state, &headers).await?;
    let settings = crate::repos::settings::get(&state.db).await?;
    let password_set = !settings.password_hash.is_empty();
    let mut value = serde_json::to_value(settings)?;
    if let Some(obj) = value.as_object_mut() {
        obj.remove("password_hash");
        obj.insert("password_set".into(), serde_json::Value::Bool(password_set));
    }
    Ok(Json(value))
}

async fn patch_settings(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(patch): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, ApiError> {
    require_session(&state, &headers).await?;
    let updated = crate::repos::settings::patch(&state.db, patch).await?;
    Ok(Json(serde_json::to_value(updated)?))
}
