//! Quota fetchers — normalized report per provider connection.
//! Ports the parsing of open-sse/services/usage/{codex,github,claude,codebuddy-cn}.js.

use serde_json::Value;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct QuotaWindow {
    pub label: String,
    /// % used (0-100).
    pub used: f64,
    pub reset_at: Option<String>,
    /// true = allowance replenishes at reset_at ("resets in");
    /// false = one-shot credits that expire for good ("expires in").
    #[serde(default = "default_recurring")]
    pub recurring: bool,
}

fn default_recurring() -> bool {
    true
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
                // Numeric strings are unix epochs (sec or ms), like 9router parseResetTime.
                if !s.is_empty() && s.chars().all(|c| c.is_ascii_digit()) {
                    if let Ok(n) = s.parse::<i64>() {
                        let ms = if n < 1_000_000_000_000 { n * 1000 } else { n };
                        if let Some(t) = chrono::DateTime::from_timestamp_millis(ms) {
                            return Some(t.to_rfc3339());
                        }
                    }
                }
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
                        recurring: true,
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
                            recurring: true,
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
                                recurring: true,
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
                            recurring: true,
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
    let (ua, ide) = if provider == "codebuddy-cn" {
        ("CLI/2.108.1 CodeBuddy/2.108.1", "CLI")
    } else {
        ("IDE/2.108.1 CodeBuddy/2.108.1", "IDE")
    };
    let headers = vec![
        ("authorization", format!("Bearer {token}")),
        ("user-agent", ua.to_string()),
        ("x-product", "SaaS".into()),
        ("x-ide-type", ide.into()),
        ("x-ide-name", ide.into()),
        ("x-requested-with", "XMLHttpRequest".into()),
        ("x-codebuddy-request", "1".into()),
    ];
    match post(
        client,
        &format!("{base}/v2/billing/meter/get-user-resource"),
        &headers,
    )
    .await
    {
        Err(e) => QuotaReport::err(conn_id, provider, e),
        Ok(v) => parse_codebuddy(conn_id, provider, &v),
    }
}

/// Parse the Tencent billing payload. Two credit types must NOT be merged:
/// - refill/base packs: cycle resets long before the resource expires
///   (DeductionEndTime - CycleEndTime > 2d) → *Cycle* fields, recurring.
/// - bonus packs: one-shot credits (CycleEndTime == DeductionEndTime) → plain
///   Capacity fields, non-recurring, resetAt = expiry.
///
/// Port of open-sse/services/usage/codebuddy-cn.js.
fn parse_codebuddy(conn_id: &str, provider: &str, v: &Value) -> QuotaReport {
    if v.get("code").and_then(Value::as_i64).unwrap_or(0) != 0 {
        let msg = v
            .get("msg")
            .and_then(Value::as_str)
            .unwrap_or("unknown")
            .to_string();
        return QuotaReport::err(conn_id, provider, format!("quota error: {msg}"));
    }
    let accounts = v
        .pointer("/data/Response/Data/Accounts")
        .or_else(|| v.pointer("/Data/Response/Data/Accounts"))
        .or_else(|| v.pointer("/Response/Data/Accounts"))
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    if accounts.is_empty() {
        return QuotaReport::err(conn_id, provider, "no credit package found");
    }

    const REFILL_GAP_MS: i64 = 2 * 24 * 60 * 60 * 1000;
    let ts_ms = |x: &Value| -> Option<i64> {
        if let Some(n) = x.as_i64() {
            return Some(if n < 1_000_000_000_000 { n * 1000 } else { n });
        }
        x.as_str()
            .filter(|s| !s.is_empty() && s.chars().all(|c| c.is_ascii_digit()))
            .and_then(|s| s.parse::<i64>().ok())
            .map(|n| if n < 1_000_000_000_000 { n * 1000 } else { n })
    };
    let cycle_end_ms = |acc: &Value| -> Option<i64> {
        acc.get("CycleEndTime").and_then(ts_ms).or_else(|| {
            // rfc3339 string
            acc.get("CycleEndTime")
                .and_then(Value::as_str)
                .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
                .map(|t| t.timestamp_millis())
        })
    };
    let is_refill = |acc: &Value| -> bool {
        match (
            cycle_end_ms(acc),
            acc.get("DeductionEndTime").and_then(ts_ms),
        ) {
            (Some(ce), Some(de)) => de - ce > REFILL_GAP_MS,
            _ => false,
        }
    };

    let mut refills: Vec<&Value> = accounts.iter().filter(|a| is_refill(a)).collect();
    let mut bonuses: Vec<&Value> = accounts.iter().filter(|a| !is_refill(a)).collect();
    let by_expiry = |a: &&Value, b: &&Value| {
        cycle_end_ms(a)
            .unwrap_or(i64::MAX)
            .cmp(&cycle_end_ms(b).unwrap_or(i64::MAX))
    };
    refills.sort_by(by_expiry);
    bonuses.sort_by(by_expiry);

    let num = |acc: &Value, precise: &str, plain: &str| -> f64 {
        acc.get(precise)
            .or_else(|| acc.get(plain))
            .and_then(|x| x.as_f64().or_else(|| x.as_str()?.parse::<f64>().ok()))
            .unwrap_or(0.0)
    };

    let mut windows = vec![];
    let mut seen: std::collections::HashMap<String, u32> = std::collections::HashMap::new();
    for acc in &refills {
        let base_label = cadence_label(acc);
        let n = seen.entry(base_label.clone()).or_insert(0);
        *n += 1;
        let label = if *n > 1 {
            format!("{base_label} {n}")
        } else {
            base_label
        };
        let used = num(acc, "CycleCapacityUsedPrecise", "CycleCapacityUsed");
        let total = num(acc, "CycleCapacitySizePrecise", "CycleCapacitySize");
        if total <= 0.0 {
            continue;
        }
        windows.push(QuotaWindow {
            label,
            used: (used / total * 100.0).clamp(0.0, 100.0),
            reset_at: reset_of(acc, &["CycleEndTime"]),
            recurring: true,
        });
    }
    for (i, acc) in bonuses.iter().enumerate() {
        let used = num(acc, "CapacityUsedPrecise", "CapacityUsed");
        let total = num(acc, "CapacitySizePrecise", "CapacitySize");
        if total <= 0.0 {
            continue;
        }
        windows.push(QuotaWindow {
            label: format!("Bonus Pack {}", i + 1),
            used: (used / total * 100.0).clamp(0.0, 100.0),
            reset_at: reset_of(acc, &["CycleEndTime"]),
            recurring: false,
        });
    }
    if windows.is_empty() {
        return QuotaReport::err(conn_id, provider, "no accounts data");
    }

    let default_name = if provider == "codebuddy-cn" {
        "CodeBuddy CN"
    } else {
        "CodeBuddy"
    };
    let plan = refills
        .first()
        .copied()
        .or_else(|| accounts.first())
        .and_then(|acc| {
            acc.get("PackageName")
                .or_else(|| acc.get("SubProductName"))
                .and_then(Value::as_str)
        })
        .unwrap_or(default_name)
        .to_string();
    QuotaReport {
        connection_id: conn_id.into(),
        provider: provider.into(),
        plan: Some(plan),
        windows,
        error: None,
        fetched_at: chrono::Utc::now().to_rfc3339(),
    }
}

