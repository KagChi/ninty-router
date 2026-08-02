pub mod api;
pub mod apikey;
pub mod auth;
pub mod db;
pub mod models_preload;
pub mod oauth_state;
pub mod opencode_models;
pub mod repos;
pub mod state;
pub mod v1;
#[cfg(feature = "embed-web")]
pub mod web_static;

use std::net::SocketAddr;
use std::sync::Arc;

use state::AppState;

/// Build the full axum router.
pub fn build_router(state: Arc<AppState>) -> axum::Router {
    let app = api::router(state);
    #[cfg(feature = "embed-web")]
    let app = app.fallback(web_static::static_handler);
    app
}

/// Run the HTTP server until SIGTERM/SIGINT.
pub async fn run(state: Arc<AppState>, host: &str, port: u16) -> std::io::Result<()> {
    let db = state.db.clone();
    let state_for_preload = state.clone();
    let app = build_router(state);
    let addr: SocketAddr = format!("{host}:{port}").parse().map_err(|e| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            format!("{host}:{port}: {e}"),
        )
    })?;
    let listener = tokio::net::TcpListener::bind(addr).await?;
    tracing::info!("listening on http://{addr}");
    models_preload::spawn(state_for_preload);
    let r = axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await;
    if let Err(e) = db.checkpoint() {
        tracing::warn!("db checkpoint on shutdown failed: {e}");
    }
    r
}

async fn shutdown_signal() {
    use tokio::signal;
    let mut term = signal::unix::signal(signal::unix::SignalKind::terminate()).ok();
    let ctrl_c = signal::ctrl_c();
    tokio::select! {
        _ = ctrl_c => {},
        _ = async { if let Some(t) = term.as_mut() { t.recv().await; } else { std::future::pending().await } } => {},
    }
    tracing::info!("shutdown signal received");
}
