//! OpenCode free model list: fetched from opencode.ai/zen/v1/models, cached 10min.

use ninty_core::error::Result;

use crate::state::AppState;

const URL: &str = "https://opencode.ai/zen/v1/models";
const CACHE_TTL_SECS: i64 = 600;

/// Model ids: keep "*-free" + "big-pickle" (reference opencode-free filter).
pub async fn fetch(state: &AppState) -> Result<Vec<String>> {
    if let Some(cached) = kv_get_fresh(state).await? {
        return Ok(cached);
    }
    let resp: serde_json::Value = state
        .http
        .get(URL)
        .timeout(std::time::Duration::from_secs(15))
        .send()
        .await
        .map_err(|e| ninty_core::error::Error::Upstream {
            status: 502,
            message: format!("opencode models: {e}"),
        })?
        .json()
        .await
        .map_err(|e| ninty_core::error::Error::Upstream {
            status: 502,
            message: format!("opencode models json: {e}"),
        })?;
    let arr = resp
        .get("data")
        .or_else(|| resp.get("models"))
        .and_then(|d| d.as_array())
        .or_else(|| resp.as_array())
        .cloned()
        .unwrap_or_default();
    let ids: Vec<String> = arr
        .iter()
        .filter_map(|m| m.get("id").and_then(|i| i.as_str()).or_else(|| m.as_str()))
        .filter(|id| id.ends_with("-free") || *id == "big-pickle")
        .map(String::from)
        .collect();
    kv_put(state, &ids).await?;
    Ok(ids)
}

async fn kv_get_fresh(state: &AppState) -> Result<Option<Vec<String>>> {
    state
        .db
        .call(|conn| {
            let row: Option<(String,)> = conn
                .query_row(
                    "SELECT value FROM kv WHERE scope = 'opencode_models' AND key = 'list'",
                    [],
                    |r| Ok((r.get::<_, String>(0)?,)),
                )
                .ok();
            let Some((json,)) = row else { return Ok(None) };
            let v: serde_json::Value = serde_json::from_str(&json)?;
            let ts = v.get("ts").and_then(|t| t.as_i64()).unwrap_or(0);
            if chrono::Utc::now().timestamp() - ts > CACHE_TTL_SECS {
                return Ok(None);
            }
            let ids: Vec<String> = v
                .get("ids")
                .and_then(|i| serde_json::from_value(i.clone()).ok())
                .unwrap_or_default();
            if ids.is_empty() {
                Ok(None)
            } else {
                Ok(Some(ids))
            }
        })
        .await
}

async fn kv_put(state: &AppState, ids: &[String]) -> Result<()> {
    let json = serde_json::json!({
        "ts": chrono::Utc::now().timestamp(),
        "ids": ids,
    })
    .to_string();
    state
        .db
        .call(move |conn| {
            conn.execute(
                "INSERT OR REPLACE INTO kv (scope, key, value) VALUES ('opencode_models', 'list', ?1)",
                [json],
            )
            .map_err(|e| ninty_core::error::Error::Db(e.to_string()))?;
            Ok(())
        })
        .await
}
