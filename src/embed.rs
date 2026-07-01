use axum::{
    body::Body,
    http::{header, StatusCode, Uri},
    response::{IntoResponse, Response},
};
use rust_embed::RustEmbed;

#[derive(RustEmbed)]
#[folder = "frontend/dist/"]
#[exclude = "*.map"]
struct Asset;

/// Serve frontend SPA with history API fallback
pub async fn serve_frontend(uri: Uri) -> impl IntoResponse {
    let path = uri.path().trim_start_matches('/');
    let path = if path.is_empty() { "index.html" } else { path };

    match Asset::get(path) {
        Some(content) => {
            let mime = mime_guess::from_path(path).first_or_octet_stream();
            Response::builder()
                .status(StatusCode::OK)
                .header(header::CONTENT_TYPE, mime.as_ref())
                .body(Body::from(content.data))
                .unwrap()
                .into_response()
        }
        None => {
            // SPA fallback: return index.html for client-side routing
            match Asset::get("index.html") {
                Some(content) => Response::builder()
                    .status(StatusCode::OK)
                    .header(header::CONTENT_TYPE, "text/html")
                    .body(Body::from(content.data))
                    .unwrap()
                    .into_response(),
                None => (
                    StatusCode::NOT_FOUND,
                    "Frontend not built. Run: cd frontend && bun run build",
                )
                    .into_response(),
            }
        }
    }
}
