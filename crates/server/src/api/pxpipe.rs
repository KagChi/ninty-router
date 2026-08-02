//! /api/pxpipe — status, install (npm/bun pxpipe-proxy into $DATA_DIR/pxpipe),
//! health self-test. Mirrors $REF/src/app/api/pxpipe/*.

use std::sync::Arc;

use axum::extract::State;
use axum::http::HeaderMap;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde_json::json;

use crate::api::{require_session, ApiError};
use crate::state::AppState;
use ninty_core::config;

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/status", get(status))
        .route("/health", get(health))
        .route("/install", post(install))
}

fn installing(state: &AppState) -> bool {
    state
        .pxpipe_installing
        .load(std::sync::atomic::Ordering::Relaxed)
}

async fn status(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, ApiError> {
    require_session(&state, &headers).await?;
    let info = engine::pxpipe::get_install_info(&config::data_dir());
    Ok(Json(json!({
        "installed": info.installed,
        "installing": installing(&state),
        "version": info.version,
        "path": info.path,
        "npmAvailable": engine::pxpipe::find_runtime().is_some(),
        "mode": "subprocess",
    })))
}

async fn health(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, ApiError> {
    require_session(&state, &headers).await?;
    Ok(Json(
        serde_json::to_value(engine::pxpipe::run_health_check(&config::data_dir()).await)?,
    ))
}

async fn install(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, ApiError> {
    require_session(&state, &headers).await?;
    // Serialized: concurrent calls return the in-flight marker.
    if state
        .pxpipe_installing
        .swap(true, std::sync::atomic::Ordering::Relaxed)
    {
        return Ok(Json(json!({ "installing": true })));
    }
    let st = state.clone();
    tokio::spawn(async move {
        let result = engine::pxpipe::install_pxpipe(&config::data_dir()).await;
        match &result {
            Ok(v) => tracing::info!("PXPIPE installed v{v}"),
            Err(e) => tracing::warn!("PXPIPE install failed: {e}"),
        }
        st.pxpipe_installing
            .store(false, std::sync::atomic::Ordering::Relaxed);
    });
    Ok(Json(json!({ "installing": true })))
}
