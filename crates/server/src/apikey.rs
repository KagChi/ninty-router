//! Client API-key enforcement for /v1 endpoints: requireApiKey setting,
//! per-key token limits (total/daily/monthly), RPM sliding window, model allow-list.

use std::collections::{HashMap, VecDeque};
use std::sync::{Mutex, OnceLock};
use std::time::Instant;

use axum::http::HeaderMap;
use ninty_core::error::{Error, Result};

use crate::repos::api_keys::{self, ApiKey};
use crate::state::AppState;

/// Extract key from Authorization Bearer, x-api-key, or x-goog-api-key.
pub fn extract(headers: &HeaderMap) -> Option<String> {
    if let Some(v) = headers.get("authorization").and_then(|v| v.to_str().ok()) {
        if let Some(rest) = v.strip_prefix("Bearer ") {
            return Some(rest.trim().to_string());
        }
    }
    for h in ["x-api-key", "x-goog-api-key"] {
        if let Some(v) = headers.get(h).and_then(|v| v.to_str().ok()) {
            return Some(v.trim().to_string());
        }
    }
    None
}

pub struct KeyGuard {
    pub key: Option<ApiKey>,
}

impl KeyGuard {
    /// The raw key string if a validated key is present.
    pub fn key_str(&self) -> Option<&str> {
        self.key.as_ref().map(|k| k.key.as_str())
    }
}

/// Validate request against settings + key limits. Errors are user-facing JSON.
pub async fn check(state: &AppState, headers: &HeaderMap, model: &str) -> Result<KeyGuard> {
    let settings = crate::repos::settings::get(&state.db).await?;
    let raw = extract(headers);

    let Some(raw_key) = raw else {
        if settings.require_api_key {
            return Err(Error::Unauthorized);
        }
        return Ok(KeyGuard { key: None });
    };

    let Some(key) = api_keys::get_by_key(&state.db, &raw_key).await? else {
        if settings.require_api_key {
            return Err(Error::Unauthorized);
        }
        return Ok(KeyGuard { key: None });
    };

    if !key.is_active {
        return Err(Error::Unauthorized);
    }

    // model allow-list
    if !key.allowed_models.is_empty() && !key.allowed_models.iter().any(|m| m == model) {
        return Err(Error::BadRequest(format!(
            "model '{model}' not allowed for this key"
        )));
    }

    // token limit
    if let Some(limit) = key.token_limit {
        let since = match key.limit_window.as_deref() {
            Some("daily") => Some((chrono::Utc::now() - chrono::Duration::days(1)).to_rfc3339()),
            Some("monthly") => Some((chrono::Utc::now() - chrono::Duration::days(30)).to_rfc3339()),
            _ => key.limit_reset_at.clone(),
        };
        let used = crate::repos::usage::key_usage_since(&state.db, &key.key, since).await?;
        if used >= limit {
            return Err(Error::BadRequest(format!(
                "token limit reached ({used}/{limit})"
            )));
        }
    }

    // RPM sliding window
    if let Some(rpm) = key.rpm_limit {
        if !rpm_check(&key.key, rpm) {
            return Err(Error::BadRequest(format!(
                "rate limit: {rpm} requests/minute"
            )));
        }
    }

    Ok(KeyGuard { key: Some(key) })
}

fn rpm_store() -> &'static Mutex<HashMap<String, VecDeque<Instant>>> {
    static STORE: OnceLock<Mutex<HashMap<String, VecDeque<Instant>>>> = OnceLock::new();
    STORE.get_or_init(|| Mutex::new(HashMap::new()))
}

fn rpm_check(key: &str, limit: i64) -> bool {
    let mut store = match rpm_store().lock() {
        Ok(s) => s,
        Err(_) => return true, // poisoned: fail open, don't block traffic
    };
    let now = Instant::now();
    let window = std::time::Duration::from_secs(60);
    let q = store.entry(key.to_string()).or_default();
    while let Some(&t) = q.front() {
        if now.duration_since(t) > window {
            q.pop_front();
        } else {
            break;
        }
    }
    if q.len() as i64 >= limit {
        return false;
    }
    q.push_back(now);
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rpm_window() {
        let key = "test-rpm-key";
        assert!(rpm_check(key, 2));
        assert!(rpm_check(key, 2));
        assert!(!rpm_check(key, 2));
        assert!(rpm_check("other", 1));
    }
}
