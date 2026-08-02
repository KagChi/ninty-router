//! DefaultExecutor: config-driven upstream POST with per-status retry.
//! Retry rules from the reference runtimeConfig:
//! 429 → 0 attempts (fallback handles), 502 → 3×3s, 503 → 3×2s, 504 → 2×3s.

use ninty_core::error::{Error, Result};
use ninty_core::registry::ProviderDef;

pub const CONNECT_TIMEOUT_MS: u64 = 60_000;

/// (status, attempts, delay between attempts)
pub const RETRY_RULES: &[(u16, u32, u64)] = &[(502, 3, 3_000), (503, 3, 2_000), (504, 2, 3_000)];

pub fn retry_config_for(status: u16) -> Option<(u32, u64)> {
    RETRY_RULES
        .iter()
        .find(|(s, _, _)| *s == status)
        .map(|(_, a, d)| (*a, *d))
}

pub fn build_client() -> Result<reqwest::Client> {
    reqwest::Client::builder()
        .connect_timeout(std::time::Duration::from_millis(CONNECT_TIMEOUT_MS))
        .build()
        .map_err(|e| Error::Internal(format!("http client: {e}")))
}

/// POST a chat body to the provider's transport. Bearer auth from `api_key`.
/// Retries per RETRY_RULES; returns the final response (any status) otherwise.
pub async fn execute(
    client: &reqwest::Client,
    provider: &ProviderDef,
    url: &str,
    api_key: &str,
    body: &serde_json::Value,
) -> Result<reqwest::Response> {
    let timeout = std::time::Duration::from_millis(provider.transport.timeout_ms);
    let mut attempt = 0u32;
    loop {
        let mut req = client
            .post(url)
            .timeout(timeout)
            .header("content-type", "application/json");
        if !api_key.is_empty() {
            req = req.bearer_auth(api_key);
        }
        for (k, v) in provider.transport.headers {
            req = req.header(*k, *v);
        }
        let result = req.json(body).send().await;
        match result {
            Ok(resp) => {
                let status = resp.status().as_u16();
                if let Some((max_attempts, delay_ms)) = retry_config_for(status) {
                    attempt += 1;
                    if attempt < max_attempts {
                        tracing::warn!("upstream {status}, retry {attempt}/{max_attempts}");
                        tokio::time::sleep(std::time::Duration::from_millis(delay_ms)).await;
                        continue;
                    }
                }
                return Ok(resp);
            }
            Err(e) => {
                // Connection-level failure: one retry on timeout/connect error.
                attempt += 1;
                if attempt < 2 && (e.is_connect() || e.is_timeout()) {
                    tracing::warn!("connect error, retrying: {e}");
                    tokio::time::sleep(std::time::Duration::from_millis(1_000)).await;
                    continue;
                }
                return Err(Error::Upstream {
                    status: 502,
                    message: format!("upstream request failed: {e}"),
                });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retry_rules() {
        assert_eq!(retry_config_for(502), Some((3, 3_000)));
        assert_eq!(retry_config_for(503), Some((3, 2_000)));
        assert_eq!(retry_config_for(504), Some((2, 3_000)));
        assert_eq!(retry_config_for(429), None);
        assert_eq!(retry_config_for(200), None);
    }
}
