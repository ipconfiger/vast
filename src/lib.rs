pub mod api;
pub mod auth;
pub mod db;
pub mod embed;
pub mod error;
pub mod ws;

use axum::{
    extract::State,
    routing::{delete, get, post, put},
    Json, Router,
};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tower_http::cors::CorsLayer;
use tower_http::limit::RequestBodyLimitLayer;
use tower_http::timeout::TimeoutLayer;

pub struct AppState {
    pub pool: sqlx::SqlitePool,
    pub ws_pool: Arc<ws::ConnectionPool>,
    pub config: AppConfig,
}

#[derive(Clone)]
pub struct AppConfig {
    pub data_dir: PathBuf,
    pub jwt_secret: String,
    pub invite_code: String,
    pub tls_mode: TlsMode,
}

#[derive(Clone, PartialEq)]
pub enum TlsMode {
    /// Plain HTTP (default)
    None,
    /// Self-signed certificate from certs/cert.pem + certs/key.pem
    SelfSigned,
    /// Let's Encrypt certificate from certs/fullchain.pem + certs/privkey.pem
    LetsEncrypt,
}

impl AppConfig {
    pub fn from_env() -> Self {
        let exe_dir = std::env::current_exe()
            .unwrap()
            .parent()
            .unwrap()
            .to_path_buf();
        let data_dir = exe_dir.join("data");

        let tls_mode = match std::env::var("TLS_MODE")
            .unwrap_or_default()
            .to_lowercase()
            .as_str()
        {
            "self-signed" => TlsMode::SelfSigned,
            "lets-encrypt" => TlsMode::LetsEncrypt,
            _ => TlsMode::None,
        };

        Self {
            jwt_secret: std::env::var("JWT_SECRET")
                .unwrap_or_else(|_| "dev-secret-change-me".to_string()),
            invite_code: std::env::var("INVITE_CODE")
                .unwrap_or_else(|_| "IM2024".to_string()),
            data_dir,
            tls_mode,
        }
    }
}

/// Health check — verifies DB is reachable
pub async fn health_check(State(state): State<Arc<AppState>>) -> Json<serde_json::Value> {
    match state.pool.acquire().await {
        Ok(_conn) => {
            Json(serde_json::json!({"status": "ok", "db": "connected"}))
        }
        Err(_) => {
            Json(serde_json::json!({"status": "degraded", "db": "error"}))
        }
    }
}

/// Build the API sub-router (also used by tests)
pub fn api_routes() -> Router<Arc<AppState>> {
    Router::new()
        .route("/health", get(health_check))
        .route(
            "/channels",
            get(api::channels::list_channels).post(api::channels::create_channel),
        )
        .route(
            "/channels/{id}",
            get(api::channels::get_channel).patch(api::channels::update_channel),
        )
        .route(
            "/channels/{id}/archive",
            post(api::channels::archive_channel),
        )
        .route(
            "/channels/{id}/unarchive",
            post(api::channels::unarchive_channel),
        )
        .route(
            "/channels/{channel_id}/messages",
            get(api::messages::get_messages).post(api::messages::send_message),
        )
        .route(
            "/channels/{channel_id}/messages/{msg_id}/thread",
            get(api::messages::get_thread),
        )
        .route(
            "/messages/{message_id}",
            delete(api::messages::delete_message),
        )
        .route(
            "/files/upload",
            post(api::files::upload_file)
                .layer(RequestBodyLimitLayer::new(api::files::MAX_UPLOAD_SIZE)),
        )
        .route("/files/{file_id}", get(api::files::download_file))
        .route(
            "/messages/{message_id}/reactions",
            get(api::reactions::get_reactions).post(api::reactions::add_reaction),
        )
        .route(
            "/messages/{message_id}/reactions/{emoji}",
            delete(api::reactions::remove_reaction),
        )
        .route("/search", get(api::search::search_messages))
        .route(
            "/channels/{id}/join-request",
            post(api::requests::create_join_request),
        )
        .route("/requests", get(api::requests::list_join_requests))
        .route(
            "/requests/{id}/approve",
            put(api::requests::approve_join_request),
        )
        .route(
            "/requests/{id}/reject",
            put(api::requests::reject_join_request),
        )
        .route(
            "/channels/{id}/invitations",
            post(api::invitations::create_invitation),
        )
        .route("/invitations", get(api::invitations::list_invitations))
        .route(
            "/invitations/{id}/accept",
            put(api::invitations::accept_invitation),
        )
        .route(
            "/invitations/{id}/reject",
            put(api::invitations::reject_invitation),
        )
        .nest("/channels", api::channel_members::routes())
        .nest("/dm", api::dm::dm_routes())
        .nest("/auth", api::auth::auth_routes())
}

/// Build the full application router (API + WS + frontend fallback)
pub fn build_app(state: Arc<AppState>) -> Router {
    Router::new()
        .nest("/api", api_routes())
        .route("/ws", get(ws::ws_handler))
        .route("/", get(|| async { "IM Server" }))
        .fallback(embed::serve_frontend)
        .layer(CorsLayer::permissive())
        .layer(TimeoutLayer::with_status_code(
            axum::http::StatusCode::REQUEST_TIMEOUT,
            Duration::from_secs(30),
        ))
        .with_state(state)
}
