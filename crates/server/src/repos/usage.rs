use chrono::Utc;
use ninty_core::error::{Error, Result};

use crate::db::Db;

#[derive(Debug, Clone)]
pub struct UsageRecord {
    pub provider: String,
    pub model: String,
    pub connection_id: Option<String>,
    pub api_key: Option<String>,
    pub endpoint: String,
    pub prompt_tokens: i64,
    pub completion_tokens: i64,
    pub cost: f64,
    pub status: String,
    pub meta: Option<serde_json::Value>,
}

pub async fn record(db: &Db, rec: UsageRecord) -> Result<()> {
    let meta = rec.meta.map(|m| m.to_string());
    db.call(move |conn| {
        conn.execute(
            "INSERT INTO usage_history (ts, provider, model, connection_id, api_key, endpoint, prompt_tokens, completion_tokens, cost, status, meta)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            rusqlite::params![
                Utc::now().to_rfc3339(),
                rec.provider,
                rec.model,
                rec.connection_id,
                rec.api_key,
                rec.endpoint,
                rec.prompt_tokens,
                rec.completion_tokens,
                rec.cost,
                rec.status,
                meta,
            ],
        )
        .map_err(|e| Error::Db(e.to_string()))?;
        Ok(())
    })
    .await
}

/// Sum tokens used by an API key since `since` (RFC3339); None = all time.
pub async fn key_usage_since(db: &Db, api_key: &str, since: Option<String>) -> Result<i64> {
    let key = api_key.to_string();
    db.call(move |conn| {
        let sum: i64 = match &since {
            Some(s) => conn
                .query_row(
                    "SELECT COALESCE(SUM(prompt_tokens + completion_tokens), 0) FROM usage_history WHERE api_key = ?1 AND ts >= ?2",
                    rusqlite::params![key, s],
                    |r| r.get(0),
                )
                .map_err(|e| Error::Db(e.to_string()))?,
            None => conn
                .query_row(
                    "SELECT COALESCE(SUM(prompt_tokens + completion_tokens), 0) FROM usage_history WHERE api_key = ?1",
                    [&key],
                    |r| r.get(0),
                )
                .map_err(|e| Error::Db(e.to_string()))?,
        };
        Ok(sum)
    })
    .await
}

/// Requests by api key in the last 60s (RPM sliding window).
pub async fn rpm_count(db: &Db, api_key: &str) -> Result<i64> {
    let key = api_key.to_string();
    db.call(move |conn| {
        let since = (Utc::now() - chrono::Duration::seconds(60)).to_rfc3339();
        let n: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM usage_history WHERE api_key = ?1 AND ts >= ?2",
                rusqlite::params![key, since],
                |r| r.get(0),
            )
            .map_err(|e| Error::Db(e.to_string()))?;
        Ok(n)
    })
    .await
}

pub struct RequestDetail {
    pub provider: String,
    pub model: String,
    pub status: String,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub endpoint: String,
    /// Extra context merged into the data JSON (request/providerRequest bodies
    /// already truncated by the caller, latency, pxpipe summary, error…).
    pub extra: Option<serde_json::Value>,
}

pub const REQUEST_DETAILS_CAP: i64 = 1000;

pub async fn insert_request_detail(db: &Db, d: RequestDetail) -> Result<()> {
    db.call(move |conn| {
        let mut data = serde_json::json!({
            "input_tokens": d.input_tokens,
            "output_tokens": d.output_tokens,
            "endpoint": d.endpoint,
        });
        if let (Some(map), Some(serde_json::Value::Object(extra))) = (data.as_object_mut(), d.extra)
        {
            for (k, v) in extra {
                map.insert(k, v);
            }
        }
        conn.execute(
            "INSERT INTO request_details (ts, provider, model, status, data)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            rusqlite::params![
                Utc::now().to_rfc3339(),
                d.provider,
                d.model,
                d.status,
                data.to_string(),
            ],
        )
        .map_err(|e| Error::Db(e.to_string()))?;
        // ring buffer: keep newest REQUEST_DETAILS_CAP rows
        conn.execute(
            "DELETE FROM request_details WHERE id NOT IN
             (SELECT id FROM request_details ORDER BY id DESC LIMIT ?1)",
            [REQUEST_DETAILS_CAP],
        )
        .map_err(|e| Error::Db(e.to_string()))?;
        Ok(())
    })
    .await
}

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RequestDetailRow {
    pub id: i64,
    pub ts: String,
    pub provider: String,
    pub model: String,
    pub status: String,
    pub data: serde_json::Value,
}

pub async fn list_request_details(db: &Db, limit: i64) -> Result<Vec<RequestDetailRow>> {
    db.call(move |conn| {
        let mut stmt = conn
            .prepare(
                "SELECT id, ts, provider, model, status, data FROM request_details
                 ORDER BY id DESC LIMIT ?1",
            )
            .map_err(|e| Error::Db(e.to_string()))?;
        let rows = stmt
            .query_map([limit.clamp(1, 1000)], |r| {
                let data: String = r.get(5)?;
                Ok(RequestDetailRow {
                    id: r.get(0)?,
                    ts: r.get(1)?,
                    provider: r.get(2)?,
                    model: r.get(3)?,
                    status: r.get(4)?,
                    data: serde_json::from_str(&data).unwrap_or(serde_json::Value::Null),
                })
            })
            .map_err(|e| Error::Db(e.to_string()))?
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(|e| Error::Db(e.to_string()))?;
        Ok(rows)
    })
    .await
}

