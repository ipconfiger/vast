//! Admin-console REST endpoints — login, refresh, logout, current-user.
//!
//! All routes are mounted under `/api/admin`. Admin auth is JWT-based but
//! isolated from user auth: admin tokens carry `is_admin: true` and are
//! rejected by user-facing middleware (and vice versa).

use axum::{
    extract::State,
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use std::sync::Arc;
use uuid::Uuid;

use crate::auth::{self, admin::AdminAuthenticatedUser, TokenPair};
use crate::error::{no_content, ok_response, AppError};
use crate::AppState;

// ---------------------------------------------------------------------------
// Request / response types
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct AdminLoginRequest {
    pub username: String,
    pub password: String,
}

#[derive(Debug, Deserialize)]
pub struct AdminRefreshRequest {
    pub refresh_token: String,
}

#[derive(Debug, Serialize)]
pub struct AdminAuthResponse {
    pub access_token: String,
    pub refresh_token: String,
    pub expires_in: u32,
}

impl AdminAuthResponse {
    fn from_tokens(tokens: TokenPair) -> Self {
        Self {
            access_token: tokens.access_token,
            refresh_token: tokens.refresh_token,
            expires_in: tokens.expires_in,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct AdminMeResponse {
    pub username: String,
}

#[derive(Debug, Serialize)]
pub struct DashboardStats {
    pub total_users: i64,
    pub active_sessions_24h: i64,
    pub total_channels: i64,
    pub total_messages: i64,
    pub total_invite_codes: i64,
    pub active_invite_codes: i64,
}

// ---------------------------------------------------------------------------
// Audit helper
// ---------------------------------------------------------------------------

/// Append a row to `admin_audit_logs`.
///
/// NOTE: the T4 task brief listed a `performed_by` column, but migration
/// 006's schema is `(id, action, target_type, target_id, details,
/// performed_at)` — there is no `performed_by`. This helper matches the
/// actual schema. The admin principal is always `"admin"` (the sole
/// configured username), so encoding it as a column would carry no
/// additional information.
///
/// `performed_at` is a Unix-seconds `i64` (the column is `INTEGER NOT NULL`),
/// matching the timestamp convention used in `src/api/auth.rs`.
///
/// Best-effort: callers should ignore the `Result` unless they specifically
/// need to surface audit failures. Login, for example, must not fail just
/// because the audit row could not be written.
pub(crate) async fn audit(
    pool: &SqlitePool,
    action: &str,
    target_type: Option<&str>,
    target_id: Option<&str>,
    details: Option<&str>,
) -> Result<(), AppError> {
    let id = Uuid::now_v7().to_string();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|e| AppError::Internal(format!("SystemTime before UNIX_EPOCH: {e}")))?
        .as_secs() as i64;

    sqlx::query(
        "INSERT INTO admin_audit_logs \
         (id, action, target_type, target_id, details, performed_at) \
         VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(&id)
    .bind(action)
    .bind(target_type)
    .bind(target_id)
    .bind(details)
    .bind(now)
    .execute(pool)
    .await?;

    Ok(())
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

/// POST /api/admin/login
///
/// Returns 403 when the admin backend is disabled (no password hash
/// configured), 401 on username/password mismatch, 200 with a fresh admin
/// token pair on success.
pub async fn login(
    State(state): State<Arc<AppState>>,
    Json(body): Json<AdminLoginRequest>,
) -> Result<(axum::http::StatusCode, Json<AdminAuthResponse>), AppError> {
    if state.config.admin_password_hash.is_empty() {
        return Err(AppError::Forbidden(
            "Admin backend is disabled".to_string(),
        ));
    }

    if body.username != state.config.admin_username {
        return Err(AppError::Unauthorized(
            "Invalid username or password".to_string(),
        ));
    }

    let valid = auth::verify_password(&body.password, &state.config.admin_password_hash)
        .map_err(|_| AppError::Internal("Password verification failed".to_string()))?;
    if !valid {
        return Err(AppError::Unauthorized(
            "Invalid username or password".to_string(),
        ));
    }

    let tokens = crate::auth::admin::create_admin_token_pair(&state.config.jwt_secret)?;

    // Best-effort audit; failure must not block login.
    let _ = audit(&state.pool, "admin.login", None, None, None).await;

    ok_response(AdminAuthResponse::from_tokens(tokens))
}

/// POST /api/admin/refresh
pub async fn refresh(
    State(state): State<Arc<AppState>>,
    Json(body): Json<AdminRefreshRequest>,
) -> Result<(axum::http::StatusCode, Json<AdminAuthResponse>), AppError> {
    let tokens = crate::auth::admin::refresh_admin_token(
        &body.refresh_token,
        &state.config.jwt_secret,
    )?;
    ok_response(AdminAuthResponse::from_tokens(tokens))
}

/// POST /api/admin/logout
///
/// Stateless 204 No Content. v1 known limitation: a stolen refresh token
/// survives `REFRESH_TTL_SECS` (7 days) because admin auth has no
/// server-side session revocation list. Mitigation: rotate `ADMIN_PASSWORD`
/// (forces hash mismatch on next login) or rotate `JWT_SECRET` (invalidates
/// all outstanding admin tokens).
pub async fn logout(
    _auth: AdminAuthenticatedUser,
    State(_state): State<Arc<AppState>>,
) -> Result<(axum::http::StatusCode, Json<serde_json::Value>), AppError> {
    no_content()
}

/// GET /api/admin/me
///
/// Returns the configured `admin_username`. The token's `sub` is always
/// `"admin"` (a fixed subject shared by every admin pair), so the response
/// uses `state.config.admin_username` for a human-readable handle.
pub async fn me(
    _auth: AdminAuthenticatedUser,
    State(state): State<Arc<AppState>>,
) -> Result<Json<AdminMeResponse>, AppError> {
    Ok(Json(AdminMeResponse {
        username: state.config.admin_username.clone(),
    }))
}

/// GET /api/admin/dashboard
///
/// Returns aggregate platform statistics: total users, sessions active
/// in the last 24 hours, total channels, messages, and invite codes
/// (total + active). Runs 6 lightweight COUNT queries.
pub async fn dashboard(
    _admin: AdminAuthenticatedUser,
    State(state): State<Arc<AppState>>,
) -> Result<(axum::http::StatusCode, Json<DashboardStats>), AppError> {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|e| AppError::Internal(format!("SystemTime before UNIX_EPOCH: {e}")))?
        .as_secs() as i64;
    let cutoff = now - 86400;

    let total_users: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM users")
            .fetch_one(&state.pool)
            .await?;

    let active_sessions_24h: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM sessions WHERE is_active = 1 AND created_at >= ?")
            .bind(cutoff)
            .fetch_one(&state.pool)
            .await?;

    let total_channels: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM channels")
            .fetch_one(&state.pool)
            .await?;

    let total_messages: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM messages")
            .fetch_one(&state.pool)
            .await?;

    let total_invite_codes: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM invite_codes")
            .fetch_one(&state.pool)
            .await?;

    let active_invite_codes: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM invite_codes WHERE is_active = 1")
            .fetch_one(&state.pool)
            .await?;

    ok_response(DashboardStats {
        total_users,
        active_sessions_24h,
        total_channels,
        total_messages,
        total_invite_codes,
        active_invite_codes,
    })
}

// ---------------------------------------------------------------------------
// Router
// ---------------------------------------------------------------------------

/// Build the `/admin` sub-router.
pub fn admin_routes() -> Router<Arc<AppState>> {
    Router::new()
        .route("/login", post(login))
        .route("/refresh", post(refresh))
        .route("/logout", post(logout))
        .route("/me", get(me))
        .route("/dashboard", get(dashboard))
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::admin::create_admin_token_pair;
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use serde_json::{json, Value};
    use tower::ServiceExt;

    const ADMIN_PASS: &str = "test-admin-pass";

    async fn setup_pool() -> sqlx::SqlitePool {
        let pool = sqlx::SqlitePool::connect(":memory:").await.unwrap();
        sqlx::migrate!("db/migrations").run(&pool).await.unwrap();
        pool
    }

    fn admin_enabled_config() -> crate::AppConfig {
        let mut config = crate::AppConfig::test_default();
        config.admin_password_hash = crate::auth::hash_password(ADMIN_PASS).unwrap();
        config.admin_username = "admin".to_string();
        config
    }

    fn make_state(pool: sqlx::SqlitePool, config: crate::AppConfig) -> Arc<AppState> {
        Arc::new(AppState {
            pool,
            ws_pool: Arc::new(crate::ws::ConnectionPool::new()),
            config,
        })
    }

    fn build_app(state: Arc<AppState>) -> Router {
        Router::new()
            .nest("/admin", admin_routes())
            .with_state(state)
    }

    async fn post_json(
        app: &mut Router,
        uri: &str,
        body: Value,
    ) -> (StatusCode, Value) {
        let req = Request::builder()
            .method("POST")
            .uri(uri)
            .header("content-type", "application/json")
            .body(Body::from(body.to_string()))
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        let status = resp.status();
        let body_bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let val: Value = serde_json::from_slice(&body_bytes).unwrap_or(json!({}));
        (status, val)
    }

    async fn get_with_token(
        app: &mut Router,
        uri: &str,
        token: &str,
    ) -> (StatusCode, Value) {
        let req = Request::builder()
            .method("GET")
            .uri(uri)
            .header("authorization", format!("Bearer {token}"))
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        let status = resp.status();
        let body_bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let val: Value = serde_json::from_slice(&body_bytes).unwrap_or(json!({}));
        (status, val)
    }

    async fn post_with_token(
        app: &mut Router,
        uri: &str,
        token: &str,
    ) -> (StatusCode, Value) {
        let req = Request::builder()
            .method("POST")
            .uri(uri)
            .header("authorization", format!("Bearer {token}"))
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        let status = resp.status();
        let body_bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let val: Value = serde_json::from_slice(&body_bytes).unwrap_or(json!({}));
        (status, val)
    }

    // -----------------------------------------------------------------------
    // Login tests
    // -----------------------------------------------------------------------

    /// Given: admin backend is enabled with a known password hash.
    /// When:  POST /admin/login with correct credentials.
    /// Then:  200 OK with access_token, refresh_token, and expires_in.
    #[tokio::test]
    async fn test_login_success() {
        let pool = setup_pool().await;
        let mut app = build_app(make_state(pool, admin_enabled_config()));

        let (status, body) = post_json(
            &mut app,
            "/admin/login",
            json!({"username": "admin", "password": ADMIN_PASS}),
        )
        .await;

        assert_eq!(status, StatusCode::OK);
        assert!(body.get("access_token").and_then(|v| v.as_str()).is_some());
        assert!(
            body.get("refresh_token").and_then(|v| v.as_str()).is_some()
        );
        assert!(body.get("expires_in").and_then(|v| v.as_u64()).is_some());
    }

    /// Given: admin backend is enabled.
    /// When:  POST /admin/login with a wrong password.
    /// Then:  401 Unauthorized.
    #[tokio::test]
    async fn test_login_wrong_password() {
        let pool = setup_pool().await;
        let mut app = build_app(make_state(pool, admin_enabled_config()));

        let (status, body) = post_json(
            &mut app,
            "/admin/login",
            json!({"username": "admin", "password": "totally-wrong"}),
        )
        .await;

        assert_eq!(status, StatusCode::UNAUTHORIZED);
        assert_eq!(body["error"]["code"], "UNAUTHORIZED");
    }

    /// Given: admin backend is enabled with username "admin".
    /// When:  POST /admin/login with a wrong username.
    /// Then:  401 Unauthorized.
    #[tokio::test]
    async fn test_login_wrong_username() {
        let pool = setup_pool().await;
        let mut app = build_app(make_state(pool, admin_enabled_config()));

        let (status, body) = post_json(
            &mut app,
            "/admin/login",
            json!({"username": "root", "password": ADMIN_PASS}),
        )
        .await;

        assert_eq!(status, StatusCode::UNAUTHORIZED);
        assert_eq!(body["error"]["code"], "UNAUTHORIZED");
    }

    /// Given: admin backend is disabled (empty password hash).
    /// When:  POST /admin/login with any credentials.
    /// Then:  403 Forbidden — admin backend is disabled.
    #[tokio::test]
    async fn test_login_admin_disabled() {
        let pool = setup_pool().await;
        let mut app = build_app(make_state(pool, crate::AppConfig::test_default()));

        let (status, body) = post_json(
            &mut app,
            "/admin/login",
            json!({"username": "admin", "password": ADMIN_PASS}),
        )
        .await;

        assert_eq!(status, StatusCode::FORBIDDEN);
        assert_eq!(body["error"]["code"], "FORBIDDEN");
    }

    /// Given: a successful admin login.
    /// When:  the audit table is queried.
    /// Then:  exactly one row exists with action = "admin.login".
    #[tokio::test]
    async fn test_login_writes_audit_row() {
        let pool = setup_pool().await;
        let state = make_state(pool.clone(), admin_enabled_config());
        let mut app = build_app(state);

        let (status, _) = post_json(
            &mut app,
            "/admin/login",
            json!({"username": "admin", "password": ADMIN_PASS}),
        )
        .await;
        assert_eq!(status, StatusCode::OK);

        let (count,): (i64,) =
            sqlx::query_as("SELECT COUNT(*) FROM admin_audit_logs WHERE action = 'admin.login'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(count, 1, "login must write exactly one audit row");
    }

    // -----------------------------------------------------------------------
    // /me tests
    // -----------------------------------------------------------------------

    /// Given: /admin/me requires authentication.
    /// When:  GET /admin/me with no Authorization header.
    /// Then:  401 Unauthorized.
    #[tokio::test]
    async fn test_me_without_token() {
        let pool = setup_pool().await;
        let app = build_app(make_state(pool, admin_enabled_config()));

        let req = Request::builder()
            .method("GET")
            .uri("/admin/me")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    /// Given: a user JWT (is_admin = false).
    /// When:  GET /admin/me with the user token.
    /// Then:  401 Unauthorized — admin endpoints reject user tokens.
    #[tokio::test]
    async fn test_me_with_user_token() {
        let pool = setup_pool().await;
        let mut app = build_app(make_state(pool, admin_enabled_config()));

        let user_pair = crate::auth::create_token_pair("user-1", "test-secret", 0).unwrap();

        let (status, body) = get_with_token(&mut app, "/admin/me", &user_pair.access_token).await;

        assert_eq!(status, StatusCode::UNAUTHORIZED);
        assert_eq!(body["error"]["code"], "UNAUTHORIZED");
    }

    /// Given: a valid admin access token.
    /// When:  GET /admin/me.
    /// Then:  200 OK with { username: "admin" } (from config, not token sub).
    #[tokio::test]
    async fn test_me_with_admin_token() {
        let pool = setup_pool().await;
        let mut app = build_app(make_state(pool, admin_enabled_config()));

        let pair = create_admin_token_pair("test-secret").unwrap();
        let (status, body) = get_with_token(&mut app, "/admin/me", &pair.access_token).await;

        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["username"], "admin");
    }

    // -----------------------------------------------------------------------
    // Refresh tests
    // -----------------------------------------------------------------------

    /// Given: a valid admin refresh token.
    /// When:  POST /admin/refresh.
    /// Then:  200 OK with a new token pair (access differs from original).
    #[tokio::test]
    async fn test_refresh_success() {
        let pool = setup_pool().await;
        let mut app = build_app(make_state(pool, admin_enabled_config()));

        let original = create_admin_token_pair("test-secret").unwrap();
        let (status, body) = post_json(
            &mut app,
            "/admin/refresh",
            json!({"refresh_token": original.refresh_token}),
        )
        .await;

        assert_eq!(status, StatusCode::OK);
        let new_access = body["access_token"].as_str().expect("access_token present");
        let new_refresh = body["refresh_token"].as_str().expect("refresh_token present");
        assert!(body.get("expires_in").and_then(|v| v.as_u64()).is_some());
        assert_ne!(new_access, original.access_token);
        assert_ne!(new_refresh, original.refresh_token);
    }

    /// Given: an invalid refresh token string.
    /// When:  POST /admin/refresh.
    /// Then:  401 Unauthorized.
    #[tokio::test]
    async fn test_refresh_invalid_token() {
        let pool = setup_pool().await;
        let mut app = build_app(make_state(pool, admin_enabled_config()));

        let (status, body) = post_json(
            &mut app,
            "/admin/refresh",
            json!({"refresh_token": "not-a-real-jwt"}),
        )
        .await;

        assert_eq!(status, StatusCode::UNAUTHORIZED);
        assert_eq!(body["error"]["code"], "UNAUTHORIZED");
    }

    // -----------------------------------------------------------------------
    // Logout tests
    // -----------------------------------------------------------------------

    /// Given: a valid admin access token.
    /// When:  POST /admin/logout.
    /// Then:  204 No Content (stateless).
    #[tokio::test]
    async fn test_logout_with_admin_token() {
        let pool = setup_pool().await;
        let mut app = build_app(make_state(pool, admin_enabled_config()));

        let pair = create_admin_token_pair("test-secret").unwrap();
        let (status, _body) = post_with_token(&mut app, "/admin/logout", &pair.access_token).await;

        assert_eq!(status, StatusCode::NO_CONTENT);
    }

    // -----------------------------------------------------------------------
    // Dashboard tests
    // -----------------------------------------------------------------------

    /// Given: a freshly migrated database with the seeded invite code removed.
    /// When:  GET /admin/dashboard with a valid admin token.
    /// Then:  200 OK with all six counts at 0.
    #[tokio::test]
    async fn test_dashboard_empty_db() {
        let pool = setup_pool().await;
        sqlx::query("DELETE FROM invite_codes")
            .execute(&pool)
            .await
            .unwrap();

        let mut app = build_app(make_state(pool, admin_enabled_config()));
        let pair = create_admin_token_pair("test-secret").unwrap();

        let (status, body) = get_with_token(&mut app, "/admin/dashboard", &pair.access_token).await;

        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["total_users"], 0);
        assert_eq!(body["active_sessions_24h"], 0);
        assert_eq!(body["total_channels"], 0);
        assert_eq!(body["total_messages"], 0);
        assert_eq!(body["total_invite_codes"], 0);
        assert_eq!(body["active_invite_codes"], 0);
    }

    /// Given: 3 users, 3 sessions (2 recent+active, 1 old), 5 channels,
    ///         10 messages, 4 invite codes (3 active, 1 inactive), with
    ///         the seeded IM2024 invite code removed for deterministic counts.
    /// When:  GET /admin/dashboard with a valid admin token.
    /// Then:  200 OK with exact aggregate counts.
    #[tokio::test]
    async fn test_dashboard_with_data() {
        let pool = setup_pool().await;
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;

        // Clear seeded invite code so counts are deterministic.
        sqlx::query("DELETE FROM invite_codes")
            .execute(&pool)
            .await
            .unwrap();

        for i in 0..3 {
            let id = format!("user-{i}");
            sqlx::query("INSERT INTO users (id, username, password_hash) VALUES (?, ?, ?)")
                .bind(&id)
                .bind(format!("u{i}"))
                .bind("hash")
                .execute(&pool)
                .await
                .unwrap();
        }

        for i in 0..2 {
            let id = format!("session-{i}");
            sqlx::query(
                "INSERT INTO sessions (id, user_id, token_hash, is_refresh, created_at, expires_at, is_active) \
                 VALUES (?, ?, ?, 0, ?, ?, 1)",
            )
            .bind(&id)
            .bind(format!("user-{i}"))
            .bind(format!("hash-{i}"))
            .bind(now)
            .bind(now + 3600)
            .execute(&pool)
            .await
            .unwrap();
        }
        sqlx::query(
            "INSERT INTO sessions (id, user_id, token_hash, is_refresh, created_at, expires_at, is_active) \
             VALUES (?, ?, ?, 0, ?, ?, 1)",
        )
        .bind("session-old")
        .bind("user-0")
        .bind("hash-old")
        .bind(now - 100_000)
        .bind(now + 3600)
        .execute(&pool)
        .await
        .unwrap();

        for i in 0..5 {
            let id = format!("channel-{i}");
            sqlx::query("INSERT INTO channels (id, name) VALUES (?, ?)")
                .bind(&id)
                .bind(format!("ch{i}"))
                .execute(&pool)
                .await
                .unwrap();
        }

        for i in 0..10 {
            let mid = format!("msg-{i}");
            sqlx::query(
                "INSERT INTO messages (msg_id, channel_id, sender_id, msg_type, payload) \
                 VALUES (?, 'channel-0', 'user-0', 'text', '{}')",
            )
            .bind(&mid)
            .execute(&pool)
            .await
            .unwrap();
        }

        for i in 0..3 {
            let code = format!("active-{i}");
            sqlx::query("INSERT INTO invite_codes (code, is_active) VALUES (?, 1)")
                .bind(&code)
                .execute(&pool)
                .await
                .unwrap();
        }
        sqlx::query("INSERT INTO invite_codes (code, is_active) VALUES (?, 0)")
            .bind("inactive")
            .execute(&pool)
            .await
            .unwrap();

        let mut app = build_app(make_state(pool, admin_enabled_config()));
        let pair = create_admin_token_pair("test-secret").unwrap();

        let (status, body) = get_with_token(&mut app, "/admin/dashboard", &pair.access_token).await;

        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["total_users"], 3);
        assert_eq!(body["active_sessions_24h"], 2);
        assert_eq!(body["total_channels"], 5);
        assert_eq!(body["total_messages"], 10);
        assert_eq!(body["total_invite_codes"], 4);
        assert_eq!(body["active_invite_codes"], 3);
    }

    /// Given: /admin/dashboard requires admin authentication.
    /// When:  GET /admin/dashboard with no Authorization header.
    /// Then:  401 Unauthorized.
    #[tokio::test]
    async fn test_dashboard_without_token() {
        let pool = setup_pool().await;
        let app = build_app(make_state(pool, admin_enabled_config()));

        let req = Request::builder()
            .method("GET")
            .uri("/admin/dashboard")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    /// Given: a user JWT (is_admin = false).
    /// When:  GET /admin/dashboard with the user token.
    /// Then:  401 Unauthorized — admin endpoints reject user tokens.
    #[tokio::test]
    async fn test_dashboard_with_user_token() {
        let pool = setup_pool().await;
        let mut app = build_app(make_state(pool, admin_enabled_config()));

        let user_pair = crate::auth::create_token_pair("user-1", "test-secret", 0).unwrap();
        let (status, body) =
            get_with_token(&mut app, "/admin/dashboard", &user_pair.access_token).await;

        assert_eq!(status, StatusCode::UNAUTHORIZED);
        assert_eq!(body["error"]["code"], "UNAUTHORIZED");
    }
}
