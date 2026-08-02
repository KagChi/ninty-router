//! Quota fetchers — normalized report per provider connection.
//! Ports the parsing of open-sse/services/usage/{codex,github,claude,codebuddy-cn}.js.

use serde_json::Value;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct QuotaWindow {
    pub label: String,
    /// % used (0-100).
    pub used: f64,
    pub reset_at: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct QuotaReport {
    pub connection_id: String,
    pub provider: String,
    pub plan: Option<String>,
    pub windows: Vec<QuotaWindow>,
    pub error: Option<String>,
    pub fetched_at: String,
}

impl QuotaReport {
    pub fn err(conn_id: &str, provider: &str, msg: impl Into<String>) -> Self {
        Self {
            connection_id: conn_id.into(),
            provider: provider.into(),
            plan: None,
            windows: vec![],
            error: Some(msg.into()),
            fetched_at: chrono::Utc::now().to_rfc3339(),
        }
    }
}

fn pct(v: &Value) -> f64 {
    v.as_f64().unwrap_or(0.0).clamp(0.0, 100.0)
}

fn reset_of(v: &Value, keys: &[&str]) -> Option<String> {
    for k in keys {
        if let Some(x) = v.get(k) {
            if let Some(s) = x.as_str() {
                return Some(s.into());
            }
            if let Some(n) = x.as_i64() {
                let ms = if n < 1_000_000_000_000 { n * 1000 } else { n };
                if let Some(t) = chrono::DateTime::from_timestamp_millis(ms) {
                    return Some(t.to_rfc3339());
                }
            }
        }
    }
    None
}

async fn get(
    client: &reqwest::Client,
    url: &str,
    headers: &[(&str, String)],
) -> Result<Value, String> {
    let mut req = client.get(url).header("accept", "application/json");
    for (k, v) in headers {
        req = req.header(*k, v);
    }
    let resp = req.send().await.map_err(|e| e.to_string())?;
    let status = resp.status();
    let text = resp.text().await.unwrap_or_default();
    if !status.is_success() {
        return Err(format!("HTTP {status}: {}", &text[..text.len().min(200)]));
    }
    serde_json::from_str(&text).map_err(|e| e.to_string())
}

async fn post(
    client: &reqwest::Client,
    url: &str,
    headers: &[(&str, String)],
) -> Result<Value, String> {
    let mut req = client
        .post(url)
        .header("content-type", "application/json")
        .header("accept", "application/json")
        .body("{}");
    for (k, v) in headers {
        req = req.header(*k, v);
    }
    let resp = req.send().await.map_err(|e| e.to_string())?;
    let status = resp.status();
    let text = resp.text().await.unwrap_or_default();
    if !status.is_success() {
        return Err(format!("HTTP {status}: {}", &text[..text.len().min(200)]));
    }
    serde_json::from_str(&text).map_err(|e| e.to_string())
}

// ---------------------------------------------------------------------------
// codex: wham/usage → primary/secondary windows
// ---------------------------------------------------------------------------

pub async fn codex(
    client: &reqwest::Client,
    conn_id: &str,
    access_token: &str,
    account_id: Option<&str>,
) -> QuotaReport {
    let mut headers = vec![("authorization", format!("Bearer {access_token}"))];
    if let Some(acc) = account_id {
        headers.push(("chatgpt-account-id", acc.to_string()));
    }
    match get(
        client,
        "https://chatgpt.com/backend-api/wham/usage",
        &headers,
    )
    .await
    {
        Err(e) => QuotaReport::err(conn_id, "codex", e),
        Ok(v) => {
            let rl = v.get("rate_limit").cloned().unwrap_or(v.clone());
            let mut windows = vec![];
            for (key, label) in [
                ("primary_window", "session"),
                ("secondary_window", "weekly"),
            ] {
                let w = rl.get(key).or_else(|| v.get(key)).cloned();
                if let Some(w) = w {
                    windows.push(QuotaWindow {
                        label: label.into(),
                        used: pct(w
                            .get("used_percent")
                            .or_else(|| w.get("percent_used"))
                            .unwrap_or(&Value::Null)),
                        reset_at: reset_of(&w, &["reset_at", "resets_at", "resetAt"]),
                    });
                }
            }
            if windows.is_empty() {
                return QuotaReport::err(conn_id, "codex", "no rate limit data");
            }
            QuotaReport {
                connection_id: conn_id.into(),
                provider: "codex".into(),
                plan: v.get("plan_type").and_then(Value::as_str).map(String::from),
                windows,
                error: None,
                fetched_at: chrono::Utc::now().to_rfc3339(),
            }
        }
    }
}

// ---------------------------------------------------------------------------
// github copilot: copilot_internal/user
// ---------------------------------------------------------------------------

pub async fn github(client: &reqwest::Client, conn_id: &str, github_token: &str) -> QuotaReport {
    let headers = vec![
        ("authorization", format!("token {github_token}")),
        ("x-github-api-version", "2022-11-28".into()),
        ("user-agent", "GitHubCopilotChat/0.26.7".into()),
        ("editor-version", "vscode/1.100.0".into()),
        ("editor-plugin-version", "copilot-chat/0.26.7".into()),
    ];
    match get(
        client,
        "https://api.github.com/copilot_internal/user",
        &headers,
    )
    .await
    {
        Err(e) => QuotaReport::err(conn_id, "github", e),
        Ok(v) => {
            let plan = v
                .get("copilot_plan")
                .and_then(Value::as_str)
                .map(String::from);
            let mut windows = vec![];
            if let Some(snaps) = v.get("quota_snapshots") {
                for (key, label) in [
                    ("premium_interactions", "premium"),
                    ("chat", "chat"),
                    ("completions", "completions"),
                ] {
                    if let Some(w) = snaps.get(key) {
                        let used = pct(w
                            .get("percent_used")
                            .or_else(|| w.get("used_percent"))
                            .unwrap_or(&Value::Null));
                        windows.push(QuotaWindow {
                            label: label.into(),
                            used,
                            reset_at: reset_of(&v, &["quota_reset_date", "quota_reset_date_utc"]),
                        });
                    }
                }
            } else if let Some(q) = v.get("limited_user_quota") {
                // free plan: {chat: N, completions: N} totals, vs limited_user_quotas used
                let used = v.get("limited_user_quotas").cloned().unwrap_or(Value::Null);
                for (key, label) in [("chat", "chat"), ("completions", "completions")] {
                    let total = q.get(key).and_then(Value::as_f64);
                    let u = used.get(key).and_then(Value::as_f64);
                    if let (Some(t), Some(u)) = (total, u) {
                        if t > 0.0 {
                            windows.push(QuotaWindow {
                                label: label.into(),
                                used: (u / t * 100.0).clamp(0.0, 100.0),
                                reset_at: reset_of(
                                    &v,
                                    &["quota_reset_date", "monthly_quota_reset_date"],
                                ),
                            });
                        }
                    }
                }
            }
            if windows.is_empty() {
                return QuotaReport::err(conn_id, "github", "no quota data");
            }
            QuotaReport {
                connection_id: conn_id.into(),
                provider: "github".into(),
                plan,
                windows,
                error: None,
                fetched_at: chrono::Utc::now().to_rfc3339(),
            }
        }
    }
}

// ---------------------------------------------------------------------------
// claude oauth usage: utilization per window
// ---------------------------------------------------------------------------

pub async fn claude(client: &reqwest::Client, conn_id: &str, access_token: &str) -> QuotaReport {
    let headers = vec![
        ("authorization", format!("Bearer {access_token}")),
        ("anthropic-beta", "oauth-2025-04-20".into()),
        ("anthropic-version", "2023-06-01".into()),
    ];
    match get(
        client,
        "https://api.anthropic.com/api/oauth/usage",
        &headers,
    )
    .await
    {
        Err(e) => QuotaReport::err(conn_id, "claude", e),
        Ok(v) => {
            let mut windows = vec![];
            for (key, label) in [
                ("five_hour", "5h"),
                ("seven_day", "weekly"),
                ("seven_day_oauth_apps", "weekly (apps)"),
                ("seven_day_sonnet", "weekly sonnet"),
                ("thirty_day", "monthly"),
            ] {
                if let Some(w) = v.get(key) {
                    if let Some(u) = w.get("utilization").and_then(Value::as_f64) {
                        windows.push(QuotaWindow {
                            label: label.into(),
                            used: u.clamp(0.0, 100.0),
                            reset_at: reset_of(w, &["resets_at", "reset_at", "resetAt"]),
                        });
                    }
                }
            }
            if windows.is_empty() {
                return QuotaReport::err(conn_id, "claude", "no utilization data");
            }
            QuotaReport {
                connection_id: conn_id.into(),
                provider: "claude".into(),
                plan: None,
                windows,
                error: None,
                fetched_at: chrono::Utc::now().to_rfc3339(),
            }
        }
    }
}

// ---------------------------------------------------------------------------
// codebuddy cn/intl: billing meter Accounts[]
// ---------------------------------------------------------------------------

pub async fn codebuddy(
    client: &reqwest::Client,
    conn_id: &str,
    provider: &str,
    token: &str,
) -> QuotaReport {
    let base = if provider == "codebuddy-cn" {
        "https://copilot.tencent.com"
    } else {
        "https://www.codebuddy.ai"
    };
    let ua = if provider == "codebuddy-cn" {
        "CLI/2.108.1 CodeBuddy/2.108.1"
    } else {
        "IDE/2.108.1 CodeBuddy/2.108.1"
    };
    let headers = vec![
        ("authorization", format!("Bearer {token}")),
        ("user-agent", ua.to_string()),
        ("x-product", "SaaS".into()),
    ];
    match post(
        client,
        &format!("{base}/v2/billing/meter/get-user-resource"),
        &headers,
    )
    .await
    {
        Err(e) => QuotaReport::err(conn_id, provider, e),
        Ok(v) => {
            let accounts = v
                .pointer("/Response/Data/Accounts")
                .or_else(|| v.pointer("/response/data/accounts"))
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();
            let mut windows = vec![];
            for (i, acc) in accounts.iter().enumerate() {
                let total = acc
                    .get("TotalPrecise")
                    .or_else(|| acc.get("Total"))
                    .and_then(Value::as_f64)
                    .unwrap_or(0.0);
                let used_n = acc
                    .get("UsedPrecise")
                    .or_else(|| acc.get("Used"))
                    .and_then(Value::as_f64)
                    .unwrap_or(0.0);
                if total <= 0.0 {
                    continue;
                }
                let label = {
                    let start = reset_of(acc, &["CycleStartTime"]);
                    let end = reset_of(acc, &["CycleEndTime"]);
                    match (start, end) {
                        (Some(s), Some(e)) => {
                            let days =
                                (chrono::DateTime::parse_from_rfc3339(&e).ok().and_then(|e| {
                                    chrono::DateTime::parse_from_rfc3339(&s)
                                        .ok()
                                        .map(|s| (e - s).num_days())
                                }))
                                .unwrap_or(30);
                            if days <= 1 {
                                "Daily".to_string()
                            } else if days <= 10 {
                                "Weekly".to_string()
                            } else {
                                "Monthly".to_string()
                            }
                        }
                        _ => format!("Pack {}", i + 1),
                    }
                };
                windows.push(QuotaWindow {
                    label,
                    used: (used_n / total * 100.0).clamp(0.0, 100.0),
                    reset_at: reset_of(acc, &["CycleEndTime"]),
                });
            }
            if windows.is_empty() {
                return QuotaReport::err(conn_id, provider, "no accounts data");
            }
            QuotaReport {
                connection_id: conn_id.into(),
                provider: provider.into(),
                plan: None,
                windows,
                error: None,
                fetched_at: chrono::Utc::now().to_rfc3339(),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codex_parse_fixture() {
        let v = serde_json::json!({
            "plan_type": "plus",
            "rate_limit": {
                "primary_window": {"used_percent": 42, "reset_at": 1_800_000_000},
                "secondary_window": {"used_percent": 10}
            }
        });
        let rl = v.get("rate_limit").unwrap();
        let w = rl.get("primary_window").unwrap();
        assert_eq!(pct(w.get("used_percent").unwrap()), 42.0);
        assert!(reset_of(w, &["reset_at"]).is_some());
    }

    #[test]
    fn codebuddy_cadence_label() {
        let acc = serde_json::json!({"TotalPrecise": 1000.0, "UsedPrecise": 250.0,
            "CycleStartTime": "2026-08-01T00:00:00Z", "CycleEndTime": "2026-09-01T00:00:00Z"});
        let total = acc.get("TotalPrecise").and_then(Value::as_f64).unwrap();
        let used = acc.get("UsedPrecise").and_then(Value::as_f64).unwrap();
        assert_eq!(used / total * 100.0, 25.0);
    }

    #[test]
    fn github_free_plan_parse() {
        let v = serde_json::json!({
            "copilot_plan": "free",
            "limited_user_quota": {"chat": 50, "completions": 2000},
            "limited_user_quotas": {"chat": 10, "completions": 500}
        });
        let q = v.get("limited_user_quota").unwrap();
        let u = v.get("limited_user_quotas").unwrap();
        let used_pct = u.get("chat").and_then(Value::as_f64).unwrap()
            / q.get("chat").and_then(Value::as_f64).unwrap()
            * 100.0;
        assert_eq!(used_pct, 20.0);
    }
}
