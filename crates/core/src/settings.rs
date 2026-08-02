use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// App settings, stored as one JSON blob in the `settings` table (id = 1).
/// Missing fields fall back to defaults on read (serde default).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub rtk_enabled: bool,
    pub caveman_enabled: bool,
    pub ponytail_enabled: bool,
    pub ponytail_level: String,
    pub pxpipe_enabled: bool,
    pub pxpipe_min_chars: usize,
    pub pxpipe_timeout_ms: u64,
    pub combo_strategy: String,
    pub sticky_round_robin_limit: u32,
    pub require_api_key: bool,
    pub require_login: bool,
    pub enable_request_logs: bool,
    /// bcrypt hash; empty = no password set. Stripped from API responses at the edge.
    pub password_hash: String,
    pub provider_strategies: HashMap<String, ProviderStrategy>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct ProviderStrategy {
    pub fallback_strategy: Option<String>,
    pub sticky_round_robin_limit: Option<u32>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            rtk_enabled: true,
            caveman_enabled: false,
            ponytail_enabled: false,
            ponytail_level: "full".to_string(),
            pxpipe_enabled: false,
            pxpipe_min_chars: 25_000,
            pxpipe_timeout_ms: 15_000,
            combo_strategy: "fallback".to_string(),
            sticky_round_robin_limit: 3,
            require_api_key: false,
            require_login: false,
            enable_request_logs: false,
            password_hash: String::new(),
            provider_strategies: HashMap::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn partial_json_fills_defaults() {
        let s: Settings = serde_json::from_str(r#"{"rtk_enabled":false}"#).unwrap();
        assert!(!s.rtk_enabled);
        assert_eq!(s.sticky_round_robin_limit, 3);
        assert_eq!(s.combo_strategy, "fallback");
    }
}
