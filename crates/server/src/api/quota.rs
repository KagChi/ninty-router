//! GET /api/usage/quota — per-connection quota reports (5min kv cache).

use std::sync::Arc;

use axum::extract::State;
use axum::http::HeaderMap;
use axum::routing::get;
use axum::{Json, Router};
use serde_json::Value;

use super::ApiError;
use crate::state::AppState;
use engine::quota::{self, QuotaReport};

const CACHE_TTL_S: i64 = 300;

pub fn router() -> Router<Arc<AppState>> {
    Router::new().route("/quota", get(all_quota))
}

async fn cache_get(state: &Arc<AppState>, key: &str) -> Option<Value> {
    let key = key.to_string();
    let raw: Option<String> = state
        .db
        .call(move |conn| {
            Ok(conn
                .query_row("SELECT value FROM kv WHERE key = ?1", [&key], |r| r.get::<_, String>(0))
                .ok())
        })
        .await
        .ok()
        .flatten();
    let raw = raw?;
    let v: Value = serde_json::from_str(&raw).ok()?;
    let ts = v.get("ts").and_then(Value::as_i64)?;
    if chrono::Utc::now().timestamp() - ts > CACHE_TTL_S {
        return None;
    }
    v.get("data").cloned()
}

async fn cache_set(state: &Arc<AppState>, key: &str, data: &Value) {
    let (key, payload) = (
        key.to_string(),
        serde_json::json!({"ts": chrono::Utc::now().timestamp(), "data": data}).to_string(),
    );
    let _ = state
        .db
        .call(move |conn| {
            conn.execute(
                "INSERT INTO kv (key, value) VALUES (?1, ?2)
                 ON CONFLICT(key) DO UPDATE SET value = excluded.value",
                [&key, &payload],
            )
            .map_err(|e| ninty_core::error::Error::Db(e.to_string()))?;
            Ok(())
        })
        .await;
}

async fn fetch_for(state: &Arc<AppState>, conn: &crate::repos::connections::Connection) -> QuotaReport {
    let key = format!("quota:{}", conn.id);
    if let Some(cached) = cache_get(state, &key).await {
        if let Ok(r) = serde_json::from_value::<QuotaReport>(cached) {
            return r;
        }
    }
    let d = &conn.data;
    let token = d.get("accessToken").and_then(Value::as_str).unwrap_or("");
    let report = match conn.provider.as_str() {
        "codex" => quota::codex(&state.http, &conn.id, token, d.get("chatgptAccountId").and_then(Value::as_str)).await,
        "github" => quota::github(&state.http, &conn.id, token).await,
        "claude" => quota::claude(&state.http, &conn.id, token).await,
        "codebuddy-cn" | "codebuddy-intl" => {
            let t = if token.is_empty() { conn.api_key().unwrap_or("") } else { token };
            quota::codebuddy(&state.http, &conn.id, &conn.provider, t).await
        }
        _ => QuotaReport::err(&conn.id, &conn.provider, "quota not supported"),
    };
    if report.error.is_none() {
        if let Ok(v) = serde_json::to_value(&report) {
            cache_set(state, &key, &v).await;
        }
    }
    report
}

async fn all_quota(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Json<Value>, ApiError> {
    super::require_session(&state, &headers).await?;
    let conns = crate::repos::connections::list(&state.db, None).await?;
    let supported: Vec<_> = conns
        .into_iter()
        .filter(|c| c.is_active && matches!(c.provider.as_str(), "codex" | "github" | "claude" | "codebuddy-cn" | "codebuddy-intl"))
        .collect();
    let mut reports = Vec::with_capacity(supported.len());
    for conn in supported {
        reports.push(fetch_for(&state, &conn).await);
    }
    Ok(Json(serde_json::json!({"reports": reports})))
}
