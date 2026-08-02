//! /api/models — aliases, disabled models, custom models (9router kv scopes 1:1).
//! Storage: kv scope modelAliases (key=alias → target spec), disabledModels
//! (key=provider → json ids), customModels (key=provider|id → json model).

use std::sync::Arc;

use axum::extract::{Query, State};
use axum::http::HeaderMap;
use axum::routing::{get, put};
use axum::{Json, Router};
use serde_json::{json, Value};

use super::{require_session, ApiError};
use crate::state::AppState;
use ninty_core::registry;

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/alias", get(list_aliases).put(set_alias).delete(delete_alias))
        .route(
            "/disabled",
            get(list_disabled).post(disable_models).delete(enable_models),
        )
        .route(
            "/custom",
            get(list_custom).post(add_custom).delete(delete_custom),
        )
        .route("/test", axum::routing::post(test_model))
        .route("/", put(set_alias))
}

// ---------- model test (9router /api/models/test) ----------

#[derive(serde::Deserialize)]
struct TestModelBody {
    provider: Option<String>,
    model: Option<String>,
}

/// POST /api/models/test — minimal chat ping against the first eligible
/// connection of the provider (format-aware body).
async fn test_model(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(body): Json<TestModelBody>,
) -> Result<Json<Value>, ApiError> {
    require_session(&state, &headers).await?;
    let (Some(provider_id), Some(model)) = (body.provider, body.model) else {
        return Err(ninty_core::error::Error::BadRequest("provider and model required".into()).into());
    };
    let spec = format!("{provider_id}/{model}");
    let targets = crate::v1::chat::resolve_targets(&state, &spec, registry::WireFormat::Openai)
        .await
        .map_err(ApiError::from)?;
    let Some(target) = targets.first() else {
        return Ok(Json(json!({"ok": false, "message": "no eligible connection"})));
    };
    let (url, headers2) = crate::v1::chat::build_url_and_auth(&state, target, false)
        .await
        .map_err(ApiError::from)?;
    let body_out = match target.format {
        registry::WireFormat::Claude => json!({
            "model": target.model, "max_tokens": 1,
            "messages": [{"role": "user", "content": "ping"}],
        }),
        registry::WireFormat::Gemini => json!({
            "contents": [{"role": "user", "parts": [{"text": "ping"}]}],
        }),
        registry::WireFormat::Responses => json!({"model": target.model, "input": "ping"}),
        registry::WireFormat::Openai => json!({
            "model": target.model,
            "messages": [{"role": "user", "content": "ping"}],
            "max_tokens": 1,
            // force_stream providers (codebuddy-intl: 11101 on non-stream) —
            // mirror the chat pipeline.
            "stream": target.force_stream,
        }),
    };
    let mut req = state
        .http
        .post(&url)
        .timeout(std::time::Duration::from_secs(30))
        .header("content-type", "application/json");
    for (k, v) in &headers2 {
        req = req.header(k, v);
    }
    match req.json(&body_out).send().await {
        Err(e) => Ok(Json(json!({"ok": false, "message": e.to_string()}))),
        Ok(resp) => {
            let s = resp.status().as_u16();
            if resp.status().is_success() {
                Ok(Json(json!({"ok": true, "status": s})))
            } else {
                let text = resp.text().await.unwrap_or_default();
                Ok(Json(json!({"ok": false, "status": s, "message": text.chars().take(200).collect::<String>()})))
            }
        }
    }
}

// ---------- kv helpers ----------

async fn kv_get_all(state: &Arc<AppState>, scope: &str) -> Vec<(String, String)> {
    let scope = scope.to_string();
    state
        .db
        .call(move |conn| {
            let out = conn
                .prepare("SELECT key, value FROM kv WHERE scope = ?1")
                .and_then(|mut stmt| {
                    let rows: Vec<(String, String)> = stmt
                        .query_map([&scope], |r| {
                            Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
                        })?
                        .filter_map(|r| r.ok())
                        .collect();
                    Ok(rows)
                })
                .unwrap_or_default();
            Ok(out)
        })
        .await
        .unwrap_or_default()
}

async fn kv_get(state: &Arc<AppState>, scope: &str, key: &str) -> Option<String> {
    let (scope, key) = (scope.to_string(), key.to_string());
    state
        .db
        .call(move |conn| {
            Ok(conn
                .query_row(
                    "SELECT value FROM kv WHERE scope = ?1 AND key = ?2",
                    [&scope, &key],
                    |r| r.get::<_, String>(0),
                )
                .ok())
        })
        .await
        .ok()
        .flatten()
}

