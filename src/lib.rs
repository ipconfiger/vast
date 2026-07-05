pub mod api;
pub mod auth;
pub mod bot;
pub mod db;
pub mod embed;
pub mod error;
pub mod ws;

use axum::{
    extract::State,
    http::{HeaderValue, Method, header},
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
    pub admin_username: String,
    /// Argon2id digest of the admin password. Empty string means the admin
    /// backend is disabled (no ADMIN_PASSWORD configured).
    pub admin_password_hash: String,
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
            .ok()
            .and_then(|path| path.parent().map(|p| p.to_path_buf()))
            .unwrap_or_else(|| std::path::PathBuf::from("."));
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

        let admin_username = std::env::var("ADMIN_USERNAME")
            .unwrap_or_else(|_| "admin".to_string());

        let admin_password_hash = std::env::var("ADMIN_PASSWORD")
            .ok()
            .filter(|pw| !pw.is_empty())
            .and_then(|pw| auth::hash_password(&pw).ok())
            .unwrap_or_default();

        Self {
            jwt_secret: std::env::var("JWT_SECRET")
                .unwrap_or_else(|_| "dev-secret-change-me".to_string()),
            invite_code: std::env::var("INVITE_CODE")
                .unwrap_or_else(|_| "IM2024".to_string()),
            admin_username,
            admin_password_hash,
            data_dir,
            tls_mode,
        }
    }

    /// Base `AppConfig` for the in-tree test helpers (`src/api/*.rs` test
    /// modules and `tests/integration/*.rs`). Sets deterministic non-secret
    /// values; the admin backend is disabled (empty hash). Test code overrides
    /// individual fields via struct-update syntax, e.g.
    /// `AppConfig { jwt_secret: secret.to_string(), ..AppConfig::test_default() }`.
    pub fn test_default() -> Self {
        Self {
            data_dir: std::path::PathBuf::from("/tmp"),
            jwt_secret: "test-secret".to_string(),
            invite_code: "TESTINVITE".to_string(),
            admin_username: "admin".to_string(),
            admin_password_hash: String::new(),
            tls_mode: TlsMode::None,
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
        .route("/raw", get(api::messages::raw_proxy))
        .route(
            "/channels",
            get(api::channels::list_channels).post(api::channels::create_channel),
        )
        .route(
            "/channels/discover",
            get(api::channels::discover_channels),
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
            "/trains/{train_id}",
            get(api::trains::get_train),
        )
        .route("/trains/{train_id}/join", post(api::trains::join_train))
        .route(
            "/votes/{vote_id}",
            get(api::votes::get_vote),
        )
        .route("/votes/{vote_id}/vote", post(api::votes::cast_vote))
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
        .nest("/admin", api::admin::admin_routes())
}

/// Build the full application router (API + WS + frontend fallback)
pub fn build_app(state: Arc<AppState>) -> Router {
    let allowed_origins = [
        "http://localhost:5173".parse::<HeaderValue>().unwrap(),
        "http://localhost:3000".parse::<HeaderValue>().unwrap(),
        "http://0.0.0.0:3000".parse::<HeaderValue>().unwrap(),
        "http://127.0.0.1:3000".parse::<HeaderValue>().unwrap(),
    ];

    let cors = CorsLayer::new()
        .allow_origin(allowed_origins)
        .allow_methods([
            Method::GET,
            Method::POST,
            Method::PUT,
            Method::PATCH,
            Method::DELETE,
            Method::OPTIONS,
        ])
        .allow_headers([
            header::CONTENT_TYPE,
            header::AUTHORIZATION,
            header::ACCEPT,
        ])
        .allow_credentials(true)
        .max_age(Duration::from_secs(3600));

    Router::new()
        .nest("/api", api_routes())
        .route("/ws", get(ws::ws_handler))
        .fallback(embed::serve_frontend)
        .layer(cors)
        .layer(TimeoutLayer::with_status_code(
            axum::http::StatusCode::REQUEST_TIMEOUT,
            Duration::from_secs(30),
        ))
        .with_state(state)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Serialize tests that mutate process-wide env vars to prevent data races
    /// between concurrent test threads. Tests acquiring this lock may freely
    /// call `unsafe { std::env::set_var(...) }`.
    static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    /// Given: ADMIN_PASSWORD is unset.
    /// When:  AppConfig::from_env() runs.
    /// Then:  admin_username falls back to "admin" and admin_password_hash is
    ///        empty (admin backend disabled).
    #[test]
    fn from_env_without_admin_password_yields_empty_hash() {
        let _guard = ENV_LOCK.lock().unwrap();
        // SAFETY: test-only; serialized via ENV_LOCK to prevent data races
        // with concurrent std::env::var readers.
        unsafe {
            std::env::remove_var("ADMIN_PASSWORD");
            std::env::remove_var("ADMIN_USERNAME");
        }
        let cfg = AppConfig::from_env();
        assert_eq!(cfg.admin_username, "admin");
        assert_eq!(cfg.admin_password_hash, "");
    }

    /// Given: ADMIN_USERNAME and ADMIN_PASSWORD are both set.
    /// When:  AppConfig::from_env() runs.
    /// Then:  admin_username reflects the env value, admin_password_hash is a
    ///        non-empty Argon2 digest, and the cleartext verifies against it.
    ///        The cleartext password is never retained on the struct.
    #[test]
    fn from_env_with_admin_password_yields_verifiable_hash() {
        let _guard = ENV_LOCK.lock().unwrap();
        // SAFETY: test-only; serialized via ENV_LOCK.
        unsafe {
            std::env::set_var("ADMIN_USERNAME", "root");
            std::env::set_var("ADMIN_PASSWORD", "S3cret-Pass-123!");
        }
        let cfg = AppConfig::from_env();
        assert_eq!(cfg.admin_username, "root");
        assert!(!cfg.admin_password_hash.is_empty());
        assert!(
            auth::verify_password("S3cret-Pass-123!", &cfg.admin_password_hash).unwrap(),
            "admin password hash must verify against the cleartext"
        );
        assert!(
            !auth::verify_password("wrong", &cfg.admin_password_hash).unwrap(),
            "an incorrect password must not verify"
        );
    }
}
