//! GET /v1/models — models from providers that have ≥1 active connection, plus nodes.

use std::sync::Arc;

use axum::extract::State;
use axum::Json;
use ninty_core::registry;
use serde_json::json;

use crate::state::AppState;

pub async fn list_models(State(state): State<Arc<AppState>>) -> Json<serde_json::Value> {
    let conns = crate::repos::connections::list(&state.db, None)
        .await
        .unwrap_or_default();
    let nodes = crate::repos::nodes::list(&state.db)
        .await
        .unwrap_or_default();

    let mut data: Vec<serde_json::Value> = Vec::new();
    for provider in registry::all_providers() {
        let has_conn = conns
            .iter()
            .any(|c| c.provider == provider.id && c.is_active);
        if !has_conn {
            continue;
        }
        if provider.id == "opencode" {
            // dynamic free model list: preloaded cache first, live fetch fallback
            let cached = crate::models_preload::cached(&state, "opencode").await;
            if !cached.is_empty() {
                for m in cached {
                    data.push(json!({
                        "id": format!("oc/{}", m.id),
                        "object": "model",
                        "owned_by": "opencode",
                    }));
                }
            } else if let Ok(ids) = crate::opencode_models::fetch(&state).await {
                for id in ids {
                    data.push(json!({
                        "id": format!("oc/{id}"),
                        "object": "model",
                        "owned_by": "opencode",
                    }));
                }
            }
            continue;
        }
        for m in provider.models {
            data.push(json!({
                "id": format!("{}/{}", provider.id, m.id),
                "object": "model",
                "owned_by": provider.id,
            }));
        }
        // Preloaded upstream extras (openrouter suggested free models).
        if registry::models_fetcher(provider.id).is_some() {
            let fetched = crate::models_preload::cached(&state, provider.id).await;
            for m in fetched {
                if provider.models.iter().any(|sm| sm.id == m.id) {
                    continue;
                }
                data.push(json!({
                    "id": format!("{}/{}", provider.id, m.id),
                    "object": "model",
                    "owned_by": provider.id,
                }));
            }
        }
    }
    for node in nodes {
        let prefix = node.prefix().unwrap_or("node");
        if let Some(models) = node.data.get("models").and_then(|m| m.as_array()) {
            for m in models {
                if let Some(id) = m.as_str() {
                    data.push(json!({
                        "id": format!("{prefix}/{id}"),
                        "object": "model",
                        "owned_by": "custom-node",
                    }));
                }
            }
        }
    }

    Json(json!({"object": "list", "data": data}))
}
