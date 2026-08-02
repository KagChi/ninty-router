//! Gemini-native endpoint: POST /v1beta/models/{model}:{action}

use std::sync::Arc;

use axum::extract::{Path, Query, State};
use axum::http::HeaderMap;
use axum::response::Response;
use axum::Json;
use ninty_core::error::Error;
use ninty_core::registry::WireFormat;

use crate::api::ApiError;
use crate::state::AppState;

pub async fn models_action(
    State(state): State<Arc<AppState>>,
    mut headers: HeaderMap,
    Path(path): Path<String>,
    Query(query): Query<std::collections::HashMap<String, String>>,
    Json(mut body): Json<serde_json::Value>,
) -> Result<Response, ApiError> {
    if let Some(key) = query.get("key") {
        if let Ok(v) = format!("Bearer {key}").parse() {
            headers.insert(axum::http::header::AUTHORIZATION, v);
        }
    }
    // path = "model:action" (action: generateContent | streamGenerateContent)
    let (model, action) = path
        .rsplit_once(':')
        .ok_or_else(|| Error::BadRequest(format!("path must be 'model:action', got '{path}'")))?;
    if model.is_empty() {
        return Err(Error::BadRequest("empty model".into()).into());
    }
    let stream = action == "streamGenerateContent";
    if action != "generateContent" && !stream {
        return Err(Error::BadRequest(format!("unsupported action '{action}'")).into());
    }

    // inject model + stream into the body for the shared chat core
    body["model"] = serde_json::Value::String(model.to_string());
    body["stream"] = serde_json::Value::Bool(stream);

    super::chat::run(
        State(state),
        headers,
        Json(body),
        WireFormat::Gemini,
        "/v1beta/models",
    )
    .await
}
