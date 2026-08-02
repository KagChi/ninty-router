//! Suggested-models fetchers — port of 9router /api/providers/suggested-models
//! (route.js + filters.js). Upstream JSON: `json.data ?? json.models ?? json`.

use ninty_core::registry::ModelsFilter;
use serde_json::Value;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct FetchedModel {
    pub id: String,
    pub name: String,
}

/// Free opencode models without the "-free" id suffix (9router filters.js).
const KNOWN_FREE_OPENCODE: &[&str] = &["big-pickle"];

pub fn filter_models(filter: ModelsFilter, models: &[Value]) -> Vec<FetchedModel> {
    match filter {
        ModelsFilter::OpencodeFree => models
            .iter()
            .filter_map(|m| m.get("id").and_then(Value::as_str))
            .filter(|id| id.ends_with("-free") || KNOWN_FREE_OPENCODE.contains(id))
            .map(|id| FetchedModel {
                id: id.to_string(),
                name: id.to_string(),
            })
            .collect(),
        ModelsFilter::OpenrouterFree => {
            let mut out: Vec<(i64, FetchedModel)> = models
                .iter()
                .filter(|m| {
                    let pricing = m.get("pricing").cloned().unwrap_or(Value::Null);
                    pricing.get("prompt").and_then(Value::as_str) == Some("0")
                        && pricing.get("completion").and_then(Value::as_str) == Some("0")
                        && m.get("context_length").and_then(Value::as_i64).unwrap_or(0)
                            >= 200_000
                })
                .filter_map(|m| {
                    let id = m.get("id").and_then(Value::as_str)?.to_string();
                    let name = m
                        .get("name")
                        .and_then(Value::as_str)
                        .unwrap_or(&id)
                        .to_string();
                    let ctx = m.get("context_length").and_then(Value::as_i64).unwrap_or(0);
                    Some((ctx, FetchedModel { id, name }))
                })
                .collect();
            out.sort_by_key(|(ctx, _)| std::cmp::Reverse(*ctx));
            out.into_iter().map(|(_, m)| m).collect()
        }
    }
}

/// Fetch + filter upstream models. Returns Err on network/parse failure.
pub async fn fetch_models(
    client: &reqwest::Client,
    url: &str,
    filter: ModelsFilter,
) -> Result<Vec<FetchedModel>, String> {
    let resp = client
        .get(url)
        .timeout(std::time::Duration::from_secs(15))
        .header("accept", "application/json")
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if !resp.status().is_success() {
        return Err(format!("HTTP {}", resp.status()));
    }
    let json: Value = resp.json().await.map_err(|e| e.to_string())?;
    let raw = json
        .get("data")
        .or_else(|| json.get("models"))
        .unwrap_or(&json);
    let arr = raw.as_array().cloned().unwrap_or_default();
    Ok(filter_models(filter, &arr))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opencode_free_filter() {
        let models = serde_json::json!([
            {"id": "gpt-5-free"},
            {"id": "big-pickle"},
            {"id": "gpt-5"},
            {"id": "glm-5.2-free"}
        ]);
        let out = filter_models(ModelsFilter::OpencodeFree, models.as_array().unwrap());
        let ids: Vec<_> = out.iter().map(|m| m.id.as_str()).collect();
        assert_eq!(ids, ["gpt-5-free", "big-pickle", "glm-5.2-free"]);
    }

    #[test]
    fn openrouter_free_filter() {
        let models = serde_json::json!([
            {"id": "free-big", "name": "Free Big", "context_length": 256000,
             "pricing": {"prompt": "0", "completion": "0"}},
            {"id": "paid", "name": "Paid", "context_length": 1000000,
             "pricing": {"prompt": "0.001", "completion": "0.002"}},
            {"id": "free-small", "name": "Free Small", "context_length": 32000,
             "pricing": {"prompt": "0", "completion": "0"}}
        ]);
        let out = filter_models(ModelsFilter::OpenrouterFree, models.as_array().unwrap());
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].id, "free-big");
    }
}
