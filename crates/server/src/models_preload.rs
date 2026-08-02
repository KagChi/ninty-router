//! Models preload — boot-time + 10min refresh of upstream model lists into kv.
//! Providers with a `registry::models_fetcher` entry (opencode, openrouter)
//! have no/limited static lists; fetched lists are merged into /api/providers.

use std::sync::Arc;

use engine::models_fetch::{self, FetchedModel};
use ninty_core::registry;

use crate::state::AppState;

const REFRESH_S: u64 = 600;
const SCOPE: &str = "models";

fn key(provider: &str) -> String {
    provider.to_string()
}

/// Read the cached fetched list for a provider (empty vec when absent/corrupt).
pub async fn cached(state: &Arc<AppState>, provider: &str) -> Vec<FetchedModel> {
    let k = key(provider);
    let raw: Option<String> = state
        .db
        .call(move |conn| {
            Ok(conn
                .query_row(
                    "SELECT value FROM kv WHERE scope = ?1 AND key = ?2",
                    [SCOPE, &k],
                    |r| r.get::<_, String>(0),
                )
                .ok())
        })
        .await
        .ok()
        .flatten();
    raw.and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

async fn store(state: &Arc<AppState>, provider: &str, models: &[FetchedModel]) {
    let (k, v) = (key(provider), serde_json::to_string(models).unwrap_or_default());
    let _ = state
        .db
        .call(move |conn| {
            conn.execute(
                "INSERT INTO kv (scope, key, value) VALUES (?1, ?2, ?3)
                 ON CONFLICT(scope, key) DO UPDATE SET value = excluded.value",
                [SCOPE, &k, &v],
            )
            .map_err(|e| ninty_core::error::Error::Db(e.to_string()))?;
            Ok(())
        })
        .await;
}

async fn refresh_once(state: &Arc<AppState>) {
    for p in registry::all_providers() {
        let Some((url, filter)) = registry::models_fetcher(p.id) else {
            continue;
        };
        match models_fetch::fetch_models(&state.http, url, filter).await {
            Ok(models) => {
                tracing::info!("models preload {}: {} models", p.id, models.len());
                store(state, p.id, &models).await;
            }
            // Stale cache kept on failure — last good list survives upstream downtime.
            Err(e) => tracing::warn!("models preload {} failed: {e}", p.id),
        }
    }
}

/// Spawn the background preload loop (immediate first run, then every 10min).
pub fn spawn(state: Arc<AppState>) {
    tokio::spawn(async move {
        loop {
            refresh_once(&state).await;
            tokio::time::sleep(std::time::Duration::from_secs(REFRESH_S)).await;
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn kv_roundtrip() {
        let db = crate::db::Db::open_memory().unwrap();
        let state = Arc::new(AppState::new(db));
        assert!(cached(&state, "opencode").await.is_empty());
        let models = vec![FetchedModel {
            id: "gpt-5-free".into(),
            name: "gpt-5-free".into(),
        }];
        store(&state, "opencode", &models).await;
        assert_eq!(cached(&state, "opencode").await, models);
    }
}
