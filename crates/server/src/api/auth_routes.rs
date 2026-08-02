use std::sync::Arc;

use axum::extract::State;
use axum::http::{header, HeaderMap};
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::json;

use crate::api::{require_session, ApiError};
use crate::auth;
use crate::state::AppState;
use ninty_core::error::Error;

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/login", post(login))
        .route("/logout", post(logout))
        .route("/status", get(status))
        .route("/set-password", post(set_password))
}

#[derive(Deserialize)]
struct LoginBody {
    password: String,
}

async fn login(
    State(state): State<Arc<AppState>>,
    Json(body): Json<LoginBody>,
) -> Result<impl IntoResponse, ApiError> {
    let settings = crate::repos::settings::get(&state.db).await?;
    if settings.password_hash.is_empty()
        || !auth::verify_password(&body.password, &settings.password_hash)
    {
        return Err(ApiError(Error::Unauthorized));
    }
    let token = auth::create_session()?;
    let cookie = format!(
        "{}={}; Path=/; HttpOnly; SameSite=Lax; Max-Age={}",
        auth::SESSION_COOKIE,
        token,
        7 * 24 * 3600
    );
    Ok((
        [(header::SET_COOKIE, cookie)],
        Json(json!({"authenticated": true})),
    ))
}

async fn logout() -> impl IntoResponse {
    let cookie = format!(
        "{}=; Path=/; HttpOnly; SameSite=Lax; Max-Age=0",
        auth::SESSION_COOKIE
    );
    ([(header::SET_COOKIE, cookie)], Json(json!({"ok": true})))
}

async fn status(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, ApiError> {
    let settings = crate::repos::settings::get(&state.db).await?;
    let password_set = !settings.password_hash.is_empty();
    let required = settings.require_login && password_set;
    let authenticated = !required || require_session(&state, &headers).await.is_ok();
    Ok(Json(json!({
        "authenticated": authenticated,
        "require_login": required,
        "password_set": password_set,
    })))
}

#[derive(Deserialize)]
struct SetPasswordBody {
    password: String,
}

async fn set_password(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(body): Json<SetPasswordBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    // If a password already exists and login is enforced, must be authed to change it.
    let settings = crate::repos::settings::get(&state.db).await?;
    if !settings.password_hash.is_empty() {
        require_session(&state, &headers).await?;
    }
    if body.password.len() < 4 {
        return Err(ApiError(Error::BadRequest(
            "password must be at least 4 characters".into(),
        )));
    }
    let mut settings = settings;
    settings.password_hash = auth::hash_password(&body.password)?;
    if !settings.require_login {
        settings.require_login = true;
    }
    crate::repos::settings::put(&state.db, &settings).await?;
    Ok(Json(
        json!({"ok": true, "require_login": settings.require_login}),
    ))
}
