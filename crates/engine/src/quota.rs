//! Quota fetchers — normalized report per provider connection.
//! Ports open-sse/services/usage/*.js (9router) — raw numbers; the UI computes
//! remaining percentages.

use serde_json::Value;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct QuotaWindow {
    pub label: String,
    pub used: f64,
    pub total: f64,
    /// total == 0 upstream means unlimited.
    #[serde(default)]
    pub unlimited: bool,
    /// Remaining % (0-100) when upstream reports it directly.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub remaining: Option<f64>,
    /// true = allowance replenishes at reset_at ("resets in");
    /// false = one-shot credits that expire for good ("expires in").
    #[serde(default = "default_recurring")]
    pub recurring: bool,
    pub reset_at: Option<String>,
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
    /// 9router calls this `message` — info/error text instead of windows.
    pub error: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub extra: Option<Value>,
    /// Card display: name || email || displayName.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    /// Secondary line (email/displayName when distinct from label).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub secondary: Option<String>,
    #[serde(default = "default_active")]
    pub active: bool,
    #[serde(default)]
    pub priority: i64,
    pub fetched_at: String,
}

fn default_active() -> bool {
    true
}

impl QuotaReport {
    pub fn err(conn_id: &str, provider: &str, msg: impl Into<String>) -> Self {
        Self {
            connection_id: conn_id.into(),
            provider: provider.into(),
            plan: None,
            windows: vec![],
            error: Some(msg.into()),
            extra: None,
            label: None,
            secondary: None,
            active: true,
            priority: 0,
            fetched_at: chrono::Utc::now().to_rfc3339(),
        }
    }

    fn ok(conn_id: &str, provider: &str, plan: Option<String>, windows: Vec<QuotaWindow>) -> Self {
        Self {
            connection_id: conn_id.into(),
            provider: provider.into(),
            plan,
            windows,
            error: None,
            extra: None,
            label: None,
            secondary: None,
            active: true,
            priority: 0,
            fetched_at: chrono::Utc::now().to_rfc3339(),
        }
    }
}

fn window(
    label: impl Into<String>,
    used: f64,
    total: f64,
    reset_at: Option<String>,
    recurring: bool,
) -> QuotaWindow {
    QuotaWindow {
        label: label.into(),
        used,
        total,
        unlimited: total <= 0.0,
        remaining: None,
        recurring,
        reset_at,
    }
}

fn num(v: &Value) -> f64 {
    v.as_f64()
        .or_else(|| v.as_str()?.parse::<f64>().ok())
        .unwrap_or(0.0)
}

