//! /api/providers — registry providers + connections CRUD + test.

use std::sync::Arc;

use axum::extract::{Path, State};
use axum::http::HeaderMap;
use axum::routing::{get, post};
use axum::{Json, Router};
use ninty_core::registry;
use serde_json::json;

use crate::api::{require_session, ApiError};
use crate::repos::connections::{self, Connection, ConnectionPatch, NewConnection};
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
        .route("/export/{provider}", get(export_connections))
        .route("/import/{provider}", post(import_connections))
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
        let disabled = crate::api::models_admin::disabled_ids(&state, p.id).await;
        let mut models: Vec<serde_json::Value> = p
            .models
            .iter()
            .map(|m| {
                let caps = ninty_core::capabilities::capabilities(p.id, m.id);
                json!({
                    "id": m.id,
                    "name": m.name,
                    "caps": { "vision": caps.vision, "reasoning": caps.reasoning },
                    "disabled": disabled.iter().any(|d| d == m.id),
                })
            })
            .collect();
        // Preloaded upstream lists (opencode/openrouter): fill empty registries,
        // append extras after static models (deduped by id).
        if registry::models_fetcher(p.id).is_some() {
            let fetched = crate::models_preload::cached(&state, p.id).await;
            for fm in fetched {
                if models.iter().any(|m| m["id"] == fm.id) {
                    continue;
                }
                let caps = ninty_core::capabilities::capabilities(p.id, &fm.id);
                models.push(json!({
                    "id": fm.id,
                    "name": fm.name,
                    "suggested": true,
                    "caps": { "vision": caps.vision, "reasoning": caps.reasoning },
                    "disabled": disabled.contains(&fm.id),
                }));
            }
        }
        // Custom user-added models first (9router orders them first).
        let custom = crate::api::models_admin::custom_models(&state, p.id).await;
        for cm in custom.into_iter().rev() {
            let caps = ninty_core::capabilities::capabilities(
                p.id,
                cm.get("id").and_then(|v| v.as_str()).unwrap_or(""),
            );
            let id = cm.get("id").cloned().unwrap_or(json!(""));
            models.insert(
                0,
                json!({
                    "id": id,
                    "name": cm.get("name").cloned().unwrap_or(id),
                    "custom": true,
                    "caps": { "vision": caps.vision, "reasoning": caps.reasoning },
                    "disabled": disabled.iter().any(|d| Some(d.as_str()) == cm.get("id").and_then(|v| v.as_str())),
                }),
            );
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

/// Export connections of provider as JSON (9router /api/providers/[id]/export).
/// Includes api_key (full secrets, like 9router export file).
async fn export_connections(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(provider): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    require_session(&state, &headers).await?;
    let conns = connections::list(&state.db, Some(&provider)).await?;
    let out: Vec<serde_json::Value> = conns
        .iter()
        .map(|c| {
            json!({
                "name": c.name,
                "priority": c.priority,
                "apiKey": c.api_key(),
                "providerSpecificData": c.data,
            })
        })
        .collect();
    Ok(Json(json!({"connections": out})))
}

/// Import connections JSON (9router /api/providers/[id]/import).
/// Accepts {"connections": [...]} — name, priority, apiKey, providerSpecificData.
/// Skips entries with no apiKey for key-based providers. Returns created count.
async fn import_connections(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(provider): Path<String>,
    Json(body): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, ApiError> {
    require_session(&state, &headers).await?;
    if registry::find_provider(&provider).is_none() {
        return Err(ninty_core::error::Error::BadRequest(format!(
            "unknown provider '{provider}'"
        ))
        .into());
    }
    let items = body
        .get("connections")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    if items.is_empty() {
        return Err(ninty_core::error::Error::BadRequest("no connections in file".into()).into());
    }
    let existing = connections::list(&state.db, Some(&provider)).await?;
    let mut created = 0usize;
    for (i, item) in items.iter().enumerate() {
        let api_key = item
            .get("apiKey")
            .or_else(|| item.get("api_key"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        // Skip dupes: same api_key already on this provider.
        if !api_key.is_empty()
            && existing
                .iter()
                .any(|c| c.api_key() == Some(api_key.as_str()))
        {
            continue;
        }
        let priority = item
            .get("priority")
            .and_then(|v| v.as_i64())
            .unwrap_or((existing.len() + i + 1) as i64);
        let data = item
            .get("providerSpecificData")
            .or_else(|| item.get("data"))
            .cloned()
            .unwrap_or(json!({}));
        let name = item
            .get("name")
            .and_then(|v| v.as_str())
            .map(String::from)
            .unwrap_or_else(|| format!("{} #{}", provider, existing.len() + created + 1));
        connections::create(
            &state.db,
            NewConnection {
                provider: provider.clone(),
                name: Some(name),
                priority: Some(priority),
                api_key: if api_key.is_empty() { None } else { Some(api_key) },
                data: Some(data),
            },
        )
        .await?;
        created += 1;
    }
    Ok(Json(json!({"ok": true, "created": created})))
}


/// Test: per-provider probe matrix (9router testUtils.js 1:1). Stores
/// testStatus "active"/"error" + lastError in connection data.
async fn test_connection(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    require_session(&state, &headers).await?;
    let conn = connections::get(&state.db, &id)
        .await?
        .ok_or_else(|| ninty_core::error::Error::NotFound("connection".into()))?;
    // Proactive refresh for oauth connections near expiry (9router refreshes before probe).
    let conn = crate::oauth_state::ensure_fresh(&state, &conn)
        .await
        .unwrap_or(conn);

    let mut result = probe(&state, &conn).await;

    // 401/403 + refreshToken → refresh once, retry probe once.
    if matches!(result.status, Some(401) | Some(403))
        && conn.data.get("refreshToken").and_then(|v| v.as_str()).is_some()
    {
        if let Ok(fresh) = crate::oauth_state::refresh_now(&state, &conn).await {
            result = probe(&state, &fresh).await;
        }
    }

    connections::update(
        &state.db,
        &id,
        ConnectionPatch {
            data: Some(json!({
                "testStatus": if result.ok { "active" } else { "error" },
                "lastError": if result.ok { serde_json::Value::Null } else { json!(result.message) },
                "lastErrorAt": chrono::Utc::now().to_rfc3339(),
            })),
            ..Default::default()
        },
    )
    .await?;

    Ok(Json(json!({
        "ok": result.ok,
        "status": result.status,
        "message": result.message,
    })))
}

struct ProbeResult {
    ok: bool,
    status: Option<u16>,
    message: String,
}

impl ProbeResult {
    fn ok() -> Self {
        Self { ok: true, status: None, message: "ok".into() }
    }
    fn fail(status: Option<u16>, message: impl Into<String>) -> Self {
        Self { ok: false, status, message: message.into() }
    }
}

/// Send a probe request; returns (status, body preview).
async fn send_probe(
    client: &reqwest::Client,
    method: &str,
    url: &str,
    headers: &[(&str, String)],
    body: Option<serde_json::Value>,
) -> Result<(u16, String), String> {
    let mut req = match method {
        "GET" => client.get(url),
        _ => client.post(url),
    }
    .timeout(std::time::Duration::from_secs(30));
    for (k, v) in headers {
        req = req.header(*k, v);
    }
    if let Some(b) = body {
        req = req.json(&b);
    }
    let resp = req.send().await.map_err(|e| e.to_string())?;
    let status = resp.status().as_u16();
    let text = resp.text().await.unwrap_or_default();
    Ok((status, text.chars().take(300).collect()))
}

fn bearer(token: &str) -> Vec<(&'static str, String)> {
    vec![("authorization", format!("Bearer {token}"))]
}

/// Per-provider probe (9router testUtils.js). OAuth cred = accessToken,
/// apikey cred = apiKey; both fall back across each other where 9router does.
async fn probe(state: &Arc<AppState>, conn: &Connection) -> ProbeResult {
    let token = conn
        .data
        .get("accessToken")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let key = conn.api_key().unwrap_or("");
    let cred = if !token.is_empty() { token } else { key };
    let http = &state.http;
    let ping_msg = || {
        json!({"model": "ping", "max_tokens": 1, "messages": [{"role": "user", "content": "hi"}]})
    };

    match conn.provider.as_str() {
        // --- tokenExists: no network (9router: cursor, codebuddy-cn, opencode) ---
        "codebuddy-cn" | "opencode" => {
            if cred.is_empty() {
                ProbeResult::fail(None, "no credential")
            } else {
                ProbeResult::ok()
            }
        }

        // --- checkExpiry only (no probe): claude ---
        "claude" => {
            let expires_at = conn.data.get("expiresAt").and_then(|v| v.as_i64()).unwrap_or(0);
            if token.is_empty() {
                ProbeResult::fail(None, "No access token")
            } else if expires_at > 0 && expires_at <= chrono::Utc::now().timestamp_millis() {
                ProbeResult::fail(None, "Token expired")
            } else {
                ProbeResult::ok()
            }
        }

        // --- codex: POST responses; 400 = auth OK, only 401/403 fail ---
        "codex" => {
            let headers = vec![
                ("authorization", format!("Bearer {cred}")),
                ("content-type", "application/json".into()),
                ("originator", "codex_cli_rs".into()),
                ("user-agent", "codex_cli_rs/0.136.0".into()),
            ];
            let body = json!({"model": "gpt-5.3-codex", "input": [], "stream": false, "store": false});
            match send_probe(http, "POST", "https://chatgpt.com/backend-api/codex/responses", &headers, Some(body)).await {
                Err(e) => ProbeResult::fail(None, e),
                Ok((s, text)) => {
                    if s == 401 || s == 403 {
                        ProbeResult::fail(Some(s), "Token invalid or revoked")
                    } else {
                        ProbeResult::ok()
                    }
                    .with_body(s, text)
                }
            }
        }

        // --- github: GET /user ---
        "github" => {
            let headers = vec![
                ("authorization", format!("Bearer {cred}")),
                ("user-agent", "9Router".into()),
                ("accept", "application/vnd.github+json".into()),
            ];
            match send_probe(http, "GET", "https://api.github.com/user", &headers, None).await {
                Err(e) => ProbeResult::fail(None, e),
                Ok((s, text)) => status_probe(s, text, &[401, 403]),
            }
        }

        // --- cline: GET users/me with workos bearer + client headers ---
        "cline" => {
            let workos = if cred.starts_with("workos:") { cred.to_string() } else { format!("workos:{cred}") };
            let headers = vec![
                ("authorization", format!("Bearer {workos}")),
                ("http-referer", "https://cline.bot".into()),
                ("x-title", "Cline".into()),
                ("user-agent", format!("ninty-router/{}", env!("CARGO_PKG_VERSION"))),
                ("x-client-type", "ninty-router".into()),
                ("accept", "application/json".into()),
            ];
            match send_probe(http, "GET", "https://api.cline.bot/api/v1/users/me", &headers, None).await {
                Err(e) => ProbeResult::fail(None, e),
                Ok((s, text)) => status_probe(s, text, &[401, 403]),
            }
        }

        // --- qoder: oauth → GET userinfo; apikey → POST jobToken/exchange ---
        "qoder" => {
            if !token.is_empty() {
                match send_probe(http, "GET", "https://openapi.qoder.sh/api/v1/userinfo", &bearer(token), None).await {
                    Err(e) => ProbeResult::fail(None, e),
                    Ok((s, text)) => status_probe(s, text, &[401, 403]),
                }
            } else if !key.is_empty() {
                let pt = if key.starts_with("pt-") { key.to_string() } else { format!("pt-{key}") };
                let headers = vec![
                    ("content-type", "application/json".into()),
                    ("accept", "application/json".into()),
                    ("cosy-version", "1.0.1".into()),
                    ("cosy-clienttype", "5".into()),
                ];
                let body = json!({"personal_token": pt});
                match send_probe(http, "POST", "https://openapi.qoder.sh/api/v1/jobToken/exchange", &headers, Some(body)).await {
                    Err(e) => ProbeResult::fail(None, e),
                    Ok((s, text)) => {
                        if s == 200 { ProbeResult::ok() } else { ProbeResult::fail(Some(s), format!("Invalid Personal Access Token: {}", &text[..text.len().min(120)])) }
                    }
                }
            } else {
                ProbeResult::fail(None, "no credential")
            }
        }

        // --- codebuddy-intl: POST chat; any status != 401 = valid ---
        "codebuddy-intl" => {
            let headers = vec![
                ("authorization", format!("Bearer {cred}")),
                ("content-type", "application/json".into()),
                ("accept", "application/json".into()),
                ("user-agent", "CLI/2.52.0 CodeBuddy/2.52.0".into()),
                ("x-product", "SaaS".into()),
                ("x-ide-type", "CLI".into()),
                ("x-ide-name", "CLI".into()),
                ("x-ide-version", "2.52.0".into()),
                ("x-agent-intent", "craft".into()),
                ("x-domain", "www.codebuddy.ai".into()),
                ("x-requested-with", "XMLHttpRequest".into()),
            ];
            let body = json!({"model": "gemini-2.5-flash", "messages": [{"role": "user", "content": "hi"}], "max_tokens": 1, "stream": false});
            match send_probe(http, "POST", "https://www.codebuddy.ai/v2/chat/completions", &headers, Some(body)).await {
                Err(e) => ProbeResult::fail(None, e),
                Ok((s, text)) => {
                    if s == 401 { ProbeResult::fail(Some(s), "Invalid API key") } else { ProbeResult::ok() }.with_body(s, text)
                }
            }
        }

        // --- GET /models probes ---
        "deepseek" => get_models_probe(http, "https://api.deepseek.com/models", cred).await,
        "groq" => get_models_probe(http, "https://api.groq.com/openai/v1/models", cred).await,
        "mistral" => get_models_probe(http, "https://api.mistral.ai/v1/models", cred).await,
        "xai" => get_models_probe(http, "https://api.x.ai/v1/models", cred).await,
        "together" => get_models_probe(http, "https://api.together.xyz/v1/models", cred).await,
        "blackbox" => get_models_probe(http, "https://api.blackbox.ai/v1/models", cred).await,
        "openrouter" => get_models_probe(http, "https://openrouter.ai/api/v1/auth/key", cred).await,

        // --- gemini: key in query ---
        "gemini" => {
            match send_probe(http, "GET", &format!("https://generativelanguage.googleapis.com/v1/models?key={cred}"), &[], None).await {
                Err(e) => ProbeResult::fail(None, e),
                Ok((s, text)) => status_probe(s, text, &[401, 403]),
            }
        }

        // --- POST /messages probes (valid = !401 && !403) ---
        "anthropic" | "glm" | "minimax" | "minimax-cn" | "kimi" => {
            let url = match conn.provider.as_str() {
                "anthropic" => "https://api.anthropic.com/v1/messages",
                "glm" => "https://api.z.ai/api/anthropic/v1/messages",
                "minimax" => "https://api.minimax.io/anthropic/v1/messages",
                "minimax-cn" => "https://api.minimaxi.com/anthropic/v1/messages",
                _ => "https://api.kimi.com/coding/v1/messages",
            };
            let headers = vec![
                ("x-api-key", cred.to_string()),
                ("anthropic-version", "2023-06-01".into()),
                ("content-type", "application/json".into()),
                ("authorization", format!("Bearer {cred}")),
            ];
            match send_probe(http, "POST", url, &headers, Some(ping_msg())).await {
                Err(e) => ProbeResult::fail(None, e),
                Ok((s, text)) => {
                    if s == 401 || s == 403 { ProbeResult::fail(Some(s), text) } else { ProbeResult::ok() }
                }
            }
        }

        "glm-cn" => {
            let headers = vec![
                ("authorization", format!("Bearer {cred}")),
                ("content-type", "application/json".into()),
            ];
            let body = json!({"model": "glm-4.7", "max_tokens": 1, "messages": [{"role": "user", "content": "hi"}]});
            match send_probe(http, "POST", "https://open.bigmodel.cn/api/coding/paas/v4/chat/completions", &headers, Some(body)).await {
                Err(e) => ProbeResult::fail(None, e),
                Ok((s, text)) => {
                    if s == 401 || s == 403 { ProbeResult::fail(Some(s), text) } else { ProbeResult::ok() }
                }
            }
        }

        _ => ProbeResult::fail(None, "Provider test not supported"),
    }
}

impl ProbeResult {
    /// Attach status/body context to the message on failure.
    fn with_body(self, status: u16, body: String) -> Self {
        if self.ok {
            return self;
        }
        let preview = &body[..body.len().min(200)];
        ProbeResult {
            ok: false,
            status: Some(status),
            message: if preview.is_empty() { self.message } else { format!("{}: {preview}", self.message) },
        }
    }
}

async fn get_models_probe(client: &reqwest::Client, url: &str, cred: &str) -> ProbeResult {
    match send_probe(client, "GET", url, &bearer(cred), None).await {
        Err(e) => ProbeResult::fail(None, e),
        Ok((s, text)) => status_probe(s, text, &[401, 403]),
    }
}

/// ok unless status is one of the failure statuses.
fn status_probe(status: u16, body: String, fail_on: &[u16]) -> ProbeResult {
    if fail_on.contains(&status) {
        let preview = &body[..body.len().min(200)];
        ProbeResult::fail(Some(status), preview.to_string())
    } else {
        ProbeResult::ok()
    }
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
