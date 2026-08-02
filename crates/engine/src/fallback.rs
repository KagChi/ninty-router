//! Error classification for account/combo fallback.
//! Port of the reference `services/accountFallback.js` + `config/errorConfig.js`.

pub const MAX_RATE_LIMIT_COOLDOWN_MS: i64 = 30 * 60 * 1_000;
const BACKOFF_BASE_MS: i64 = 2_000;
const BACKOFF_CAP_MS: i64 = 5 * 60 * 1_000;
const BACKOFF_MAX_LEVEL: u32 = 15;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verdict {
    /// Fall back to next account/model after writing cooldown.
    Fallback { cooldown_ms: i64 },
    /// Permanent: deactivate the connection.
    Deactivate,
    /// Do not fall back; return the error to the client.
    NoFallback,
}

fn contains_any(hay: &str, needles: &[&str]) -> bool {
    let h = hay.to_ascii_lowercase();
    needles.iter().any(|n| h.contains(n))
}

const CREDITS_PATTERNS: &[&str] = &[
    "14018",
    "积分不足",
    "余额不足",
    "insufficient credits",
    "credit balance",
    "insufficient balance",
];

/// Classify an upstream failure. `backoff_level` = connection's current level.
pub fn classify(status: u16, body_text: &str, backoff_level: u32) -> Verdict {
    if contains_any(body_text, CREDITS_PATTERNS) {
        return Verdict::Deactivate;
    }

    // ordered text-then-status rules
    if contains_any(body_text, &["no credentials"]) || matches!(status, 401..=404) {
        return Verdict::Fallback {
            cooldown_ms: 2 * 60 * 1_000,
        };
    }
    if contains_any(body_text, &["request not allowed"]) {
        return Verdict::Fallback { cooldown_ms: 5_000 };
    }
    if status == 429
        || contains_any(
            body_text,
            &[
                "rate limit",
                "too many requests",
                "quota exceeded",
                "capacity",
                "overloaded",
            ],
        )
    {
        let level = (backoff_level + 1).min(BACKOFF_MAX_LEVEL);
        let ms = (BACKOFF_BASE_MS * 2_i64.pow(level)).min(BACKOFF_CAP_MS);
        return Verdict::Fallback { cooldown_ms: ms };
    }
    if matches!(status, 400 | 422) {
        return Verdict::NoFallback;
    }
    Verdict::Fallback { cooldown_ms: 30_000 }
}

/// Provider-reported reset time (e.g. codex usage_limit) overrides, capped 30min.
pub fn with_resets_at(verdict: Verdict, resets_at_ms: Option<i64>) -> Verdict {
    match (verdict, resets_at_ms) {
        (Verdict::Fallback { cooldown_ms }, Some(reset)) => {
            let capped = reset.min(MAX_RATE_LIMIT_COOLDOWN_MS);
            Verdict::Fallback {
                cooldown_ms: cooldown_ms.max(capped),
            }
        }
        (v, _) => v,
    }
}

/// Try to extract a provider-reported reset (ms from now) from an error body.
pub fn extract_resets_at(body: &serde_json::Value) -> Option<i64> {
    for key in ["resetsAtMs", "resets_at_ms", "resetMs", "retryAfterMs"] {
        if let Some(v) = body.get(key).and_then(|v| v.as_i64()) {
            return Some(v);
        }
    }
    // nested: error.resetsAt / resets_at as RFC3339
    let err = body.get("error").unwrap_or(body);
    for key in ["resetsAt", "resets_at"] {
        if let Some(s) = err.get(key).and_then(|v| v.as_str()) {
            if let Ok(t) = chrono::DateTime::parse_from_rfc3339(s) {
                let ms = t.timestamp_millis() - chrono::Utc::now().timestamp_millis();
                if ms > 0 {
                    return Some(ms);
                }
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rules() {
        assert_eq!(
            classify(401, "", 0),
            Verdict::Fallback { cooldown_ms: 120_000 }
        );
        assert_eq!(
            classify(200, "no credentials found", 0),
            Verdict::Fallback { cooldown_ms: 120_000 }
        );
        assert_eq!(
            classify(200, "request not allowed", 0),
            Verdict::Fallback { cooldown_ms: 5_000 }
        );
        assert_eq!(
            classify(429, "", 0),
            Verdict::Fallback { cooldown_ms: 4_000 }
        );
        assert_eq!(
            classify(429, "", 2),
            Verdict::Fallback { cooldown_ms: 16_000 }
        );
        assert_eq!(
            classify(429, "", 14),
            Verdict::Fallback { cooldown_ms: 300_000 } // capped 5min
        );
        assert_eq!(classify(400, "bad request", 0), Verdict::NoFallback);
        assert_eq!(classify(422, "", 0), Verdict::NoFallback);
        assert_eq!(
            classify(500, "", 0),
            Verdict::Fallback { cooldown_ms: 30_000 }
        );
        assert_eq!(classify(200, "error 14018 积分不足", 0), Verdict::Deactivate);
    }

    #[test]
    fn resets_at_capped() {
        let v = with_resets_at(Verdict::Fallback { cooldown_ms: 5_000 }, Some(999_999_999));
        assert_eq!(v, Verdict::Fallback { cooldown_ms: MAX_RATE_LIMIT_COOLDOWN_MS });
    }
}
