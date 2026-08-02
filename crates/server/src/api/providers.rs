//! /api/providers — registry providers + connections CRUD + test.

use std::sync::Arc;

use axum::extract::{Path, State};
use axum::http::HeaderMap;
use axum::routing::{get, post};
use axum::{Json, Router};
use ninty_core::registry;
use serde_json::json;

use crate::api::{require_session, ApiError};
use crate::repos::connections::{self, ConnectionPatch, NewConnection};
use crate::repos::nodes::{self, NewNode};
use crate::state::AppState;

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/", get(list_providers).post(create_connection))
        .route(
            "/{id}",
            axum::routing::put(update_connection).delete(delete_connection),
        )
        .route("/{id}/test", post(test_connection))
        .route("/nodes", get(list_nodes).post(create_node))
        .route("/nodes/{id}", axum::routing::delete(delete_node))
}

async fn list_providers(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, ApiError> {
    require_session(&state, &headers).await?;
    let conns = connections::list(&state.db, None).await?;
    let nodes = nodes::list(&state.db).await?;

    let mut providers: Vec<serde_json::Value> = Vec::new();
    for p in registry::all_providers() {
        let pc: Vec<serde_json::Value> = conns
            .iter()
            .filter(|c| c.provider == p.id)
            .map(|c| serde_json::to_value(c.sanitized()).unwrap_or(json!({})))
            .collect();
        let mut models: Vec<serde_json::Value> = p
            .models
            .iter()
            .map(|m| json!({"id": m.id, "name": m.name}))
            .collect();
        // Preloaded upstream lists (opencode/openrouter): fill empty registries,
        // append extras after static models (deduped by id).
        if registry::models_fetcher(p.id).is_some() {
            let fetched = crate::models_preload::cached(&state, p.id).await;
            for fm in fetched {
                if models.iter().any(|m| m["id"] == fm.id) {
                    continue;
                }
                models.push(json!({"id": fm.id, "name": fm.name, "suggested": true}));
            }
        }
        providers.push(json!({
            "id": p.id,
            "alias": p.alias,
            "category": match p.category {
                registry::Category::ApiKey => "apikey",
                registry::Category::OAuth => "oauth",
                registry::Category::Free => "free",
            },
            "display_name": p.display_name,
            "notice_url": p.notice_url,
            "color": p.color,
            "text_icon": p.text_icon,
            "no_auth": p.no_auth,
            "models": models,
            "connections": pc,
        }));
    }

    let nodes_json: Vec<serde_json::Value> = nodes
        .iter()
        .map(|n| serde_json::to_value(n.sanitized()).unwrap_or(json!({})))
        .collect();

    Ok(Json(json!({"providers": providers, "nodes": nodes_json})))
}

async fn create_connection(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(body): Json<NewConnection>,
) -> Result<Json<serde_json::Value>, ApiError> {
    require_session(&state, &headers).await?;
    if registry::find_provider(&body.provider).is_none() {
        return Err(ninty_core::error::Error::BadRequest(format!(
            "unknown provider '{}'",
            body.provider
        ))
        .into());
    }
    let needs_key = registry::find_provider(&body.provider)
        .map(|p| !p.no_auth && p.category != ninty_core::registry::Category::OAuth)
        .unwrap_or(true);
    if needs_key
        && body
            .api_key
            .as_deref()
            .map(|s| s.is_empty())
            .unwrap_or(true)
    {
        return Err(ninty_core::error::Error::BadRequest("api_key required".into()).into());
    }
    let conn = connections::create(&state.db, body).await?;
    Ok(Json(json!({"connection": conn.sanitized()})))
}

async fn update_connection(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(patch): Json<ConnectionPatch>,
) -> Result<Json<serde_json::Value>, ApiError> {
    require_session(&state, &headers).await?;
    connections::update(&state.db, &id, patch).await?;
    Ok(Json(json!({"ok": true})))
}

async fn delete_connection(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    require_session(&state, &headers).await?;
    connections::delete(&state.db, &id).await?;
    Ok(Json(json!({"ok": true})))
}

/// Test: minimal upstream chat request; stores testStatus in connection data.
async fn test_connection(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    require_session(&state, &headers).await?;
    let conn = connections::get(&state.db, &id)
        .await?
        .ok_or_else(|| ninty_core::error::Error::NotFound("connection".into()))?;
    let provider = registry::find_provider(&conn.provider)
        .ok_or_else(|| ninty_core::error::Error::NotFound("provider".into()))?;
    if provider.id == "qoder" {
        return Ok(Json(json!({"ok": false, "message": "test not supported for qoder (COSY signing)"})));
    }
    // Registry model first; preloaded upstream list for dynamic providers
    // (opencode has no static models); clear error when nothing to probe with.
    let model = match provider.models.first().map(|m| m.id.to_string()) {
        Some(m) => m,
        None => {
            let fetched = crate::models_preload::cached(&state, provider.id).await;
            match fetched.first() {
                Some(m) => m.id.clone(),
                None => {
                    return Ok(Json(
                        json!({"ok": false, "message": "no models available to test (preload pending or upstream unreachable)"}),
                    ));
                }
            }
        }
    };
    let spec = format!("{}/{model}", provider.id);

    // Same target + URL + auth pipeline the chat path uses — transport-aware
    // (x-api-key / query key / vertex SA / cline workos / oauth refresh).
    let targets = crate::v1::chat::resolve_targets(&state, &spec, registry::WireFormat::Openai)
        .await
        .map_err(ApiError::from)?;
    let Some(target) = targets.iter().find(|t| t.conn_id.as_deref() == Some(id.as_str())) else {
        return Ok(Json(json!({"ok": false, "message": "connection not eligible (disabled, locked, or missing credentials)"})));
    };
    let (url, headers) = crate::v1::chat::build_url_and_auth(&state, target, false)
        .await
        .map_err(ApiError::from)?;

    let body = match target.format {
        registry::WireFormat::Claude => json!({
            "model": target.model,
            "max_tokens": 1,
            "messages": [{"role": "user", "content": "ping"}],
        }),
        registry::WireFormat::Gemini => json!({
            "contents": [{"role": "user", "parts": [{"text": "ping"}]}],
        }),
        registry::WireFormat::Responses => json!({
            "model": target.model,
            "input": "ping",
        }),
        registry::WireFormat::Openai => json!({
            "model": target.model,
            "messages": [{"role": "user", "content": "ping"}],
            "max_tokens": 1,
            "stream": false,
        }),
    };

    let mut req = state
        .http
        .post(&url)
        .timeout(std::time::Duration::from_secs(30))
        .header("content-type", "application/json");
    for (k, v) in &headers {
        req = req.header(k, v);
    }
    let (status, message) = match req.json(&body).send().await {
        Ok(resp) => {
            let s = resp.status();
            if s.is_success() {
                ("ok".to_string(), "ok".to_string())
            } else {
                let text = resp.text().await.unwrap_or_default();
                (s.as_u16().to_string(), text.chars().take(300).collect())
            }
        }
        Err(e) => ("error".to_string(), e.to_string()),
    };
    let ok = status == "ok";
    connections::update(
        &state.db,
        &id,
        ConnectionPatch {
            data: Some(json!({
                "testStatus": if ok { "ok" } else { "error" },
                "lastError": if ok { serde_json::Value::Null } else { json!(message) },
                "lastErrorAt": chrono::Utc::now().to_rfc3339(),
            })),
            ..Default::default()
        },
    )
    .await?;

    Ok(Json(json!({ "ok": ok, "status": status, "message": message })))
}

async fn list_nodes(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, ApiError> {
    require_session(&state, &headers).await?;
    let nodes = nodes::list(&state.db).await?;
    let out: Vec<_> = nodes.iter().map(|n| n.sanitized()).collect();
    Ok(Json(json!({"nodes": out})))
}

async fn create_node(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(body): Json<NewNode>,
) -> Result<Json<serde_json::Value>, ApiError> {
    require_session(&state, &headers).await?;
    let node = nodes::create(&state.db, body).await?;
    Ok(Json(json!({"node": node.sanitized()})))
}

async fn delete_node(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    require_session(&state, &headers).await?;
    nodes::delete(&state.db, &id).await?;
    Ok(Json(json!({"ok": true})))
}
