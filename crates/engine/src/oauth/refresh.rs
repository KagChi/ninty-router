//! Token refresh: per-provider fns + should_refresh lead logic.
//! Port of open-sse/services/tokenRefresh/providers.js (claude/codex/github/kiro).

use serde_json::{json, Value};

use ninty_core::error::{Error, Result};

pub const CLAUDE_CLIENT_ID: &str = "9d1c250a-e61b-44d9-88ed-5944d1962f5e";
pub const CLAUDE_TOKEN_URL: &str = "https://api.anthropic.com/v1/oauth/token";
pub const CLAUDE_REFRESH_LEAD_MS: i64 = 14_400_000; // 4h

pub const CODEX_CLIENT_ID: &str = "app_EMoamEEZ73f0CkXaXp7hrann";
pub const CODEX_TOKEN_URL: &str = "https://auth.openai.com/oauth/token";
pub const CODEX_REFRESH_LEAD_MS: i64 = 432_000_000; // 5d
pub const CODEX_MAX_REFRESH_AGE_MS: i64 = 691_200_000; // 8d — force re-auth beyond

pub const GITHUB_CLIENT_ID: &str = "Iv1.b507a08c87ecfe98";
pub const GITHUB_DEVICE_CODE_URL: &str = "https://github.com/login/device/code";
pub const GITHUB_TOKEN_URL: &str = "https://github.com/login/oauth/access_token";
pub const COPILOT_TOKEN_URL: &str = "https://api.github.com/copilot_internal/v2/token";
pub const COPILOT_UA: &str = "GitHubCopilotChat/0.26.7";
pub const COPILOT_EDITOR_VERSION: &str = "vscode/1.85.0";
pub const COPILOT_PLUGIN_VERSION: &str = "copilot-chat/0.26.7";
pub const COPILOT_API_VERSION: &str = "2022-11-28";
/// Re-mint copilot token when this close to expiry.
pub const COPILOT_REMINT_LEAD_MS: i64 = 60_000;

pub const KIRO_OIDC: &str = "https://oidc.us-east-1.amazonaws.com";
pub const KIRO_REFRESH_LEAD_MS: i64 = 300_000;

#[derive(Debug, Clone)]
pub struct Refreshed {
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub id_token: Option<String>,
    pub expires_in: Option<i64>,
    /// provider-specific extras to merge into connection data
    pub extra: Value,
}

/// True when `expires_at_ms` is within `lead_ms` of now (or past).
pub fn should_refresh(expires_at_ms: i64, lead_ms: i64) -> bool {
    chrono::Utc::now().timestamp_millis() + lead_ms >= expires_at_ms
}

/// Codex: refresh allowed only while refresh token younger than 8d.
pub fn codex_must_reauth(refreshed_at_ms: Option<i64>) -> bool {
    match refreshed_at_ms {
        Some(t) => chrono::Utc::now().timestamp_millis() - t > CODEX_MAX_REFRESH_AGE_MS,
        None => false,
    }
}

/// Copilot token (separate from github oauth token) near expiry?
pub fn copilot_needs_remint(expires_at_rfc3339: Option<&str>) -> bool {
    match expires_at_rfc3339 {
        Some(s) => chrono::DateTime::parse_from_rfc3339(s)
            .map(|t| {
                t.timestamp_millis() - chrono::Utc::now().timestamp_millis()
                    < COPILOT_REMINT_LEAD_MS
            })
            .unwrap_or(true),
        None => true,
    }
}

async fn post_json(client: &reqwest::Client, url: &str, body: &Value) -> Result<Value> {
    let resp = client
        .post(url)
        .header("content-type", "application/json")
        .header("accept", "application/json")
        .json(body)
        .send()
        .await
        .map_err(|e| Error::Upstream {
            status: 502,
            message: e.to_string(),
        })?;
    let status = resp.status().as_u16();
    let text = resp.text().await.unwrap_or_default();
    if !(200..300).contains(&status) {
        return Err(Error::Upstream {
            status,
            message: text,
        });
    }
    serde_json::from_str(&text).map_err(Error::from)
}

pub async fn refresh_claude(client: &reqwest::Client, refresh_token: &str) -> Result<Refreshed> {
    let v = post_json(
        client,
        CLAUDE_TOKEN_URL,
        &json!({
            "grant_type": "refresh_token",
            "refresh_token": refresh_token,
            "client_id": CLAUDE_CLIENT_ID,
        }),
    )
    .await?;
    Ok(Refreshed {
        access_token: v["access_token"].as_str().unwrap_or("").into(),
        refresh_token: v["refresh_token"].as_str().map(String::from),
        id_token: None,
        expires_in: v["expires_in"].as_i64(),
        extra: Value::Null,
    })
}

