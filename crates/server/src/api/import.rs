//! POST /api/import/9router — import a 9router database export
//! (GET /api/settings/database on a 9router instance). Merge semantics:
//! INSERT OR REPLACE by id, existing ninty data never wiped.

use std::sync::Arc;

use axum::extract::State;
use axum::http::HeaderMap;
use axum::routing::post;
use axum::{Json, Router};
use serde_json::{Map, Value};

use crate::api::{require_session, ApiError};
use crate::state::AppState;
use ninty_core::error::Error;

pub fn router() -> Router<Arc<AppState>> {
    Router::new().route("/9router", post(import_9router))
}

type ApiKeyRow = (String, String, Option<String>, Option<String>, i64, String);
type ComboRow = (String, String, Option<String>, String, String, String);

#[derive(Debug, Default, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct ImportReport {
    connections: usize,
    api_keys: usize,
    combos: usize,
    settings_applied: Vec<String>,
    skipped: Vec<String>,
}

/// 9router settings key → ninty settings key (camelCase → snake_case).
const SETTINGS_MAP: &[(&str, &str)] = &[
    ("rtkEnabled", "rtk_enabled"),
    ("cavemanEnabled", "caveman_enabled"),
    ("ponytailEnabled", "ponytail_enabled"),
    ("ponytailLevel", "ponytail_level"),
    ("pxpipeEnabled", "pxpipe_enabled"),
    ("pxpipeMinChars", "pxpipe_min_chars"),
    ("pxpipeTimeoutMs", "pxpipe_timeout_ms"),
    ("requireApiKey", "require_api_key"),
    ("requireLogin", "require_login"),
    ("enableRequestLogs", "enable_request_logs"),
    ("comboStrategy", "combo_strategy"),
    ("stickyRoundRobinLimit", "sticky_round_robin_limit"),
];

fn now() -> String {
    chrono::Utc::now().to_rfc3339()
}

fn as_str(v: Option<&Value>) -> Option<String> {
    v.and_then(|v| v.as_str()).map(String::from)
}

fn bool_or(v: Option<&Value>, default: bool) -> bool {
    v.and_then(Value::as_bool).unwrap_or(default)
}

