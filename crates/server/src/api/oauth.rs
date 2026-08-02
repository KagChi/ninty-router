//! /api/oauth/{provider}/... — authorize URL, code exchange, device flow, poll.
//! Pending flows kept in memory (single-process router).

use std::collections::HashMap;
use std::sync::{Arc, OnceLock};

use axum::extract::{Path, State};
use axum::routing::{get, post};
use axum::{Json, Router};
use ninty_core::error::Error;
use serde_json::{json, Value};
use tokio::sync::Mutex;

use super::ApiError;
use crate::repos::connections::{self, NewConnection};
use crate::state::AppState;
use engine::oauth::{pkce, refresh as rf};

const CODEX_REDIRECT_URI: &str = "http://localhost:1455/auth/callback";
const CLAUDE_REDIRECT_URI: &str = "https://console.anthropic.com/oauth/code/callback";

#[derive(Clone)]
struct Pending {
    #[allow(dead_code)]
    provider: String,
    verifier: String,
    #[allow(dead_code)]
    state: String,
    /// device flow fields
    device_code: Option<String>,
    interval: u64,
    /// kiro: stored for token poll
    kiro_client: Option<(String, String, String)>, // (clientId, clientSecret, endpoint)
}

static PENDING: OnceLock<Mutex<HashMap<String, Pending>>> = OnceLock::new();

fn pending() -> &'static Mutex<HashMap<String, Pending>> {
    PENDING.get_or_init(|| Mutex::new(HashMap::new()))
}

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/{provider}/authorize", post(authorize))
        .route("/{provider}/exchange", post(exchange))
        .route("/{provider}/device-code", post(device_code))
        .route("/{provider}/poll", get(poll))
}

fn oauth_enabled(provider: &str) -> bool {
    matches!(provider, "claude" | "codex" | "github" | "kiro" | "cline" | "codebuddy-cn" | "codebuddy-intl" | "qoder")
}

struct CodebuddyOauth {
    state_url: &'static str,
    token_url: &'static str,
    user_agent: &'static str,
    domain: &'static str,
    platform: &'static str,
}