pub async fn refresh_codex(client: &reqwest::Client, refresh_token: &str) -> Result<Refreshed> {
    let v = post_json(
        client,
        CODEX_TOKEN_URL,
        &json!({
            "client_id": CODEX_CLIENT_ID,
            "grant_type": "refresh_token",
            "refresh_token": refresh_token,
        }),
    )
    .await?;
    Ok(Refreshed {
        access_token: v["access_token"].as_str().unwrap_or("").into(),
        refresh_token: v["refresh_token"].as_str().map(String::from),
        id_token: v["id_token"].as_str().map(String::from),
        expires_in: v["expires_in"].as_i64(),
        extra: Value::Null,
    })
}

/// GitHub OAuth tokens don't expire/refresh; copilot token is minted from it.
pub async fn mint_copilot_token(client: &reqwest::Client, github_token: &str) -> Result<Refreshed> {
    let resp = client
        .get(COPILOT_TOKEN_URL)
        .header("authorization", format!("token {github_token}"))
        .header("user-agent", COPILOT_UA)
        .header("editor-version", COPILOT_EDITOR_VERSION)
        .header("editor-plugin-version", COPILOT_PLUGIN_VERSION)
        .header("accept", "application/json")
        .header("x-github-api-version", COPILOT_API_VERSION)
        .send()
        .await
        .map_err(|e| Error::Upstream {
            status: 502,
            message: e.to_string(),
        })?;
    let status = resp.status().as_u16();
    let text = resp.text().await.unwrap_or_default();
    if !(200..300).contains(&status) {
        return Err(Error::Upstream {
            status,
            message: text,
        });
    }
    let v: Value = serde_json::from_str(&text)?;
    Ok(Refreshed {
        access_token: v["token"].as_str().unwrap_or("").into(),
        refresh_token: None,
        id_token: None,
        expires_in: None,
        extra: json!({"copilotTokenExpiresAt": v["expires_at"].as_str().unwrap_or("")}),
    })
}

/// Kiro: SSO OIDC refresh with stored client creds (camelCase JSON).
pub async fn refresh_kiro(
    client: &reqwest::Client,
    oidc_endpoint: &str,
    client_id: &str,
    client_secret: &str,
    refresh_token: &str,
) -> Result<Refreshed> {
    let v = post_json(
        client,
        &format!("{oidc_endpoint}/token"),
        &json!({
            "clientId": client_id,
            "clientSecret": client_secret,
            "grantType": "refresh_token",
            "refreshToken": refresh_token,
        }),
    )
    .await?;
    Ok(Refreshed {
        access_token: v["accessToken"]
            .as_str()
            .or_else(|| v["access_token"].as_str())
            .unwrap_or("")
            .into(),
        refresh_token: v["refreshToken"]
            .as_str()
            .or_else(|| v["refresh_token"].as_str())
            .map(String::from),
        id_token: None,
        expires_in: v["expiresIn"].as_i64().or_else(|| v["expires_in"].as_i64()),
        extra: Value::Null,
    })
}

