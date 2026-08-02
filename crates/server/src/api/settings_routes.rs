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
    Json(mut patch): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, ApiError> {
    require_session(&state, &headers).await?;
    // 9router profile contract: {currentPassword, newPassword} — verify current,
    // hash+store. Handled before generic merge (keys stripped either way).
    let current_pw = patch
        .get("currentPassword")
        .and_then(|v| v.as_str())
        .map(String::from);
    let new_pw = patch
        .get("newPassword")
        .and_then(|v| v.as_str())
        .map(String::from);
    if let Some(obj) = patch.as_object_mut() {
        obj.remove("currentPassword");
        obj.remove("newPassword");
    }
    if current_pw.is_some() || new_pw.is_some() {
        let (Some(cur), Some(new)) = (current_pw, new_pw) else {
            return Err(ninty_core::error::Error::BadRequest(
                "currentPassword and newPassword required".into(),
            )
            .into());
        };
        let settings = crate::repos::settings::get(&state.db).await?;
        if !settings.password_hash.is_empty()
            && !crate::auth::verify_password(&cur, &settings.password_hash)
        {
            return Err(ninty_core::error::Error::BadRequest("current password is wrong".into()).into());
        }
        if new.len() < 4 {
            return Err(
                ninty_core::error::Error::BadRequest("password must be at least 4 characters".into())
                    .into(),
            );
        }
        let mut settings = settings;
        settings.password_hash = crate::auth::hash_password(&new)?;
        crate::repos::settings::put(&state.db, &settings).await?;
    }
    let updated = crate::repos::settings::patch(&state.db, patch).await?;
    state.set_request_logs(updated.enable_request_logs);
    let password_set = !updated.password_hash.is_empty();
    let mut value = serde_json::to_value(updated)?;
    if let Some(obj) = value.as_object_mut() {
        obj.remove("password_hash");
        obj.insert("password_set".into(), serde_json::Value::Bool(password_set));
    }
    Ok(Json(value))
}
