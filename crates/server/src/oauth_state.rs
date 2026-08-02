//! Per-request OAuth freshness: proactive refresh (lead) + reactive (401).
//! Tokens live in provider_connections.data {accessToken, refreshToken, expiresAt(ms), ...}.

use std::sync::Arc;

use chrono::Utc;
use ninty_core::error::{Error, Result};
use serde_json::json;

use crate::repos::connections::{self, Connection, ConnectionPatch};
use crate::state::AppState;
use engine::oauth::refresh as rf;

/// Refresh tokens when expired/near-lead. Returns possibly-updated connection.
pub async fn ensure_fresh(state: &Arc<AppState>, conn: &Connection) -> Result<Connection> {
    let Some(lead) = lead_ms(&conn.provider) else {
        return Ok(conn.clone());
    };
    let expires_at = conn.data.get("expiresAt").and_then(|v| v.as_i64()).unwrap_or(0);
    if expires_at == 0 || !rf::should_refresh(expires_at, lead) {
        return Ok(conn.clone());
    }
    refresh_now(state, conn).await
}

/// Force refresh (used after upstream 401/403).
pub async fn refresh_now(state: &Arc<AppState>, conn: &Connection) -> Result<Connection> {
    let refresh_token = conn
        .data
        .get("refreshToken")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    let refreshed = match conn.provider.as_str() {
        "claude" => {
            if refresh_token.is_empty() {
                return Err(Error::BadRequest("claude connection missing refreshToken".into()));
            }
            rf::refresh_claude(&state.http, &refresh_token).await?
        }
        "codex" => {
            if refresh_token.is_empty() {
                return Err(Error::BadRequest("codex connection missing refreshToken".into()));
            }
            let refreshed_at = conn.data.get("refreshedAt").and_then(|v| v.as_i64());
            if rf::codex_must_reauth(refreshed_at) {
                return Err(Error::BadRequest("codex refresh token stale (>8d) — re-auth required".into()));
            }
            rf::refresh_codex(&state.http, &refresh_token).await?
        }
        "github" => {
            // github oauth token is long-lived; re-mint the copilot token
            let gh = conn
                .data
                .get("accessToken")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            if gh.is_empty() {
                return Err(Error::BadRequest("github connection missing accessToken".into()));
            }
            let r = rf::mint_copilot_token(&state.http, &gh).await?;
            // copilot token stored separately, accessToken (github) untouched
            let mut data = conn.data.clone();
            data["copilotToken"] = json!(r.access_token);
            if let Some(exp) = r.extra.get("copilotTokenExpiresAt").and_then(|v| v.as_str()) {
                data["copilotTokenExpiresAt"] = json!(exp);
            }
            return persist(state, conn, data).await;
        }
        "kiro" => {
            let d = &conn.data;
            let (cid, cs, endpoint) = (
                d.get("clientId").and_then(|v| v.as_str()).unwrap_or(""),
                d.get("clientSecret").and_then(|v| v.as_str()).unwrap_or(""),
                d.get("ssoOidcEndpoint").and_then(|v| v.as_str()).unwrap_or(rf::KIRO_OIDC),
            );
            rf::refresh_kiro(&state.http, endpoint, cid, cs, &refresh_token).await?
        }
        "cline" => {
            if refresh_token.is_empty() {
                return Err(Error::BadRequest("cline connection missing refreshToken".into()));
            }
            rf::refresh_cline(&state.http, &refresh_token).await?
        }
        "codebuddy-cn" => rf::refresh_codebuddy(&state.http, &rf::CODEBUDDY_CN, &refresh_token).await?,
        "codebuddy-intl" => rf::refresh_codebuddy(&state.http, &rf::CODEBUDDY_INTL, &refresh_token).await?,
        _ => return Ok(conn.clone()),
    };

    if refreshed.access_token.is_empty() {
        return Err(Error::Upstream { status: 401, message: "refresh returned no access token".into() });
    }

    let mut data = conn.data.clone();
    data["accessToken"] = json!(refreshed.access_token);
    if let Some(rt) = &refreshed.refresh_token {
        data["refreshToken"] = json!(rt);
    }
    if let Some(id) = &refreshed.id_token {
        data["idToken"] = json!(id);
        if conn.provider == "codex" {
            let (acc, plan) = rf::codex_parse_id_token(id);
            if let Some(a) = acc {
                data["chatgptAccountId"] = json!(a);
            }
            if let Some(p) = plan {
                data["planType"] = json!(p);
            }
        }
    }
    let expires_in = refreshed.expires_in.unwrap_or(3600);
    data["expiresAt"] = json!(Utc::now().timestamp_millis() + expires_in * 1000);
    if conn.provider == "codex" {
        data["refreshedAt"] = json!(Utc::now().timestamp_millis());
    }
    persist(state, conn, data).await
}

async fn persist(state: &Arc<AppState>, conn: &Connection, data: serde_json::Value) -> Result<Connection> {
    connections::update(
        &state.db,
        &conn.id,
        ConnectionPatch { name: None, priority: None, is_active: None, api_key: None, data: Some(data.clone()) },
    )
    .await?;
    let mut out = conn.clone();
    out.data = data;
    Ok(out)
}

fn lead_ms(provider: &str) -> Option<i64> {
    Some(match provider {
        "claude" => rf::CLAUDE_REFRESH_LEAD_MS,
        "codex" => rf::CODEX_REFRESH_LEAD_MS,
        "kiro" => rf::KIRO_REFRESH_LEAD_MS,
        "cline" => rf::CLINE_REFRESH_LEAD_MS,
        "codebuddy-cn" | "codebuddy-intl" => rf::CODEBUDDY_REFRESH_LEAD_MS,
        _ => return None,
    })
}