/// Parse codex id_token JWT claims → (chatgpt_account_id, plan_type).
pub fn codex_parse_id_token(id_token: &str) -> (Option<String>, Option<String>) {
    let mut parts = id_token.split('.');
    parts.next();
    let payload = match parts.next() {
        Some(p) => p,
        None => return (None, None),
    };
    use base64::Engine;
    let bytes = match base64::engine::general_purpose::URL_SAFE_NO_PAD.decode(payload) {
        Ok(b) => b,
        Err(_) => return (None, None),
    };
    let v: Value = match serde_json::from_slice(&bytes) {
        Ok(v) => v,
        Err(_) => return (None, None),
    };
    let auth = v
        .get("https://api.openai.com/auth")
        .cloned()
        .unwrap_or(Value::Null);
    (
        auth.get("chatgpt_account_id")
            .and_then(Value::as_str)
            .map(String::from),
        auth.get("chatgpt_plan_type")
            .and_then(Value::as_str)
            .map(String::from),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn should_refresh_lead() {
        let now = chrono::Utc::now().timestamp_millis();
        assert!(should_refresh(now - 1000, CLAUDE_REFRESH_LEAD_MS)); // past
        assert!(should_refresh(now + 3_600_000, CLAUDE_REFRESH_LEAD_MS)); // within 4h lead
        assert!(!should_refresh(now + 5 * 3_600_000, CLAUDE_REFRESH_LEAD_MS)); // 5h out
    }

    #[test]
    fn codex_stale_8d() {
        let now = chrono::Utc::now().timestamp_millis();
        assert!(!codex_must_reauth(Some(now - 7 * 86_400_000)));
        assert!(codex_must_reauth(Some(now - 9 * 86_400_000)));
        assert!(!codex_must_reauth(None));
    }

    #[test]
    fn copilot_remint() {
        assert!(copilot_needs_remint(None));
        assert!(copilot_needs_remint(Some("not-a-date")));
        let future = (chrono::Utc::now() + chrono::Duration::hours(1)).to_rfc3339();
        assert!(!copilot_needs_remint(Some(&future)));
        let soon = (chrono::Utc::now() + chrono::Duration::seconds(30)).to_rfc3339();
        assert!(copilot_needs_remint(Some(&soon)));
    }

    #[test]
    fn codex_id_token_parse() {
        use base64::Engine;
        let payload = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(
            br#"{"https://api.openai.com/auth":{"chatgpt_account_id":"acc_1","chatgpt_plan_type":"plus"}}"#,
        );
        let token = format!("h.{payload}.s");
        assert_eq!(
            codex_parse_id_token(&token),
            (Some("acc_1".into()), Some("plus".into()))
        );
        assert_eq!(codex_parse_id_token("bad"), (None, None));
    }
}

// ---------------------------------------------------------------------------
// M07: cline + codebuddy
// ---------------------------------------------------------------------------

pub const CLINE_REFRESH_URL: &str = "https://api.cline.bot/api/v1/auth/refresh";
pub const CLINE_REFRESH_LEAD_MS: i64 = 300_000;
pub const CODEBUDDY_REFRESH_LEAD_MS: i64 = 300_000;

pub struct CodebuddyProfile {
    pub refresh_url: &'static str,
    pub user_agent: &'static str,
    pub domain: &'static str,
}

pub const CODEBUDDY_CN: CodebuddyProfile = CodebuddyProfile {
    refresh_url: "https://copilot.tencent.com/v2/plugin/auth/token/refresh",
    user_agent: "CLI/2.63.2 CodeBuddy/2.63.2",
    domain: "copilot.tencent.com",
};
pub const CODEBUDDY_INTL: CodebuddyProfile = CodebuddyProfile {
    refresh_url: "https://www.codebuddy.ai/v2/plugin/auth/token/refresh",
    user_agent: "IDE/2.63.2 CodeBuddy/2.63.2",
    domain: "www.codebuddy.ai",
};

/// Cline: POST {refresh_token} JSON to refresh URL.
pub async fn refresh_cline(client: &reqwest::Client, refresh_token: &str) -> Result<Refreshed> {
    let v = post_json(
        client,
        CLINE_REFRESH_URL,
        &json!({"refresh_token": refresh_token}),
    )
    .await?;
    let data = v.get("data").cloned().unwrap_or(v.clone());
    let access = data
        .get("accessToken")
        .or_else(|| data.get("access_token"))
        .and_then(Value::as_str)
        .unwrap_or("");
    if access.is_empty() {
        return Err(Error::Upstream {
            status: 401,
            message: "cline refresh returned no token".into(),
        });
    }
    Ok(Refreshed {
        access_token: access.into(),
        refresh_token: data
            .get("refreshToken")
            .or_else(|| data.get("refresh_token"))
            .and_then(Value::as_str)
            .map(String::from),
        id_token: None,
        expires_in: data
            .get("expiresIn")
            .or_else(|| data.get("expires_in"))
            .and_then(Value::as_i64),
        extra: Value::Null,
    })
}

/// CodeBuddy: POST {} with X-Refresh-Token header.
pub async fn refresh_codebuddy(
    client: &reqwest::Client,
    profile: &CodebuddyProfile,
    refresh_token: &str,
) -> Result<Refreshed> {
    let resp = client
        .post(profile.refresh_url)
        .header("content-type", "application/json")
        .header("accept", "application/json")
        .header("user-agent", profile.user_agent)
        .header("x-requested-with", "XMLHttpRequest")
        .header("x-domain", profile.domain)
        .header("x-refresh-token", refresh_token)
        .header("x-auth-refresh-source", "plugin")
        .header("x-product", "SaaS")
        .body("{}")
        .send()
        .await
        .map_err(|e| Error::Upstream {
            status: 502,
            message: e.to_string(),
        })?;
    let status = resp.status().as_u16();
    let text = resp.text().await.unwrap_or_default();
    if !(200..300).contains(&status) {
        return Err(Error::Upstream {
            status,
            message: text,
        });
    }
    let v: Value = serde_json::from_str(&text)?;
    let data = v.get("data").cloned().unwrap_or(Value::Null);
    let access = data
        .get("accessToken")
        .and_then(Value::as_str)
        .unwrap_or("");
    if v.get("code").and_then(Value::as_i64) != Some(0) || access.is_empty() {
        return Err(Error::Upstream {
            status: 401,
            message: format!(
                "codebuddy refresh failed: {}",
                v.get("msg").and_then(Value::as_str).unwrap_or("no token")
            ),
        });
    }
    Ok(Refreshed {
        access_token: access.into(),
        refresh_token: data
            .get("refreshToken")
            .and_then(Value::as_str)
            .map(String::from),
        id_token: None,
        expires_in: data.get("expiresIn").and_then(Value::as_i64),
        extra: Value::Null,
    })
}

/// Cline token: prefix `workos:` when absent.
pub fn cline_workos(token: &str) -> String {
    let t = token.trim();
    if t.is_empty() || t.starts_with("workos:") {
        t.to_string()
    } else {
        format!("workos:{t}")
    }
}

#[cfg(test)]
mod m07_tests {
    use super::*;

    #[test]
    fn workos_prefix() {
        assert_eq!(cline_workos("abc"), "workos:abc");
        assert_eq!(cline_workos("workos:abc"), "workos:abc");
        assert_eq!(cline_workos("  "), "");
    }
}
