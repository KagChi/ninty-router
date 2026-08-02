//! Embedded dashboard (feature `embed-web`): serves the Solid-Start static
//! bundle from web/.output/public with SPA fallback to index.html.

use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};

#[derive(rust_embed::RustEmbed)]
#[folder = "../../web/.output/public"]
struct WebAssets;

pub async fn static_handler(uri: axum::http::Uri) -> Response {
    let path = uri.path().trim_start_matches('/');
    serve(path)
}

fn serve(path: &str) -> Response {
    let path = if path.is_empty() { "index.html" } else { path };
    if let Some(file) = WebAssets::get(path) {
        return respond(path, file.data.into());
    }
    // SPA fallback: unknown non-API paths render the app shell
    if let Some(index) = WebAssets::get("index.html") {
        return respond("index.html", index.data.into());
    }
    StatusCode::NOT_FOUND.into_response()
}

fn respond(path: &str, body: Vec<u8>) -> Response {
    let mime = mime_guess(path);
    (
        [(header::CONTENT_TYPE, mime)],
        // immutable cache for hashed assets only
        [(
            header::CACHE_CONTROL,
            if path.starts_with("assets/") {
                "public, max-age=31536000, immutable"
            } else {
                "no-cache"
            },
        )],
        body,
    )
        .into_response()
}

fn mime_guess(path: &str) -> &'static str {
    match path.rsplit('.').next() {
        Some("html") => "text/html; charset=utf-8",
        Some("js") => "text/javascript; charset=utf-8",
        Some("css") => "text/css; charset=utf-8",
        Some("json") => "application/json",
        Some("svg") => "image/svg+xml",
        Some("png") => "image/png",
        Some("ico") => "image/x-icon",
        Some("woff2") => "font/woff2",
        Some("txt") => "text/plain; charset=utf-8",
        Some("webmanifest") => "application/manifest+json",
        _ => "application/octet-stream",
    }
}