// ---------------------------------------------------------------------------
// analytics aggregations (Usage dashboard)
// ---------------------------------------------------------------------------

/// Totals + today + per-model breakdown.
pub async fn stats(db: &Db) -> Result<serde_json::Value> {
    db.call(|conn| {
        let q = |sql: &str| -> rusqlite::Result<(i64, i64, i64)> {
            conn.query_row(sql, [], |r| {
                Ok((
                    r.get::<_, Option<i64>>(0)?.unwrap_or(0),
                    r.get::<_, Option<i64>>(1)?.unwrap_or(0),
                    r.get::<_, Option<i64>>(2)?.unwrap_or(0),
                ))
            })
        };
        let (req, inp, outp) =
            q("SELECT COUNT(*), SUM(prompt_tokens), SUM(completion_tokens) FROM usage_history")
                .map_err(|e| Error::Db(e.to_string()))?;
        let today = chrono::Utc::now().format("%Y-%m-%d").to_string();
        let (req_t, inp_t, outp_t) = q(&format!(
            "SELECT COUNT(*), SUM(prompt_tokens), SUM(completion_tokens) FROM usage_history
             WHERE ts >= '{today}'"
        ))
        .map_err(|e| Error::Db(e.to_string()))?;
        let rtk_saved: i64 = conn
            .query_row(
                "SELECT SUM(CAST(json_extract(meta, '$.rtk_saved_bytes') AS INTEGER))
                 FROM usage_history WHERE meta IS NOT NULL",
                [],
                |r| r.get::<_, Option<i64>>(0),
            )
            .map_err(|e| Error::Db(e.to_string()))?
            .unwrap_or(0);
        let mut stmt = conn
            .prepare(
                "SELECT model, COUNT(*), SUM(prompt_tokens), SUM(completion_tokens)
                 FROM usage_history GROUP BY model ORDER BY 2 DESC LIMIT 20",
            )
            .map_err(|e| Error::Db(e.to_string()))?;
        let by_model: Vec<serde_json::Value> = stmt
            .query_map([], |r| {
                Ok(serde_json::json!({
                    "model": r.get::<_, Option<String>>(0)?.unwrap_or_default(),
                    "requests": r.get::<_, i64>(1)?,
                    "input": r.get::<_, Option<i64>>(2)?.unwrap_or(0),
                    "output": r.get::<_, Option<i64>>(3)?.unwrap_or(0),
                }))
            })
            .map_err(|e| Error::Db(e.to_string()))?
            .filter_map(|r| r.ok())
            .collect();
        Ok(serde_json::json!({
            "total": { "requests": req, "input": inp, "output": outp },
            "today": { "requests": req_t, "input": inp_t, "output": outp_t },
            "rtk_saved_bytes": rtk_saved,
            "by_model": by_model,
        }))
    })
    .await
}

#[derive(Debug, serde::Serialize)]
pub struct DayRow {
    pub day: String,
    pub requests: i64,
    pub input: i64,
    pub output: i64,
}

/// Tokens/day for the last N days (chart source).
pub async fn history(db: &Db, days: i64) -> Result<Vec<DayRow>> {
    db.call(move |conn| {
        let mut stmt = conn
            .prepare(
                "SELECT substr(ts, 1, 10) AS day, COUNT(*),
                        SUM(prompt_tokens), SUM(completion_tokens)
                 FROM usage_history
                 WHERE ts >= date('now', ?1)
                 GROUP BY day ORDER BY day",
            )
            .map_err(|e| Error::Db(e.to_string()))?;
        let rows = stmt
            .query_map([format!("-{} days", days.clamp(1, 365))], |r| {
                Ok(DayRow {
                    day: r.get(0)?,
                    requests: r.get(1)?,
                    input: r.get::<_, Option<i64>>(2)?.unwrap_or(0),
                    output: r.get::<_, Option<i64>>(3)?.unwrap_or(0),
                })
            })
            .map_err(|e| Error::Db(e.to_string()))?
            .filter_map(|r| r.ok())
            .collect();
        Ok(rows)
    })
    .await
}

#[derive(Debug, serde::Serialize)]
pub struct ProviderRow {
    pub provider: String,
    pub requests: i64,
    pub input: i64,
    pub output: i64,
    pub errors: i64,
}

/// Per-provider aggregates.
pub async fn by_provider(db: &Db) -> Result<Vec<ProviderRow>> {
    db.call(|conn| {
        let mut stmt = conn
            .prepare(
                "SELECT provider, COUNT(*), SUM(prompt_tokens), SUM(completion_tokens),
                        SUM(CASE WHEN status = 'error' THEN 1 ELSE 0 END)
                 FROM usage_history GROUP BY provider ORDER BY 2 DESC",
            )
            .map_err(|e| Error::Db(e.to_string()))?;
        let rows = stmt
            .query_map([], |r| {
                Ok(ProviderRow {
                    provider: r.get::<_, Option<String>>(0)?.unwrap_or_default(),
                    requests: r.get(1)?,
                    input: r.get::<_, Option<i64>>(2)?.unwrap_or(0),
                    output: r.get::<_, Option<i64>>(3)?.unwrap_or(0),
                    errors: r.get::<_, Option<i64>>(4)?.unwrap_or(0),
                })
            })
            .map_err(|e| Error::Db(e.to_string()))?
            .filter_map(|r| r.ok())
            .collect();
        Ok(rows)
    })
    .await
}