async fn import_9router(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(payload): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    require_session(&state, &headers).await?;
    if !payload.is_object() {
        return Err(ApiError(Error::BadRequest("invalid export payload".into())));
    }
    let mut report = ImportReport::default();

    // --- connections ---
    if let Some(conns) = payload.get("providerConnections").and_then(Value::as_array) {
        let rows: Vec<Vec<Value>> = conns
            .iter()
            .map(|c| {
                let mut data = c.clone();
                if let Some(obj) = data.as_object_mut() {
                    for k in [
                        "id", "provider", "authType", "auth_type", "name", "email", "priority",
                        "isActive", "is_active", "createdAt", "updatedAt", "created_at",
                        "updated_at",
                    ] {
                        obj.remove(k);
                    }
                }
                vec![
                    Value::from(as_str(c.get("id")).unwrap_or_else(|| uuid::Uuid::new_v4().to_string())),
                    Value::from(as_str(c.get("provider")).unwrap_or_default()),
                    Value::from(as_str(c.get("authType").or_else(|| c.get("auth_type")))
                        .unwrap_or_else(|| {
                            if data.get("apiKey").is_some() { "apikey" } else { "oauth" }.to_string()
                        })),
                    Value::from(as_str(c.get("name")).unwrap_or_default()),
                    Value::from(as_str(c.get("email")).unwrap_or_default()),
                    Value::from(c.get("priority").and_then(Value::as_i64).unwrap_or(0)),
                    Value::from(bool_or(c.get("isActive").or_else(|| c.get("is_active")), true)),
                    data,
                    Value::from(
                        as_str(c.get("createdAt").or_else(|| c.get("created_at")))
                            .unwrap_or_else(now),
                    ),
                    Value::from(
                        as_str(c.get("updatedAt").or_else(|| c.get("updated_at")))
                            .unwrap_or_else(now),
                    ),
                ]
            })
            .filter(|r| !r[1].as_str().unwrap_or_default().is_empty())
            .collect();
        let n = rows.len();
        state
            .db
            .call(move |conn| {
                let dbe = |e: rusqlite::Error| Error::Db(e.to_string());
                let tx = conn.unchecked_transaction().map_err(dbe)?;
                {
                    let mut stmt = tx.prepare(
                        "INSERT OR REPLACE INTO provider_connections
                         (id, provider, auth_type, name, email, priority, is_active, data, created_at, updated_at)
                         VALUES (?1, ?2, ?3, NULLIF(?4, ''), NULLIF(?5, ''), ?6, ?7, ?8, ?9, ?10)",
                    ).map_err(dbe)?;
                    for r in &rows {
                        stmt.execute(rusqlite::params![
                            r[0].as_str().unwrap_or_default(),
                            r[1].as_str().unwrap_or_default(),
                            r[2].as_str().unwrap_or_default(),
                            r[3].as_str().unwrap_or_default(),
                            r[4].as_str().unwrap_or_default(),
                            r[5].as_i64().unwrap_or(0),
                            r[6].as_bool().unwrap_or(true) as i64,
                            r[7].to_string(),
                            r[8].as_str().unwrap_or_default(),
                            r[9].as_str().unwrap_or_default(),
                        ]).map_err(dbe)?;
                    }
                }
                tx.commit().map_err(dbe)?;
                Ok(())
            })
            .await
            .map_err(|e| ApiError(Error::Db(e.to_string())))?;
        report.connections = n;
    }

    // --- api keys ---
    if let Some(keys) = payload.get("apiKeys").and_then(Value::as_array) {
        let rows: Vec<ApiKeyRow> = keys
            .iter()
            .filter_map(|k| {
                let key = as_str(k.get("key"))?;
                Some((
                    as_str(k.get("id")).unwrap_or_else(|| uuid::Uuid::new_v4().to_string()),
                    key,
                    as_str(k.get("name")),
                    as_str(k.get("machineId").or_else(|| k.get("machine_id"))),
                    bool_or(k.get("isActive").or_else(|| k.get("is_active")), true) as i64,
                    as_str(k.get("createdAt").or_else(|| k.get("created_at"))).unwrap_or_else(now),
                ))
            })
            .collect();
        let n = rows.len();
        state
            .db
            .call(move |conn| {
                let dbe = |e: rusqlite::Error| Error::Db(e.to_string());
                let tx = conn.unchecked_transaction().map_err(dbe)?;
                {
                    let mut stmt = tx.prepare(
                        "INSERT OR REPLACE INTO api_keys (id, key, name, machine_id, is_active, created_at)
                         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                    ).map_err(dbe)?;
                    for (id, key, name, machine_id, active, created) in &rows {
                        stmt.execute(rusqlite::params![id, key, name, machine_id, active, created]).map_err(dbe)?;
                    }
                }
                tx.commit().map_err(dbe)?;
                Ok(())
            })
            .await
            .map_err(|e| ApiError(Error::Db(e.to_string())))?;
        report.api_keys = n;
    }

    // --- combos ---
    if let Some(combos) = payload.get("combos").and_then(Value::as_array) {
        let rows: Vec<ComboRow> = combos
            .iter()
            .filter_map(|c| {
                let name = as_str(c.get("name"))?;
                Some((
                    as_str(c.get("id")).unwrap_or_else(|| uuid::Uuid::new_v4().to_string()),
                    name,
                    as_str(c.get("kind")),
                    c.get("models")
                        .map(|m| m.to_string())
                        .unwrap_or_else(|| "[]".to_string()),
                    as_str(c.get("createdAt").or_else(|| c.get("created_at"))).unwrap_or_else(now),
                    as_str(c.get("updatedAt").or_else(|| c.get("updated_at"))).unwrap_or_else(now),
                ))
            })
            .collect();
        let n = rows.len();
        state
            .db
            .call(move |conn| {
                let dbe = |e: rusqlite::Error| Error::Db(e.to_string());
                let tx = conn.unchecked_transaction().map_err(dbe)?;
                {
                    let mut stmt = tx.prepare(
                        "INSERT OR REPLACE INTO combos (id, name, kind, models, created_at, updated_at)
                         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                    ).map_err(dbe)?;
                    for (id, name, kind, models, created, updated) in &rows {
                        stmt.execute(rusqlite::params![id, name, kind, models, created, updated]).map_err(dbe)?;
                    }
                }
                tx.commit().map_err(dbe)?;
                Ok(())
            })
            .await
            .map_err(|e| ApiError(Error::Db(e.to_string())))?;
        report.combos = n;
    }

    // --- settings (mapped keys only; password never imported) ---
    if let Some(settings) = payload.get("settings").and_then(Value::as_object) {
        let mut patch = Map::new();
        for (from, to) in SETTINGS_MAP {
            if let Some(v) = settings.get(*from) {
                patch.insert((*to).to_string(), v.clone());
                report.settings_applied.push((*to).to_string());
            }
        }
        if !patch.is_empty() {
            let updated =
                crate::repos::settings::patch(&state.db, Value::Object(patch)).await?;
            state.set_request_logs(updated.enable_request_logs);
        }
    }

    // --- skipped sections (no ninty equivalent) ---
    for (key, label) in [
        ("providerNodes", "providerNodes"),
        ("proxyPools", "proxyPools"),
        ("modelAliases", "modelAliases"),
        ("customModels", "customModels"),
        ("mitmAlias", "mitmAlias"),
        ("pricing", "pricing"),
    ] {
        if let Some(v) = payload.get(key) {
            let n = v
                .as_array()
                .map(Vec::len)
                .or_else(|| v.as_object().map(Map::len))
                .unwrap_or(0);
            if n > 0 {
                report.skipped.push(format!("{label} ({n})"));
            }
        }
    }

    Ok(Json(serde_json::to_value(report)?))
}
