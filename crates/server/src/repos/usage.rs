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
}

pub async fn insert_request_detail(db: &Db, d: RequestDetail) -> Result<()> {
    db.call(move |conn| {
        let data = serde_json::json!({
            "input_tokens": d.input_tokens,
            "output_tokens": d.output_tokens,
            "endpoint": d.endpoint,
        });
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
        Ok(())
    })
    .await
}
