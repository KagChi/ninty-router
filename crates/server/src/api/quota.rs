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
    Router::new()
        .route("/quota", get(all_quota))
        .route("/quota/{id}", get(one_quota))
        .route("/request-logs", get(request_logs))
        .route("/stats", get(stats))
        .route("/history", get(history))
        .route("/providers", get(providers))
}

/// GET /api/usage/request-logs?limit= — newest first (ring buffer cap 1000).
async fn request_logs(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    axum::extract::Query(q): axum::extract::Query<std::collections::HashMap<String, String>>,
) -> Result<Json<Value>, ApiError> {
    super::require_session(&state, &headers).await?;
    let limit: i64 = q.get("limit").and_then(|v| v.parse().ok()).unwrap_or(200);
    let rows = crate::repos::usage::list_request_details(&state.db, limit).await?;
    Ok(Json(serde_json::json!({ "logs": rows })))
}

/// GET /api/usage/stats — totals + today + per-model breakdown.
async fn stats(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Json<Value>, ApiError> {
    super::require_session(&state, &headers).await?;
    Ok(Json(crate::repos::usage::stats(&state.db).await?))
}

/// GET /api/usage/history?days= — tokens/day for charts.
async fn history(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    axum::extract::Query(q): axum::extract::Query<std::collections::HashMap<String, String>>,
) -> Result<Json<Value>, ApiError> {
    super::require_session(&state, &headers).await?;
    let days: i64 = q.get("days").and_then(|v| v.parse().ok()).unwrap_or(30);
    Ok(Json(
        serde_json::json!({ "days": crate::repos::usage::history(&state.db, days).await? }),
    ))
}

/// GET /api/usage/providers — per-provider aggregates.
async fn providers(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Json<Value>, ApiError> {
    super::require_session(&state, &headers).await?;
    Ok(Json(
        serde_json::json!({ "providers": crate::repos::usage::by_provider(&state.db).await? }),
    ))
}

async fn cache_get(state: &Arc<AppState>, key: &str) -> Option<Value> {
    let key = key.to_string();
    let raw: Option<String> = state
        .db
        .call(move |conn| {
            Ok(conn
                .query_row(
                    "SELECT value FROM kv WHERE scope = 'quota' AND key = ?1",
                    [&key],
                    |r| r.get::<_, String>(0),
                )
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
                "INSERT INTO kv (scope, key, value) VALUES ('quota', ?1, ?2)
                 ON CONFLICT(scope, key) DO UPDATE SET value = excluded.value",
                [&key, &payload],
            )
            .map_err(|e| ninty_core::error::Error::Db(e.to_string()))?;
            Ok(())
        })
        .await;
}

/// Eligibility (9router ProviderLimits filter): provider has quota support and
/// the connection's auth kind is allowed (oauth && usage) || (apikey && usageApikey).
fn eligible(conn: &crate::repos::connections::Connection) -> bool {
    let (usage, usage_apikey) = ninty_core::registry::features(&conn.provider);
    let has_oauth = conn.data.get("accessToken").and_then(Value::as_str).is_some();
    let has_key = conn.api_key().is_some();
    (has_oauth && usage) || (has_key && usage_apikey)
}

async fn fetch_for(
    state: &Arc<AppState>,
    conn: &crate::repos::connections::Connection,
) -> QuotaReport {
    let key = format!("quota:{}", conn.id);
    if let Some(cached) = cache_get(state, &key).await {
        if let Ok(r) = serde_json::from_value::<QuotaReport>(cached) {
            return r;
        }
    }
    let report = fetch_live(state, conn).await;
    if report.error.is_none() {
        if let Ok(v) = serde_json::to_value(&report) {
            cache_set(state, &key, &v).await;
        }
    }
    report
}

/// Live quota fetch — proactive oauth refresh, dispatch per provider.
async fn fetch_live(
    state: &Arc<AppState>,
    conn: &crate::repos::connections::Connection,
) -> QuotaReport {
    // Proactive refresh for oauth connections near expiry (9router refreshes before).
    let conn = match crate::oauth_state::ensure_fresh(state, conn).await {
        Ok(c) => c,
        Err(_) => conn.clone(),
    };
    let d = &conn.data;
    let token = d.get("accessToken").and_then(Value::as_str).unwrap_or("");
    let key = conn.api_key().unwrap_or("");
    let mut report = match conn.provider.as_str() {
        "codex" => {
            quota::codex(
                &state.http,
                &conn.id,
                token,
                d.get("chatgptAccountId").and_then(Value::as_str),
            )
            .await
        }
        "github" => quota::github(&state.http, &conn.id, token).await,
        "claude" => quota::claude(&state.http, &conn.id, token).await,
        "codebuddy-cn" | "codebuddy-intl" => {
            let t = if token.is_empty() { key } else { token };
            quota::codebuddy(&state.http, &conn.id, &conn.provider, t).await
        }
        "deepseek" => quota::deepseek(&state.http, &conn.id, key).await,
        "glm" | "glm-cn" => quota::glm(&state.http, &conn.id, &conn.provider, key).await,
        "minimax" | "minimax-cn" => {
            quota::minimax(&state.http, &conn.id, &conn.provider, key).await
        }
        "kimi" => {
            let device = d
                .get("deviceId")
                .and_then(Value::as_str)
                .unwrap_or("ninty-router");
            quota::kimi(&state.http, &conn.id, token, key, device).await
        }
        "qoder" => quota::qoder(&state.http, &conn.id, token).await,
        _ => QuotaReport::err(&conn.id, &conn.provider, "quota not supported"),
    };
    // Card display metadata (9router getConnectionLabel).
    let str_field = |k: &str| d.get(k).and_then(Value::as_str).map(String::from);
    let name = conn
        .name
        .clone()
        .filter(|s| !s.trim().is_empty())
        .or_else(|| str_field("email"))
        .or_else(|| str_field("displayName"));
    let secondary = str_field("email")
        .or_else(|| str_field("displayName"))
        .filter(|s| Some(s) != name.as_ref());
    report.label = name;
    report.secondary = secondary;
    report.active = conn.is_active;
    report.priority = conn.priority;
    report
}

/// Auth-expired heuristic (9router isAuthExpiredMessage).
fn is_auth_expired(msg: &str) -> bool {
    let m = msg.to_lowercase();
    ["expired", "authentication", "unauthorized", "401", "re-authorize"]
        .iter()
        .any(|k| m.contains(k))
}

/// GET /api/usage/quota/{id} — live per-connection fetch (per-card refresh).
/// No cache; on auth-expired message + oauth → force refresh once, retry once.
async fn one_quota(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    axum::extract::Path(id): axum::extract::Path<String>,
) -> Result<Json<Value>, ApiError> {
    super::require_session(&state, &headers).await?;
    let conn = crate::repos::connections::get(&state.db, &id)
        .await?
        .ok_or_else(|| ninty_core::error::Error::NotFound("connection".into()))?;
    if !eligible(&conn) {
        return Ok(Json(serde_json::json!({
            "connection_id": conn.id,
            "provider": conn.provider,
            "plan": null,
            "windows": [],
            "error": "Usage not available for this connection",
            "fetched_at": chrono::Utc::now().to_rfc3339(),
        })));
    }
    let mut report = fetch_live(&state, &conn).await;
    let expired = report
        .error
        .as_deref()
        .map(is_auth_expired)
        .unwrap_or(false);
    if expired && conn.data.get("refreshToken").and_then(Value::as_str).is_some() {
        if let Ok(fresh) = crate::oauth_state::refresh_now(&state, &conn).await {
            report = fetch_live(&state, &fresh).await;
        }
    }
    Ok(Json(serde_json::to_value(&report).unwrap_or_default()))
}

async fn all_quota(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Json<Value>, ApiError> {
    super::require_session(&state, &headers).await?;
    let conns = crate::repos::connections::list(&state.db, None).await?;
    // All eligible connections (incl. inactive — UI filters + shows disabled state).
    let supported: Vec<_> = conns.into_iter().filter(eligible).collect();
    let mut reports = Vec::with_capacity(supported.len());
    for conn in supported {
        reports.push(fetch_for(&state, &conn).await);
    }
    Ok(Json(serde_json::json!({"reports": reports})))
}
