//! Vertex service-account → access token via JWT bearer grant.
//! Port of the reference `refreshVertexToken`.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use jsonwebtoken::{encode, Algorithm, EncodingKey, Header};
use ninty_core::error::{Error, Result};
use serde::Serialize;
use serde_json::Value;

const TOKEN_URL: &str = "https://oauth2.googleapis.com/token";
const SCOPE: &str = "https://www.googleapis.com/auth/cloud-platform";
const REFRESH_LEAD_SECS: i64 = 300;

struct CachedToken {
    access_token: String,
    expires_at: i64,
}

fn cache() -> &'static Mutex<HashMap<String, CachedToken>> {
    static CACHE: OnceLock<Mutex<HashMap<String, CachedToken>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

#[derive(Serialize)]
struct Claims<'a> {
    iss: &'a str,
    aud: &'a str,
    scope: &'a str,
    iat: i64,
    exp: i64,
}

/// Mint (or return cached) access token for a service-account JSON string.
pub async fn mint_access_token(client: &reqwest::Client, sa_json: &str) -> Result<String> {
    let sa: Value = serde_json::from_str(sa_json)
        .map_err(|e| Error::BadRequest(format!("invalid service-account JSON: {e}")))?;
    let client_email = sa
        .get("client_email")
        .and_then(|e| e.as_str())
        .ok_or_else(|| Error::BadRequest("SA JSON missing client_email".into()))?;
    let private_key = sa
        .get("private_key")
        .and_then(|k| k.as_str())
        .ok_or_else(|| Error::BadRequest("SA JSON missing private_key".into()))?;

    let now = chrono::Utc::now().timestamp();
    {
        let cache = cache().lock().map_err(|e| Error::Internal(format!("cache: {e}")))?;
        if let Some(t) = cache.get(client_email) {
            if t.expires_at - REFRESH_LEAD_SECS > now {
                return Ok(t.access_token.clone());
            }
        }
    }

    let claims = Claims {
        iss: client_email,
        aud: TOKEN_URL,
        scope: SCOPE,
        iat: now,
        exp: now + 3600,
    };
    let key = EncodingKey::from_rsa_pem(private_key.as_bytes())
        .map_err(|e| Error::BadRequest(format!("SA private_key parse: {e}")))?;
    let jwt = encode(&Header::new(Algorithm::RS256), &claims, &key)
        .map_err(|e| Error::Internal(format!("jwt encode: {e}")))?;

    let resp: Value = client
        .post(TOKEN_URL)
        .form(&[
            ("grant_type", "urn:ietf:params:oauth:grant-type:jwt-bearer"),
            ("assertion", jwt.as_str()),
        ])
        .send()
        .await
        .map_err(|e| Error::Upstream { status: 502, message: format!("google token: {e}") })?
        .json()
        .await
        .map_err(|e| Error::Upstream { status: 502, message: format!("google token json: {e}") })?;

    let access_token = resp
        .get("access_token")
        .and_then(|t| t.as_str())
        .ok_or_else(|| Error::Upstream {
            status: 502,
            message: format!("no access_token in google response: {resp}"),
        })?
        .to_string();
    let expires_in = resp.get("expires_in").and_then(|e| e.as_i64()).unwrap_or(3600);

    let mut cache = cache().lock().map_err(|e| Error::Internal(format!("cache: {e}")))?;
    cache.insert(
        client_email.to_string(),
        CachedToken {
            access_token: access_token.clone(),
            expires_at: now + expires_in,
        },
    );
    Ok(access_token)
}