async fn kv_set(state: &Arc<AppState>, scope: &str, key: &str, value: &str) {
    let (scope, key, value) = (scope.to_string(), key.to_string(), value.to_string());
    let _ = state
        .db
        .call(move |conn| {
            conn.execute(
                "INSERT INTO kv (scope, key, value) VALUES (?1, ?2, ?3)
                 ON CONFLICT(scope, key) DO UPDATE SET value = excluded.value",
                [&scope, &key, &value],
            )
            .map_err(|e| ninty_core::error::Error::Db(e.to_string()))?;
            Ok(())
        })
        .await;
}

async fn kv_del(state: &Arc<AppState>, scope: &str, key: &str) {
    let (scope, key) = (scope.to_string(), key.to_string());
    let _ = state
        .db
        .call(move |conn| {
            conn.execute(
                "DELETE FROM kv WHERE scope = ?1 AND key = ?2",
                [&scope, &key],
            )
            .map_err(|e| ninty_core::error::Error::Db(e.to_string()))?;
            Ok(())
        })
        .await;
}

// ---------- shared lookups (chat path uses these) ----------

/// Resolve an alias to its target spec ("provider/model").
pub async fn resolve_alias(state: &Arc<AppState>, alias: &str) -> Option<String> {
    kv_get(state, "modelAliases", alias).await
}

pub async fn all_aliases(state: &Arc<AppState>) -> std::collections::HashMap<String, String> {
    kv_get_all(state, "modelAliases").await.into_iter().collect()
}

