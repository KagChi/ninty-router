pub mod chat;
pub mod models;
pub mod v1beta;

use std::sync::Arc;

use axum::routing::{get, post};
use axum::Router;

use crate::state::AppState;

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/chat/completions", post(chat::chat_completions))
        .route("/messages", post(chat::messages))
        .route("/models", get(models::list_models))
}