fn field(v: &Value, keys: &[&str]) -> f64 {
    for k in keys {
        if let Some(x) = v.get(k) {
            return num(x);
        }
    }
    0.0
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
                    let used = field(&w, &["used_percent", "percent_used"]).clamp(0.0, 100.0);
                    windows.push(window(
                        label,
                        used,
                        100.0,
                        reset_of(&w, &["reset_at", "resets_at", "resetAt"]),
                        true,
                    ));
                }
            }
            if windows.is_empty() {
                return QuotaReport::err(conn_id, "codex", "no rate limit data");
            }
            QuotaReport::ok(
                conn_id,
                "codex",
                v.get("plan_type")
                    .and_then(Value::as_str)
                    .map(String::from),
                windows,
            )
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
                .or_else(|| v.get("access_type_sku"))
                .and_then(Value::as_str)
                .map(String::from);
            let mut windows = vec![];
            if let Some(snaps) = v.get("quota_snapshots") {
                // Paid: entitlement vs remaining per snapshot.
                for (key, label) in [
                    ("premium_interactions", "premium"),
                    ("chat", "chat"),
                    ("completions", "completions"),
                ] {
                    if let Some(w) = snaps.get(key) {
                        let total = field(w, &["entitlement"]);
                        if total <= 0.0 && !w.get("unlimited").and_then(Value::as_bool).unwrap_or(false) {
                            continue;
                        }
                        let remaining = field(w, &["remaining"]);
                        let unlimited = w
                            .get("unlimited")
                            .and_then(Value::as_bool)
                            .unwrap_or(false);
                        let mut qw = window(
                            label,
                            (total - remaining).max(0.0),
                            total,
                            reset_of(&v, &["quota_reset_date", "quota_reset_date_utc"]),
                            true,
                        );
                        qw.unlimited = unlimited;
                        windows.push(qw);
                    }
                }
            } else if v.get("monthly_quotas").is_some() || v.get("limited_user_quota").is_some() {
                // Free plan: monthly_quotas totals vs limited_user_quotas used.
                let totals = v
                    .get("monthly_quotas")
                    .or_else(|| v.get("limited_user_quota"))
                    .cloned()
                    .unwrap_or(Value::Null);
                let used = v.get("limited_user_quotas").cloned().unwrap_or(Value::Null);
                for (key, label) in [("chat", "chat"), ("completions", "completions")] {
                    let total = field(&totals, &[key]);
                    let u = field(&used, &[key]);
                    if total > 0.0 {
                        windows.push(window(
                            label,
                            u,
                            total,
                            reset_of(
                                &v,
                                &[
                                    "limited_user_reset_date",
                                    "quota_reset_date",
                                    "monthly_quota_reset_date",
                                ],
                            ),
                            true,
                        ));
                    }
                }
            }
            if windows.is_empty() {
                return QuotaReport::err(conn_id, "github", "no quota data");
            }
            QuotaReport::ok(conn_id, "github", plan, windows)
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
            let mut push = |label: String, w: &Value| {
                if let Some(u) = w.get("utilization").and_then(Value::as_f64) {
                    windows.push(window(
                        label,
                        u.clamp(0.0, 100.0),
                        100.0,
                        reset_of(w, &["resets_at", "reset_at", "resetAt"]),
                        true,
                    ));
                }
            };
            if let Some(w) = v.get("five_hour") {
                push("session (5h)".into(), w);
            }
            if let Some(w) = v.get("seven_day") {
                push("weekly (7d)".into(), w);
            }
            // Model-specific seven_day_* buckets first (9router orders them after main).
            if let Some(obj) = v.as_object() {
                for (k, w) in obj {
                    if k.starts_with("seven_day_") && w.is_object() {
                        let model = k.trim_start_matches("seven_day_").replace('_', " ");
                        push(format!("weekly {model} (7d)"), w);
                    }
                }
            }
            if let Some(w) = v.get("thirty_day") {
                push("monthly (30d)".into(), w);
            }
            if windows.is_empty() {
                return QuotaReport::err(conn_id, "claude", "no utilization data");
            }
            let mut r = QuotaReport::ok(conn_id, "claude", Some("Claude Code".into()), windows);
            r.extra = v.get("extra_usage").cloned();
            r
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
            .or_else(|| {
                x.as_str()
                    .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
                    .map(|t| t.timestamp_millis())
            })
    };
    let cycle_end_ms = |acc: &Value| -> Option<i64> {
        acc.get("CycleEndTime").and_then(ts_ms)
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
        let used = field(acc, &["CycleCapacityUsedPrecise", "CycleCapacityUsed"]);
        let total = field(acc, &["CycleCapacitySizePrecise", "CycleCapacitySize"]);
        if total <= 0.0 {
            continue;
        }
        windows.push(window(
            label,
            used,
            total,
            reset_of(acc, &["CycleEndTime"]),
            true,
        ));
    }
    for (i, acc) in bonuses.iter().enumerate() {
        let used = field(acc, &["CapacityUsedPrecise", "CapacityUsed"]);
        let total = field(acc, &["CapacitySizePrecise", "CapacitySize"]);
        if total <= 0.0 {
            continue;
        }
        windows.push(window(
            format!("Bonus Pack {}", i + 1),
            used,
            total,
            reset_of(acc, &["CycleEndTime"]),
            false,
        ));
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
    QuotaReport::ok(conn_id, provider, Some(plan), windows)
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

// ---------------------------------------------------------------------------
// deepseek: user/balance
// ---------------------------------------------------------------------------

pub async fn deepseek(client: &reqwest::Client, conn_id: &str, api_key: &str) -> QuotaReport {
    let headers = vec![("authorization", format!("Bearer {api_key}"))];
    match get(client, "https://api.deepseek.com/user/balance", &headers).await {
        Err(e) => {
            if e.starts_with("HTTP 401") || e.starts_with("HTTP 403") {
                QuotaReport::err(conn_id, "deepseek", "authentication failed")
            } else {
                QuotaReport::err(conn_id, "deepseek", e)
            }
        }
        Ok(v) => {
            let mut windows = vec![];
            if let Some(infos) = v.get("balance_infos").and_then(Value::as_array) {
                for b in infos {
                    let ccy = b
                        .get("currency")
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .to_string();
                    let total = field(b, &["total_balance", "totalBalance"]).max(0.0);
                    let mut w = window(format!("Balance ({ccy})"), 0.0, total, None, false);
                    w.unlimited = total > 0.0;
                    windows.push(w);
                }
            }
            if windows.is_empty() {
                return QuotaReport::err(conn_id, "deepseek", "no balance data");
            }
            let available = v
                .get("is_available")
                .and_then(Value::as_bool)
                .unwrap_or(true);
            let plan = if available {
                "DeepSeek"
            } else {
                "DeepSeek (Insufficient Balance)"
            };
            QuotaReport::ok(conn_id, "deepseek", Some(plan.into()), windows)
        }
    }
}

// ---------------------------------------------------------------------------
// glm / glm-cn: monitor/usage/quota/limit
// ---------------------------------------------------------------------------

pub async fn glm(client: &reqwest::Client, conn_id: &str, provider: &str, api_key: &str) -> QuotaReport {
    let base = if provider == "glm-cn" {
        "https://open.bigmodel.cn"
    } else {
        "https://api.z.ai"
    };
    let headers = vec![("authorization", format!("Bearer {api_key}"))];
    match get(
        client,
        &format!("{base}/api/monitor/usage/quota/limit"),
        &headers,
    )
    .await
    {
        Err(e) => QuotaReport::err(conn_id, provider, e),
        Ok(v) => {
            let mut windows = vec![];
            if let Some(limits) = v.pointer("/data/limits").and_then(Value::as_array) {
                for l in limits {
                    if l.get("type").and_then(Value::as_str) != Some("TOKENS_LIMIT") {
                        continue;
                    }
                    let used = field(l, &["percentage"]).clamp(0.0, 100.0);
                    windows.push(window(
                        "session",
                        used,
                        100.0,
                        reset_of(l, &["nextResetTime"]),
                        true,
                    ));
                }
            }
            if windows.is_empty() {
                return QuotaReport::err(conn_id, provider, "no limits data");
            }
            let level = v
                .pointer("/data/level")
                .and_then(Value::as_str)
                .map(|s| {
                    let mut c = s.chars();
                    match c.next() {
                        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
                        None => s.to_string(),
                    }
                });
            QuotaReport::ok(conn_id, provider, level, windows)
        }
    }
}

// ---------------------------------------------------------------------------
// minimax / minimax-cn: token_plan/remains (+ coding_plan fallback)
// ---------------------------------------------------------------------------

pub async fn minimax(
    client: &reqwest::Client,
    conn_id: &str,
    provider: &str,
    api_key: &str,
) -> QuotaReport {
    let urls: &[&str] = if provider == "minimax-cn" {
        &[
            "https://www.minimaxi.com/v1/api/openplatform/coding_plan/remains",
            "https://api.minimaxi.com/v1/api/openplatform/coding_plan/remains",
        ]
    } else {
        &[
            "https://www.minimax.io/v1/token_plan/remains",
            "https://api.minimax.io/v1/api/openplatform/coding_plan/remains",
        ]
    };
    let headers = vec![("authorization", format!("Bearer {api_key}"))];
    let mut last_err = String::new();
    for url in urls {
        match get(client, url, &headers).await {
            Ok(v) => {
                if v.pointer("/base_resp/status_code").and_then(Value::as_i64) == Some(1004) {
                    return QuotaReport::err(conn_id, provider, "invalid API key");
                }
                let r = parse_minimax(conn_id, provider, &v, url.contains("/coding_plan/"));
                if r.error.is_none() {
                    return r;
                }
                last_err = r.error.clone().unwrap_or_default();
            }
            Err(e) => last_err = e,
        }
    }
    QuotaReport::err(conn_id, provider, last_err)
}

fn parse_minimax(conn_id: &str, provider: &str, v: &Value, count_means_remaining: bool) -> QuotaReport {
    let remains = v
        .get("model_remains")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    if remains.is_empty() {
        return QuotaReport::err(conn_id, provider, "no quota data");
    }
    let mut windows = vec![];
    for m in remains {
        let raw_name = m
            .get("model_name")
            .and_then(Value::as_str)
            .unwrap_or("general");
        let name = if raw_name.starts_with("MiniMax-M") || raw_name == "general" {
            "M-series".to_string()
        } else {
            raw_name.to_string()
        };
        let mut pair = |usage_key: &str, total_key: &str, reset_keys: &[&str], suffix: &str| {
            let total = field(&m, &[total_key]);
            if total <= 0.0 {
                return;
            }
            let raw = field(&m, &[usage_key]);
            let used = if count_means_remaining {
                (total - raw).max(0.0)
            } else {
                raw
            };
            windows.push(window(
                format!("{name} {suffix}"),
                used,
                total,
                reset_of(&m, reset_keys),
                true,
            ));
        };
        pair(
            "current_interval_usage_count",
            "current_interval_total_count",
            &["end_time", "remains_time"],
            "(5h)",
        );
        pair(
            "weekly_usage_count",
            "weekly_total_count",
            &["weekly_end_time", "weekly_remains_time"],
            "(7d)",
        );
    }
    if windows.is_empty() {
        return QuotaReport::err(conn_id, provider, "no quota data");
    }
    QuotaReport::ok(conn_id, provider, None, windows)
}

// ---------------------------------------------------------------------------
// kimi: coding/v1/usages
// ---------------------------------------------------------------------------

pub async fn kimi(
    client: &reqwest::Client,
    conn_id: &str,
    access_token: &str,
    api_key: &str,
    device_id: &str,
) -> QuotaReport {
    let mut headers: Vec<(&str, String)> = vec![];
    if !api_key.is_empty() {
        headers.push(("x-api-key", api_key.to_string()));
    } else {
        headers.push(("authorization", format!("Bearer {access_token}")));
        headers.push(("x-msh-platform", "ninty-router".into()));
        headers.push(("x-msh-version", env!("CARGO_PKG_VERSION").into()));
        headers.push(("x-msh-device-name", "ninty-router".into()));
        headers.push(("x-msh-device-model", std::env::consts::OS.into()));
        headers.push(("x-msh-device-id", device_id.to_string()));
    }
    match get(client, "https://api.kimi.com/coding/v1/usages", &headers).await {
        Err(e) => QuotaReport::err(conn_id, "kimi", e),
        Ok(v) => {
            let mut windows = vec![];
            if let Some(u) = v.pointer("/data/usage") {
                let total = field(u, &["limit"]);
                if total > 0.0 {
                    windows.push(window(
                        "Weekly",
                        field(u, &["used"]),
                        total,
                        reset_of(u, &["resetTime", "ResetTime", "reset_at", "resetAt"]),
                        true,
                    ));
                }
            }
            if let Some(limits) = v.pointer("/data/limits").and_then(Value::as_array) {
                for l in limits {
                    let detail = l.get("detail").cloned().unwrap_or(Value::Null);
                    let total = field(&detail, &["limit"]);
                    if total > 0.0 {
                        let remaining = field(&detail, &["remaining"]);
                        windows.push(window(
                            "Ratelimit",
                            (total - remaining).max(0.0),
                            total,
                            reset_of(&detail, &["resetTime"]),
                            true,
                        ));
                    }
                }
            }
            let level = v
                .pointer("/data/user/membership/level")
                .and_then(Value::as_str);
            let plan = match level {
                Some("LEVEL_BASIC") => "Moderato",
                Some("LEVEL_INTERMEDIATE") => "Allegretto",
                Some("LEVEL_ADVANCED") => "Allegro",
                Some("LEVEL_STANDARD") => "Vivace",
                _ => "Kimi Coding",
            };
            if windows.is_empty() {
                let mut r = QuotaReport::err(conn_id, "kimi", "no usage data");
                r.plan = Some(plan.into());
                return r;
            }
            QuotaReport::ok(conn_id, "kimi", Some(plan.into()), windows)
        }
    }
}

// ---------------------------------------------------------------------------
// qoder: v2/quota/usage
// ---------------------------------------------------------------------------

pub async fn qoder(client: &reqwest::Client, conn_id: &str, access_token: &str) -> QuotaReport {
    let headers = vec![("authorization", format!("Bearer {access_token}"))];
    match get(client, "https://openapi.qoder.sh/api/v2/quota/usage", &headers).await {
        Err(e) => QuotaReport::err(conn_id, "qoder", e),
        Ok(v) => {
            let body = v.get("body").cloned().unwrap_or(v.clone());
            let expires = reset_of(&body, &["expiresAt"]);
            let mut windows = vec![];
            if let Some(uq) = body.get("userQuota") {
                let total = field(uq, &["total"]);
                if total > 0.0 {
                    windows.push(window(
                        "Personal",
                        field(uq, &["used"]),
                        total,
                        expires.clone(),
                        true,
                    ));
                }
            }
            if let Some(org) = body.get("orgResourcePackage") {
                let total = field(org, &["total"]);
                if total > 0.0 {
                    windows.push(window(
                        "Organization",
                        field(org, &["used"]),
                        total,
                        expires,
                        true,
                    ));
                }
            }
            if windows.is_empty() {
                return QuotaReport::err(conn_id, "qoder", "no quota data");
            }
            QuotaReport::ok(conn_id, "qoder", None, windows)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codex_parse_fixture() {
        let w = serde_json::json!({"used_percent": 42, "reset_at": 1_800_000_000});
        assert_eq!(field(&w, &["used_percent"]), 42.0);
        assert!(reset_of(&w, &["reset_at"]).is_some());
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
        assert!((r.windows[0].used - 6.54).abs() < 0.001);
        assert_eq!(r.windows[0].total, 500.0);
        assert_eq!(r.windows[1].label, "Bonus Pack 1");
        assert!(!r.windows[1].recurring);
        assert_eq!(r.windows[1].used, 10.0);
        assert_eq!(r.windows[1].total, 100.0);
    }

    #[test]
    fn codebuddy_envelope_error() {
        let v = serde_json::json!({"code": 40100, "msg": "invalid token"});
        let r = parse_codebuddy("c1", "codebuddy-cn", &v);
        assert_eq!(r.error.as_deref(), Some("quota error: invalid token"));
    }

    #[test]
    fn minimax_parse_fixture() {
        let v = serde_json::json!({
            "model_remains": [{
                "model_name": "MiniMax-M3",
                "current_interval_usage_count": 40,
                "current_interval_total_count": 100,
                "end_time": 1_800_000_000_000i64,
                "weekly_usage_count": 200,
                "weekly_total_count": 1000,
                "weekly_end_time": 1_800_500_000_000i64
            }]
        });
        let r = parse_minimax("c1", "minimax", &v, false);
        assert!(r.error.is_none());
        assert_eq!(r.windows.len(), 2);
        assert_eq!(r.windows[0].label, "M-series (5h)");
        assert_eq!(r.windows[0].used, 40.0);
        assert_eq!(r.windows[0].total, 100.0);
        assert_eq!(r.windows[1].label, "M-series (7d)");
    }

    #[test]
    fn github_free_plan_fields() {
        let totals = serde_json::json!({"chat": 50, "completions": 2000});
        let used = serde_json::json!({"chat": 10, "completions": 500});
        assert_eq!(field(&totals, &["chat"]), 50.0);
        assert_eq!(field(&used, &["completions"]), 500.0);
    }
}