fn codebuddy_oauth(provider: &str) -> Option<CodebuddyOauth> {
    match provider {
        "codebuddy-cn" => Some(CodebuddyOauth {
            state_url: "https://copilot.tencent.com/v2/plugin/auth/state",
            token_url: "https://copilot.tencent.com/v2/plugin/auth/token",
            user_agent: "CLI/2.63.2 CodeBuddy/2.63.2",
            domain: "copilot.tencent.com",
            platform: "CLI",
        }),
        "codebuddy-intl" => Some(CodebuddyOauth {
            state_url: "https://www.codebuddy.ai/v2/plugin/auth/state",
            token_url: "https://www.codebuddy.ai/v2/plugin/auth/token",
            user_agent: "IDE/2.63.2 CodeBuddy/2.63.2",
            domain: "www.codebuddy.ai",
            platform: "ide",
        }),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// authorization-code flows (claude paste-code, codex loopback)
// ---------------------------------------------------------------------------

async fn authorize(Path(provider): Path<String>) -> Result<Json<Value>, ApiError> {
    if !oauth_enabled(&provider) {
        return Err(Error::BadRequest(format!("oauth not supported for '{provider}'")).into());
    }
    let (verifier, state) = (pkce::generate_verifier(), pkce::generate_state());
    let challenge = pkce::challenge_s256(&verifier);

    let url = match provider.as_str() {
        "claude" => format!(
            "https://claude.ai/oauth/authorize?code=true&client_id={}&response_type=code&redirect_uri={}&scope={}&code_challenge={}&code_challenge_method=S256&state={}",
            rf::CLAUDE_CLIENT_ID,
            urlenc(CLAUDE_REDIRECT_URI),
            urlenc("org:create_api_key user:profile user:inference"),
            challenge,
            state,
        ),
        "codex" => format!(
            "https://auth.openai.com/oauth/authorize?response_type=code&client_id={}&redirect_uri={}&scope={}&code_challenge={}&code_challenge_method=S256&state={}&id_token_add_organizations=true&codex_cli_simplified_flow=true&originator=codex_cli_rs",
            rf::CODEX_CLIENT_ID,
            urlenc(CODEX_REDIRECT_URI),
            urlenc("openid profile email offline_access"),
            challenge,
            state,
        ),
        "cline" => format!(
            "https://api.cline.bot/api/v1/auth/authorize?client_type=extension&callback_url={}&redirect_uri={}",
            urlenc("http://localhost:20128/api/oauth/cline/callback"),
            urlenc("http://localhost:20128/api/oauth/cline/callback"),
        ),
        _ => return Err(Error::BadRequest(format!("'{provider}' uses device flow")).into()),
    };

    pending().lock().await.insert(
        state.clone(),
        Pending { provider, verifier, state: state.clone(), device_code: None, interval: 5, kiro_client: None },
    );
    Ok(Json(json!({"authorize_url": url, "state": state})))
}

async fn exchange(
    State(state): State<Arc<AppState>>,
    Path(provider): Path<String>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    let code = body.get("code").and_then(Value::as_str).unwrap_or("").to_string();
    if code.is_empty() {
        return Err(Error::BadRequest("missing code".into()).into());
    }

    match provider.as_str() {
        "claude" => {
            // code may carry "#state"
            let (auth_code, code_state) = match code.split_once('#') {
                Some((c, s)) => (c.to_string(), s.to_string()),
                None => (code.clone(), String::new()),
            };
            let state_key = if !code_state.is_empty() {
                code_state.clone()
            } else {
                body.get("state").and_then(Value::as_str).unwrap_or("").to_string()
            };
            let verifier = {
                let mut map = pending().lock().await;
                match map.remove(&state_key) {
                    Some(p) => p.verifier,
                    // fallback: single in-flight flow — take any pending claude flow
                    None => {
                        let key = map.keys().next().cloned();
                        key.and_then(|k| map.remove(&k)).map(|p| p.verifier).unwrap_or_default()
                    }
                }
            };
            let v: Value = state
                .http
                .post(rf::CLAUDE_TOKEN_URL)
                .json(&json!({
                    "code": auth_code,
                    "state": code_state,
                    "grant_type": "authorization_code",
                    "client_id": rf::CLAUDE_CLIENT_ID,
                    "redirect_uri": CLAUDE_REDIRECT_URI,
                    "code_verifier": verifier,
                }))
                .send()
                .await
                .map_err(upstream)?
                .json()
                .await
                .map_err(upstream)?;
            let expires_in = v["expires_in"].as_i64().unwrap_or(3600);
            let data = json!({
                "accessToken": v["access_token"].as_str().unwrap_or(""),
                "refreshToken": v["refresh_token"].as_str().unwrap_or(""),
                "expiresAt": chrono::Utc::now().timestamp_millis() + expires_in * 1000,
                "scope": v["scope"].as_str().unwrap_or(""),
            });
            let conn = connections::create(
                &state.db,
                NewConnection {
                    provider: "claude".into(),
                    name: body.get("name").and_then(Value::as_str).map(String::from),
                    priority: None,
                    api_key: None,
                    data: Some(data),
                },
            )
            .await?;
            Ok(Json(json!({"connection": conn.sanitized()})))
        }
        "codex" => {
            let v: Value = state
                .http
                .post(rf::CODEX_TOKEN_URL)
                .form(&[
                    ("grant_type", "authorization_code"),
                    ("client_id", rf::CODEX_CLIENT_ID),
                    ("code", &code),
                    ("redirect_uri", CODEX_REDIRECT_URI),
                ])
                .send()
                .await
                .map_err(upstream)?
                .json()
                .await
                .map_err(upstream)?;
            let id_token = v["id_token"].as_str().unwrap_or("").to_string();
            let (acc, plan) = rf::codex_parse_id_token(&id_token);
            let expires_in = v["expires_in"].as_i64().unwrap_or(3600);
            let data = json!({
                "accessToken": v["access_token"].as_str().unwrap_or(""),
                "refreshToken": v["refresh_token"].as_str().unwrap_or(""),
                "idToken": id_token,
                "expiresAt": chrono::Utc::now().timestamp_millis() + expires_in * 1000,
                "refreshedAt": chrono::Utc::now().timestamp_millis(),
                "chatgptAccountId": acc.unwrap_or_default(),
                "planType": plan.unwrap_or_default(),
            });
            let conn = connections::create(
                &state.db,
                NewConnection {
                    provider: "codex".into(),
                    name: body.get("name").and_then(Value::as_str).map(String::from),
                    priority: None,
                    api_key: None,
                    data: Some(data),
                },
            )
            .await?;
            Ok(Json(json!({"connection": conn.sanitized()})))
        }
        "cline" => {
            // code is base64-encoded token JSON (or fall back to POST exchange)
            use base64::Engine;
            let decoded = base64::engine::general_purpose::STANDARD
                .decode(code.trim())
                .ok()
                .and_then(|b| String::from_utf8(b).ok());
            let (access, refresh, email, expires_at) = match decoded.as_deref() {
                Some(text) if text.contains('{') => {
                    let end = text.rfind('}').map(|i| i + 1).unwrap_or(text.len());
                    let v: Value = serde_json::from_str(&text[..end])
                        .map_err(|_| Error::BadRequest("invalid cline code payload".into()))?;
                    (
                        v["accessToken"].as_str().unwrap_or("").to_string(),
                        v["refreshToken"].as_str().unwrap_or("").to_string(),
                        v["email"].as_str().unwrap_or("").to_string(),
                        v["expiresAt"].as_str().unwrap_or("").to_string(),
                    )
                }
                _ => {
                    let v: Value = state
                        .http
                        .post("https://api.cline.bot/api/v1/auth/token")
                        .json(&json!({
                            "grant_type": "authorization_code",
                            "code": code,
                            "client_type": "extension",
                            "redirect_uri": "http://localhost:20128/api/oauth/cline/callback",
                        }))
                        .send()
                        .await
                        .map_err(upstream)?
                        .json()
                        .await
                        .map_err(upstream)?;
                    let d = v.get("data").cloned().unwrap_or(v.clone());
                    (
                        d["accessToken"].as_str().unwrap_or("").to_string(),
                        d["refreshToken"].as_str().unwrap_or("").to_string(),
                        d["userInfo"]["email"].as_str().unwrap_or("").to_string(),
                        d["expiresAt"].as_str().unwrap_or("").to_string(),
                    )
                }
            };
            if access.is_empty() {
                return Err(Error::BadRequest("cline exchange returned no access token".into()).into());
            }
            let expires_ms = chrono::DateTime::parse_from_rfc3339(&expires_at)
                .map(|t| t.timestamp_millis())
                .unwrap_or_else(|_| chrono::Utc::now().timestamp_millis() + 3_600_000);
            let data = json!({
                "accessToken": access,
                "refreshToken": refresh,
                "expiresAt": expires_ms,
            });
            let conn = connections::create(
                &state.db,
                NewConnection {
                    provider: "cline".into(),
                    name: body.get("name").and_then(Value::as_str).map(String::from).or(if email.is_empty() { None } else { Some(email) }),
                    priority: None,
                    api_key: None,
                    data: Some(data),
                },
            )
            .await?;
            Ok(Json(json!({"connection": conn.sanitized()})))
        }
        _ => Err(Error::BadRequest(format!("exchange not supported for '{provider}'")).into()),
    }
}

// ---------------------------------------------------------------------------
// device flows (github, kiro)
// ---------------------------------------------------------------------------

async fn device_code(
    State(state): State<Arc<AppState>>,
    Path(provider): Path<String>,
) -> Result<Json<Value>, ApiError> {
    match provider.as_str() {
        "github" => {
            let v: Value = state
                .http
                .post(rf::GITHUB_DEVICE_CODE_URL)
                .header("accept", "application/json")
                .form(&[("client_id", rf::GITHUB_CLIENT_ID), ("scope", "read:user")])
                .send()
                .await
                .map_err(upstream)?
                .json()
                .await
                .map_err(upstream)?;
            let st = pkce::generate_state();
            pending().lock().await.insert(
                st.clone(),
                Pending {
                    provider: "github".into(),
                    verifier: String::new(),
                    state: st.clone(),
                    device_code: v["device_code"].as_str().map(String::from),
                    interval: v["interval"].as_u64().unwrap_or(5),
                    kiro_client: None,
                },
            );
            Ok(Json(json!({
                "user_code": v["user_code"].as_str().unwrap_or(""),
                "verification_uri": v["verification_uri"].as_str().unwrap_or(""),
                "expires_in": v["expires_in"].as_i64().unwrap_or(900),
                "interval": v["interval"].as_u64().unwrap_or(5),
                "state": st,
            })))
        }
        "kiro" => {
            // register client then device authorization (camelCase JSON)
            let reg: Value = state
                .http
                .post(format!("{}/client/register", rf::KIRO_OIDC))
                .json(&json!({
                    "clientName": "kiro-oauth-client",
                    "clientType": "public",
                    "scopes": ["codewhisperer:completions", "codewhisperer:analysis", "codewhisperer:conversations"],
                    "grantTypes": ["urn:ietf:params:oauth:grant-type:device_code", "refresh_token"],
                    "issuerUrl": "https://identitycenter.amazonaws.com/ssoins-722374e8c3c8e6c6",
                    "redirectUris": [],
                }))
                .send()
                .await
                .map_err(upstream)?
                .json()
                .await
                .map_err(upstream)?;
            let client_id = reg["clientId"].as_str().unwrap_or("").to_string();
            let client_secret = reg["clientSecret"].as_str().unwrap_or("").to_string();
            let dev: Value = state
                .http
                .post(format!("{}/device_authorization", rf::KIRO_OIDC))
                .json(&json!({
                    "clientId": client_id,
                    "clientSecret": client_secret,
                    "startUrl": "https://view.awsapps.com/start",
                }))
                .send()
                .await
                .map_err(upstream)?
                .json()
                .await
                .map_err(upstream)?;
            let st = pkce::generate_state();
            pending().lock().await.insert(
                st.clone(),
                Pending {
                    provider: "kiro".into(),
                    verifier: String::new(),
                    state: st.clone(),
                    device_code: dev["deviceCode"].as_str().map(String::from),
                    interval: dev["interval"].as_u64().unwrap_or(5),
                    kiro_client: Some((client_id, client_secret, rf::KIRO_OIDC.to_string())),
                },
            );
            Ok(Json(json!({
                "user_code": dev["userCode"].as_str().unwrap_or(""),
                "verification_uri": dev["verificationUri"].as_str().unwrap_or(""),
                "expires_in": dev["expiresIn"].as_i64().unwrap_or(600),
                "interval": dev["interval"].as_u64().unwrap_or(5),
                "state": st,
            })))
        }
        "qoder" => {
            let verifier = pkce::generate_verifier();
            let challenge = pkce::challenge_s256(&verifier);
            let nonce = uuid::Uuid::new_v4().to_string();
            let machine_id = uuid::Uuid::new_v4().to_string();
            let st = pkce::generate_state();
            let url = format!(
                "{}?challenge={}&challenge_method=S256&machine_id={}&nonce={}",
                engine::qoder::LOGIN_URL,
                challenge,
                machine_id,
                nonce,
            );
            pending().lock().await.insert(
                st.clone(),
                Pending {
                    provider: "qoder".into(),
                    verifier,
                    state: st.clone(),
                    device_code: Some(nonce),
                    interval: 2,
                    kiro_client: Some((machine_id, String::new(), String::new())),
                },
            );
            Ok(Json(json!({
                "user_code": "",
                "verification_uri": url,
                "expires_in": 300,
                "interval": 2,
                "state": st,
            })))
        }
        _ if codebuddy_oauth(&provider).is_some() => {
            let o = codebuddy_oauth(&provider).expect("checked");
            let resp = state
                .http
                .post(format!("{}?platform={}", o.state_url, o.platform))
                .header("content-type", "application/json")
                .header("accept", "application/json")
                .header("user-agent", o.user_agent)
                .header("x-requested-with", "XMLHttpRequest")
                .header("x-domain", o.domain)
                .header("x-no-authorization", "true")
                .header("x-no-user-id", "true")
                .header("x-product", "SaaS")
                .body("{}")
                .send()
                .await
                .map_err(upstream)?;
            let v: Value = resp.json().await.map_err(upstream)?;
            let data = v.get("data").cloned().unwrap_or(Value::Null);
            let cb_state = data.get("state").and_then(Value::as_str).unwrap_or("").to_string();
            let auth_url = data.get("authUrl").and_then(Value::as_str).unwrap_or("").to_string();
            if v.get("code").and_then(Value::as_i64) != Some(0) || cb_state.is_empty() {
                return Err(Error::BadRequest(format!("codebuddy state error: {}", v.get("msg").and_then(Value::as_str).unwrap_or("missing state"))).into());
            }
            let st = pkce::generate_state();
            pending().lock().await.insert(
                st.clone(),
                Pending {
                    provider: provider.clone(),
                    verifier: String::new(),
                    state: st.clone(),
                    device_code: Some(cb_state),
                    interval: 5,
                    kiro_client: None,
                },
            );
            Ok(Json(json!({
                "user_code": "",
                "verification_uri": auth_url,
                "expires_in": 600,
                "interval": 5,
                "state": st,
            })))
        }
        _ => Err(Error::BadRequest(format!("'{provider}' uses authorization-code flow")).into()),
    }
}

async fn poll(
    State(state): State<Arc<AppState>>,
    Path(provider): Path<String>,
    axum::extract::Query(q): axum::extract::Query<HashMap<String, String>>,
) -> Result<Json<Value>, ApiError> {
    let st = q.get("state").cloned().unwrap_or_default();
    let p = pending().lock().await.get(&st).cloned()
        .ok_or_else(|| Error::BadRequest("unknown or expired flow state".into()))?;

    let tokens: Value = match provider.as_str() {
        "github" => {
            let resp = state
                .http
                .post(rf::GITHUB_TOKEN_URL)
                .header("accept", "application/json")
                .form(&[
                    ("client_id", rf::GITHUB_CLIENT_ID),
                    ("device_code", p.device_code.as_deref().unwrap_or("")),
                    ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
                ])
                .send()
                .await
                .map_err(upstream)?;
            resp.json().await.map_err(upstream)?
        }
        "kiro" => {
            let (cid, cs, endpoint) = p.kiro_client.clone().unwrap_or_default();
            state
                .http
                .post(format!("{endpoint}/token"))
                .json(&json!({
                    "clientId": cid,
                    "clientSecret": cs,
                    "deviceCode": p.device_code.as_deref().unwrap_or(""),
                    "grantType": "urn:ietf:params:oauth:grant-type:device_code",
                }))
                .send()
                .await
                .map_err(upstream)?
                .json()
                .await
                .map_err(upstream)?
        }
        "qoder" => {
            let nonce = p.device_code.clone().unwrap_or_default();
            let verifier = p.verifier.clone();
            let machine_id = p.kiro_client.clone().map(|(m, _, _)| m).unwrap_or_default();
            let url = format!(
                "{}?nonce={}&verifier={}&challenge_method=S256",
                engine::qoder::DEVICE_TOKEN_URL,
                urlenc(&nonce),
                urlenc(&verifier),
            );
            let resp = state
                .http
                .get(&url)
                .header("accept", "application/json")
                .header("user-agent", "Go-http-client/2.0")
                .send()
                .await
                .map_err(upstream)?;
            if resp.status().as_u16() == 202 || resp.status().as_u16() == 404 {
                return Ok(Json(json!({"status": "authorization_pending", "interval": p.interval})));
            }
            let v: Value = resp.json().await.map_err(upstream)?;
            let token = v.get("token").and_then(Value::as_str).unwrap_or("").to_string();
            if token.is_empty() {
                return Ok(Json(json!({"status": "pending", "interval": p.interval})));
            }
            // best-effort userinfo
            let info: Value = match state
                .http
                .get(engine::qoder::USERINFO_URL)
                .header("authorization", format!("Bearer {token}"))
                .header("accept", "application/json")
                .header("user-agent", "Go-http-client/2.0")
                .send()
                .await
            {
                Ok(r) => r.json().await.unwrap_or(Value::Null),
                Err(_) => Value::Null,
            };
            let expires_ms = v
                .get("expires_at")
                .and_then(Value::as_i64)
                .filter(|t| *t > chrono::Utc::now().timestamp_millis())
                .unwrap_or_else(|| chrono::Utc::now().timestamp_millis() + 30 * 86_400_000);
            let data = json!({
                "accessToken": token,
                "refreshToken": v["refresh_token"].as_str().unwrap_or(""),
                "userId": v["user_id"].as_str().unwrap_or(""),
                "machineId": machine_id,
                "expiresAt": expires_ms,
                "name": info["name"].as_str().or_else(|| info["username"].as_str()).unwrap_or(""),
                "email": info["email"].as_str().unwrap_or(""),
                "organizationId": info["organization_id"].as_str().unwrap_or(""),
            });
            let conn = connections::create(
                &state.db,
                NewConnection { provider: "qoder".into(), name: None, priority: None, api_key: None, data: Some(data) },
            )
            .await?;
            pending().lock().await.remove(&st);
            return Ok(Json(json!({"status": "connected", "connection": conn.sanitized()})));
        }
        _ if codebuddy_oauth(&provider).is_some() => {
            let o = codebuddy_oauth(&provider).expect("checked");
            let resp = state
                .http
                .get(format!("{}?state={}", o.token_url, urlenc(p.device_code.as_deref().unwrap_or(""))))
                .header("accept", "application/json")
                .header("user-agent", o.user_agent)
                .header("x-requested-with", "XMLHttpRequest")
                .header("x-domain", o.domain)
                .header("x-no-authorization", "true")
                .header("x-no-user-id", "true")
                .header("x-product", "SaaS")
                .send()
                .await
                .map_err(upstream)?;
            let v: Value = resp.json().await.map_err(upstream)?;
            let code = v.get("code").and_then(Value::as_i64).unwrap_or(-1);
            let data = v.get("data").cloned().unwrap_or(Value::Null);
            if code == 0 && data.get("accessToken").and_then(Value::as_str).is_some() {
                // normalize to common token shape for storage below
                json!({
                    "access_token": data["accessToken"].as_str().unwrap_or(""),
                    "refresh_token": data["refreshToken"].as_str().unwrap_or(""),
                    "expires_in": data["expiresIn"].as_i64().unwrap_or(86400),
                })
            } else {
                return Ok(Json(json!({
                    "status": if code == 11217 { "authorization_pending" } else { "pending" },
                    "interval": p.interval,
                })));
            }
        }
        _ => return Err(Error::BadRequest(format!("poll not supported for '{provider}'")).into()),
    };

    // device flow pending states
    if tokens.get("access_token").is_none() && tokens.get("accessToken").is_none() {
        let err = tokens["error"].as_str().unwrap_or("");
        return Ok(Json(json!({"status": if err.is_empty() { "pending" } else { err }, "interval": p.interval})));
    }

    let gh_token = tokens["access_token"].as_str().unwrap_or("").to_string();
    let data = match provider.as_str() {
        "github" => {
            // mint first copilot token right away
            let minted = rf::mint_copilot_token(&state.http, &gh_token).await?;
            json!({
                "accessToken": gh_token,
                "copilotToken": minted.access_token,
                "copilotTokenExpiresAt": minted.extra["copilotTokenExpiresAt"].as_str().unwrap_or(""),
            })
        }
        _ if codebuddy_oauth(&provider).is_some() => {
            let expires_in = tokens["expires_in"].as_i64().unwrap_or(86400);
            json!({
                "accessToken": tokens["access_token"].as_str().unwrap_or(""),
                "refreshToken": tokens["refresh_token"].as_str().unwrap_or(""),
                "expiresAt": chrono::Utc::now().timestamp_millis() + expires_in * 1000,
            })
        }
        _ => {
            let expires_in = tokens["expiresIn"].as_i64().or_else(|| tokens["expires_in"].as_i64()).unwrap_or(3600);
            let (cid, cs, endpoint) = p.kiro_client.clone().unwrap_or_default();
            json!({
                "accessToken": tokens["accessToken"].as_str().or_else(|| tokens["access_token"].as_str()).unwrap_or(""),
                "refreshToken": tokens["refreshToken"].as_str().or_else(|| tokens["refresh_token"].as_str()).unwrap_or(""),
                "expiresAt": chrono::Utc::now().timestamp_millis() + expires_in * 1000,
                "clientId": cid,
                "clientSecret": cs,
                "ssoOidcEndpoint": endpoint,
                "region": "us-east-1",
                "authMethod": "builder-id",
            })
        }
    };
    let conn = connections::create(
        &state.db,
        NewConnection { provider, name: None, priority: None, api_key: None, data: Some(data) },
    )
    .await?;
    pending().lock().await.remove(&st);
    Ok(Json(json!({"status": "connected", "connection": conn.sanitized()})))
}

fn upstream(e: reqwest::Error) -> ApiError {
    ApiError(Error::Upstream { status: 502, message: e.to_string() })
}

fn urlenc(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            'A'..='Z' | 'a'..='z' | '0'..='9' | '-' | '_' | '.' | '~' => c.to_string(),
            _ => format!("%{:02X}", c as u32),
        })
        .collect()
}
