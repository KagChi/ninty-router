//! /api/combos CRUD.

use std::sync::Arc;

use axum::extract::{Path, State};
use axum::routing::{delete, get, post, put};
use axum::{Json, Router};
use ninty_core::error::Error;

use super::ApiError;
use crate::repos::combos::{self, Combo, NewCombo};
use crate::state::AppState;

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/", get(list).post(create))
        .route("/{id}", put(update).delete(remove))
}

async fn list(State(state): State<Arc<AppState>>) -> Result<Json<Vec<Combo>>, ApiError> {
    Ok(Json(combos::list(&state.db).await?))
}

async fn create(
    State(state): State<Arc<AppState>>,
    Json(new): Json<NewCombo>,
) -> Result<Json<Combo>, ApiError> {
    Ok(Json(combos::create(&state.db, new).await?))
}

async fn update(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(patch): Json<NewCombo>,
) -> Result<(), ApiError> {
    combos::update(&state.db, &id, patch).await?;
    Ok(())
}

async fn remove(State(state): State<Arc<AppState>>, Path(id): Path<String>) -> Result<(), ApiError> {
    combos::delete(&state.db, &id).await?;
    Ok(())
}

fn _unused(e: Error) -> ApiError {
    ApiError(e)
}

#[allow(unused)]
fn routes(_r: Router<Arc<AppState>>) -> Router<Arc<AppState>> {
    _r.route("/x", delete(remove)).route("/y", post(create))
}