/// Cadence label from cycle length (Monthly is the common CodeBuddy case).
fn cadence_label(acc: &Value) -> String {
    let parse = |key: &str| {
        acc.get(key).and_then(|x| {
            x.as_str().and_then(|s| {
                chrono::DateTime::parse_from_rfc3339(s)
                    .ok()
                    .or_else(|| {
                        s.parse::<i64>().ok().and_then(|n| {
                            let ms = if n < 1_000_000_000_000 { n * 1000 } else { n };
                            chrono::DateTime::from_timestamp_millis(ms)
                                .map(|t| t.fixed_offset())
                        })
                    })
            })
        })
    };
    match (parse("CycleStartTime"), parse("CycleEndTime")) {
        (Some(s), Some(e)) => {
            let days = (e - s).num_milliseconds() as f64 / 86_400_000.0;
            if days <= 1.5 {
                "Daily".to_string()
            } else if days <= 10.0 {
                "Weekly".to_string()
            } else {
                "Monthly".to_string()
            }
        }
        _ => "Pack".to_string(),
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
    fn codebuddy_parse_fixture() {
        // Refill: cycle ends 2026-09-01, resource expires 2027-08-01 (>2d gap) → Monthly recurring.
        // Bonus: CycleEndTime == DeductionEndTime → non-recurring "Bonus Pack 1".
        let v = serde_json::json!({
            "code": 0,
            "data": {"Response": {"Data": {"Accounts": [
                {
                    "PackageName": "基础体验包",
                    "CycleStartTime": "2026-08-01T00:00:00Z",
                    "CycleEndTime": "2026-09-01T00:00:00Z",
                    "DeductionEndTime": 1_815_500_000_000i64,
                    "CycleCapacityUsedPrecise": "6.54",
                    "CycleCapacitySizePrecise": "500"
                },
                {
                    "PackageName": "活动赠送包",
                    "CycleStartTime": "2026-08-01T00:00:00Z",
                    "CycleEndTime": 1_756_000_000i64,
                    "DeductionEndTime": 1_756_000_000i64,
                    "CapacityUsedPrecise": "10",
                    "CapacitySizePrecise": "100"
                }
            ]}}}
        });
        let r = parse_codebuddy("c1", "codebuddy-cn", &v);
        assert!(r.error.is_none(), "unexpected error: {:?}", r.error);
        assert_eq!(r.plan.as_deref(), Some("基础体验包"));
        assert_eq!(r.windows.len(), 2);
        assert_eq!(r.windows[0].label, "Monthly");
        assert!(r.windows[0].recurring);
        assert!((r.windows[0].used - 1.308).abs() < 0.001);
        assert_eq!(r.windows[1].label, "Bonus Pack 1");
        assert!(!r.windows[1].recurring);
        assert!((r.windows[1].used - 10.0).abs() < 0.001);
    }

    #[test]
    fn codebuddy_envelope_error() {
        let v = serde_json::json!({"code": 40100, "msg": "invalid token"});
        let r = parse_codebuddy("c1", "codebuddy-cn", &v);
        assert_eq!(r.error.as_deref(), Some("quota error: invalid token"));
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