pub async fn disabled_ids(state: &Arc<AppState>, provider: &str) -> Vec<String> {
    kv_get(state, "disabledModels", provider)
        .await
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

pub async fn all_disabled(state: &Arc<AppState>) -> std::collections::HashMap<String, Vec<String>> {
    kv_get_all(state, "disabledModels")
        .await
        .into_iter()
        .map(|(k, v)| {
            (
                k,
                serde_json::from_str::<Vec<String>>(&v).unwrap_or_default(),
            )
        })
        .collect()
}

/// Custom model ids for a provider (chat validation passthrough).
pub async fn list_custom_for(state: &Arc<AppState>, provider: &str) -> Vec<String> {
    custom_models(state, provider)
        .await
        .into_iter()
        .filter_map(|m| m.get("id").and_then(Value::as_str).map(String::from))
        .collect()
}

/// Full custom model objects for a provider.
pub async fn custom_models(state: &Arc<AppState>, provider: &str) -> Vec<Value> {
    kv_get_all(state, "customModels")
        .await
        .into_iter()
        .filter_map(|(_, v)| serde_json::from_str::<Value>(&v).ok())
        .filter(|m| {
            m.get("providerAlias").and_then(Value::as_str) == Some(provider)
                || m.get("provider").and_then(Value::as_str) == Some(provider)
        })
        .collect()
}

// ---------- alias ----------

async fn list_aliases(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Json<Value>, ApiError> {
    require_session(&state, &headers).await?;
    let aliases = all_aliases(&state).await;
    Ok(Json(json!({ "aliases": aliases })))
}

#[derive(serde::Deserialize)]
struct AliasBody {
    model: Option<String>,
    alias: Option<String>,
}

async fn set_alias(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(body): Json<AliasBody>,
) -> Result<Json<Value>, ApiError> {
    require_session(&state, &headers).await?;
    let (Some(model), Some(alias)) = (body.model, body.alias) else {
        return Err(ninty_core::error::Error::BadRequest("model and alias required".into()).into());
    };
    kv_set(&state, "modelAliases", &alias, &model).await;
    Ok(Json(json!({ "success": true, "model": model, "alias": alias })))
}

async fn delete_alias(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Query(q): Query<std::collections::HashMap<String, String>>,
) -> Result<Json<Value>, ApiError> {
    require_session(&state, &headers).await?;
    let Some(alias) = q.get("alias") else {
        return Err(ninty_core::error::Error::BadRequest("alias required".into()).into());
    };
    kv_del(&state, "modelAliases", alias).await;
    Ok(Json(json!({ "success": true })))
}

// ---------- disabled ----------

async fn list_disabled(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Query(q): Query<std::collections::HashMap<String, String>>,
) -> Result<Json<Value>, ApiError> {
    require_session(&state, &headers).await?;
    if let Some(p) = q.get("provider").or_else(|| q.get("providerAlias")) {
        let ids = disabled_ids(&state, p).await;
        return Ok(Json(json!({ "ids": ids })));
    }
    Ok(Json(json!({ "disabled": all_disabled(&state).await })))
}

#[derive(serde::Deserialize)]
struct DisableBody {
    provider: Option<String>,
    #[serde(rename = "providerAlias")]
    provider_alias: Option<String>,
    ids: Option<Vec<String>>,
}

async fn disable_models(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(body): Json<DisableBody>,
) -> Result<Json<Value>, ApiError> {
    require_session(&state, &headers).await?;
    let provider = body.provider.or(body.provider_alias);
    let (Some(provider), Some(ids)) = (provider, body.ids) else {
        return Err(ninty_core::error::Error::BadRequest("provider and ids[] required".into()).into());
    };
    let mut cur = disabled_ids(&state, &provider).await;
    for id in ids {
        if !cur.contains(&id) {
            cur.push(id);
        }
    }
    kv_set(&state, "disabledModels", &provider, &json!(cur).to_string()).await;
    Ok(Json(json!({ "success": true })))
}

async fn enable_models(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Query(q): Query<std::collections::HashMap<String, String>>,
) -> Result<Json<Value>, ApiError> {
    require_session(&state, &headers).await?;
    let provider = q.get("provider").or_else(|| q.get("providerAlias")).cloned();
    let Some(provider) = provider else {
        return Err(ninty_core::error::Error::BadRequest("provider required".into()).into());
    };
    match q.get("id") {
        Some(id) => {
            let cur: Vec<String> = disabled_ids(&state, &provider)
                .await
                .into_iter()
                .filter(|x| x != id)
                .collect();
            kv_set(&state, "disabledModels", &provider, &json!(cur).to_string()).await;
        }
        None => kv_del(&state, "disabledModels", &provider).await,
    }
    Ok(Json(json!({ "success": true })))
}

// ---------- custom ----------

async fn list_custom(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Query(q): Query<std::collections::HashMap<String, String>>,
) -> Result<Json<Value>, ApiError> {
    require_session(&state, &headers).await?;
    let all = kv_get_all(&state, "customModels").await;
    let provider_filter = q.get("provider").or_else(|| q.get("providerAlias")).cloned();
    let models: Vec<Value> = all
        .into_iter()
        .filter_map(|(_, v)| serde_json::from_str::<Value>(&v).ok())
        .filter(|m| {
            provider_filter.as_ref().is_none_or(|p| {
                m.get("providerAlias").and_then(Value::as_str) == Some(p.as_str())
                    || m.get("provider").and_then(Value::as_str) == Some(p.as_str())
            })
        })
        .collect();
    Ok(Json(json!({ "models": models })))
}

#[derive(serde::Deserialize)]
struct CustomBody {
    provider: Option<String>,
    #[serde(rename = "providerAlias")]
    provider_alias: Option<String>,
    id: Option<String>,
    name: Option<String>,
}

async fn add_custom(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(body): Json<CustomBody>,
) -> Result<Json<Value>, ApiError> {
    require_session(&state, &headers).await?;
    let provider = body.provider.or(body.provider_alias);
    let (Some(provider), Some(id)) = (provider, body.id) else {
        return Err(ninty_core::error::Error::BadRequest("provider and id required".into()).into());
    };
    let name = body.name.unwrap_or_else(|| id.clone());
    let model = json!({ "providerAlias": provider, "id": id, "name": name, "custom": true });
    kv_set(&state, "customModels", &format!("{provider}|{id}"), &model.to_string()).await;
    Ok(Json(json!({ "success": true, "model": model })))
}

async fn delete_custom(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Query(q): Query<std::collections::HashMap<String, String>>,
) -> Result<Json<Value>, ApiError> {
    require_session(&state, &headers).await?;
    let provider = q.get("provider").or_else(|| q.get("providerAlias")).cloned();
    let (Some(provider), Some(id)) = (provider, q.get("id").cloned()) else {
        return Err(ninty_core::error::Error::BadRequest("provider and id required".into()).into());
    };
    kv_del(&state, "customModels", &format!("{provider}|{id}")).await;
    Ok(Json(json!({ "success": true })))
}
