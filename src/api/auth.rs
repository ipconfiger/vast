use axum::{
    extract::State,
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use uuid::Uuid;

use crate::auth::{self, middleware::AuthenticatedUser, TokenPair};
use crate::error::{created_response, no_content, ok_response, AppError};
use crate::AppState;

// ---------------------------------------------------------------------------
// Response types
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
pub struct UserInfo {
    pub id: String,
    pub username: String,
}

#[derive(Debug, Serialize)]
pub struct AuthResponse {
    pub access_token: String,
    pub refresh_token: String,
    pub expires_in: u32,
    pub user: UserInfo,
}

impl AuthResponse {
    fn new(tokens: TokenPair, id: String, username: String) -> Self {
        Self {
            access_token: tokens.access_token,
            refresh_token: tokens.refresh_token,
            expires_in: tokens.expires_in,
            user: UserInfo { id, username },
        }
    }
}

// ---------------------------------------------------------------------------
// Request types
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct RegisterRequest {
    username: String,
    password: String,
    invite_code: String,
}

#[derive(Debug, Deserialize)]
pub struct LoginRequest {
    username: String,
    password: String,
}

#[derive(Debug, Deserialize)]
pub struct RefreshRequest {
    refresh_token: String,
}

#[derive(Debug, sqlx::FromRow)]
#[allow(dead_code)]
struct UserRow {
    id: String,
    username: String,
    password_hash: String,
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

/// POST /api/auth/register
pub async fn register(
    State(state): State<Arc<AppState>>,
    Json(body): Json<RegisterRequest>,
) -> Result<(StatusCode, Json<AuthResponse>), AppError> {
    let username = body.username.trim().to_string();

    // Validate username: 3-32 alphanumeric chars
    if username.len() < 3 || username.len() > 32 {
        return Err(AppError::ValidationError(
            "Username must be between 3 and 32 characters".to_string(),
        ));
    }
    if !username.chars().all(|c| c.is_ascii_alphanumeric()) {
        return Err(AppError::ValidationError(
            "Username must be alphanumeric (a-z, A-Z, 0-9)".to_string(),
        ));
    }

    // Validate password: >=8 chars, mix of letters + digits
    if body.password.len() < 8 {
        return Err(AppError::ValidationError(
            "Password must be at least 8 characters".to_string(),
        ));
    }
    let has_letter = body.password.chars().any(|c| c.is_ascii_alphabetic());
    let has_digit = body.password.chars().any(|c| c.is_ascii_digit());
    if !has_letter || !has_digit {
        return Err(AppError::ValidationError(
            "Password must contain both letters and digits".to_string(),
        ));
    }

    // Hash password (CPU-bound; do before opening the transaction so the
    // SQLite write lock isn't held during argon2 work).
    let password_hash = auth::hash_password(&body.password)
        .map_err(|e| AppError::Internal(format!("Failed to hash password: {e}")))?;
    let user_id = Uuid::new_v4().to_string();

    // Atomicity: invite check + user insert + use_count increment must commit
    // together — a partial commit (e.g. user created but use_count not bumped)
    // leaves the DB inconsistent. On any early `return Err` below, dropping
    // `tx` without commit triggers an automatic sqlx rollback.
    let mut tx = state.pool.begin().await.map_err(AppError::from)?;

    // 1. Check invite code validity within the transaction
    let invite = sqlx::query_as::<_, (String, i64, i64, bool)>(
        "SELECT code, max_uses, use_count, is_active FROM invite_codes WHERE code = ?",
    )
    .bind(&body.invite_code)
    .fetch_optional(&mut *tx)
    .await
    .map_err(AppError::from)?;

    match invite {
        Some((_code, max_uses, use_count, is_active)) => {
            if !is_active {
                return Err(AppError::Forbidden(
                    "Invite code is inactive".to_string(),
                ));
            }
            if use_count >= max_uses {
                return Err(AppError::Forbidden(
                    "Invite code has reached maximum uses".to_string(),
                ));
            }
        }
        None => {
            return Err(AppError::Forbidden(
                "Invalid invite code".to_string(),
            ));
        }
    }

    // 2. Check for duplicate username
    let existing = sqlx::query_as::<_, (String,)>(
        "SELECT id FROM users WHERE username = ?",
    )
    .bind(&username)
    .fetch_optional(&mut *tx)
    .await
    .map_err(AppError::from)?;

    if existing.is_some() {
        return Err(AppError::Conflict(
            "Username already taken".to_string(),
        ));
    }

    // 3. Insert user
    sqlx::query(
        "INSERT INTO users (id, username, password_hash) VALUES (?, ?, ?)",
    )
    .bind(&user_id)
    .bind(&username)
    .bind(&password_hash)
    .execute(&mut *tx)
    .await
    .map_err(AppError::from)?;

    // 4. Increment invite use_count
    sqlx::query(
        "UPDATE invite_codes SET use_count = use_count + 1 WHERE code = ?",
    )
    .bind(&body.invite_code)
    .execute(&mut *tx)
    .await
    .map_err(AppError::from)?;

    tx.commit().await.map_err(AppError::from)?;

    // Create token pair and response with user info
    let tokens = auth::create_token_pair(&user_id, &state.config.jwt_secret)
        .map_err(AppError::from)?;

    created_response(AuthResponse::new(tokens, user_id, username))
}
pub async fn login(
    State(state): State<Arc<AppState>>,
    Json(body): Json<LoginRequest>,
) -> Result<(StatusCode, Json<AuthResponse>), AppError> {
    let user = sqlx::query_as::<_, UserRow>(
        "SELECT id, username, password_hash FROM users WHERE username = ?",
    )
    .bind(&body.username)
    .fetch_optional(&state.pool)
    .await
    .map_err(AppError::from)?
    .ok_or_else(|| AppError::Unauthorized("Invalid username or password".to_string()))?;

    let valid = auth::verify_password(&body.password, &user.password_hash)
        .map_err(|_| AppError::Internal("Password verification failed".to_string()))?;

    if !valid {
        return Err(AppError::Unauthorized(
            "Invalid username or password".to_string(),
        ));
    }

    // Create token pair and response with user info
    let tokens = auth::create_token_pair(&user.id, &state.config.jwt_secret)
        .map_err(AppError::from)?;

    // Insert session record
    let session_id = Uuid::new_v4().to_string();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or(std::time::Duration::from_secs(0))
        .as_secs() as i64;
    let expires_at = now + 604800; // 7 days

    sqlx::query(
        "INSERT INTO sessions (id, user_id, token_hash, is_refresh, created_at, expires_at, is_active) VALUES (?, ?, ?, 1, ?, ?, 1)",
    )
    .bind(&session_id)
    .bind(&user.id)
    .bind(&session_id)
    .bind(now)
    .bind(expires_at)
    .execute(&state.pool)
    .await
    .map_err(AppError::from)?;

    ok_response(AuthResponse::new(tokens, user.id, user.username))
}
pub async fn refresh(
    State(state): State<Arc<AppState>>,
    Json(body): Json<RefreshRequest>,
) -> Result<(StatusCode, Json<TokenPair>), AppError> {
    let tokens = auth::refresh_access_token(&body.refresh_token, &state.config.jwt_secret)
        .map_err(AppError::from)?;
    ok_response(tokens)
}

/// POST /api/auth/logout
pub async fn logout(
    auth: AuthenticatedUser,
    State(state): State<Arc<AppState>>,
) -> Result<(StatusCode, Json<serde_json::Value>), AppError> {
    sqlx::query(
        "UPDATE sessions SET is_active = 0 WHERE user_id = ? AND is_active = 1",
    )
    .bind(&auth.0)
    .execute(&state.pool)
    .await
    .map_err(AppError::from)?;

    no_content()
}

#[derive(Debug, Serialize)]
pub struct ProfileResponse {
    pub id: String,
    pub username: String,
    pub display_name: String,
    pub avatar_url: String,
}

#[derive(Debug, Deserialize)]
pub struct UpdateProfileRequest {
    pub display_name: Option<String>,
    pub avatar_url: Option<String>,
}

/// GET /api/auth/profile
pub async fn get_profile(
    auth: AuthenticatedUser,
    State(state): State<Arc<AppState>>,
) -> Result<Json<ProfileResponse>, AppError> {
    let (username, display_name, avatar_url) = sqlx::query_as::<_, (String, String, String)>(
        "SELECT username, display_name, avatar_url FROM users WHERE id = ?",
    )
    .bind(&auth.0)
    .fetch_one(&state.pool)
    .await?;
    Ok(Json(ProfileResponse {
        id: auth.0,
        username,
        display_name,
        avatar_url,
    }))
}

/// PATCH /api/auth/profile
pub async fn update_profile(
    auth: AuthenticatedUser,
    State(state): State<Arc<AppState>>,
    Json(body): Json<UpdateProfileRequest>,
) -> Result<Json<ProfileResponse>, AppError> {
    if let Some(ref name) = body.display_name {
        let trimmed = name.trim();
        if trimmed.len() > 32 {
            return Err(AppError::BadRequest(
                "Display name must be 32 characters or fewer".into(),
            ));
        }
        sqlx::query("UPDATE users SET display_name = ? WHERE id = ?")
            .bind(trimmed)
            .bind(&auth.0)
            .execute(&state.pool)
            .await?;
    }
    if let Some(ref url) = body.avatar_url {
        sqlx::query("UPDATE users SET avatar_url = ? WHERE id = ?")
            .bind(url.trim()).bind(&auth.0).execute(&state.pool).await?;
    }
    let (username, display_name, avatar_url) = sqlx::query_as::<_, (String, String, String)>(
        "SELECT username, display_name, avatar_url FROM users WHERE id = ?",
    )
    .bind(&auth.0)
    .fetch_one(&state.pool)
    .await?;
    Ok(Json(ProfileResponse {
        id: auth.0,
        username,
        display_name,
        avatar_url,
    }))
}

/// Build the /auth sub-router
pub fn auth_routes() -> Router<Arc<AppState>> {
    Router::new()
        .route("/register", post(register))
        .route("/login", post(login))
        .route("/refresh", post(refresh))
        .route("/logout", post(logout))
        .route("/profile", get(get_profile).patch(update_profile))
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use serde_json::{json, Value};
    use std::sync::OnceLock;
    use tower::ServiceExt;

    fn ensure_env() {
        static ENV: OnceLock<()> = OnceLock::new();
        ENV.get_or_init(|| {
            // SAFETY: test-only; single-threaded, no concurrent readers
            unsafe { std::env::set_var("JWT_SECRET", "test-secret") };
        });
    }

    async fn setup_pool() -> sqlx::SqlitePool {
        let pool = sqlx::SqlitePool::connect(":memory:").await.unwrap();
        sqlx::migrate!("db/migrations").run(&pool).await.unwrap();

        sqlx::query(
            "INSERT INTO invite_codes (code, max_uses, is_active) VALUES ('TESTINVITE', 100, 1)",
        )
        .execute(&pool)
        .await
        .unwrap();

        pool
    }

    fn make_state(pool: sqlx::SqlitePool) -> Arc<AppState> {
        Arc::new(AppState {
            pool,
            ws_pool: Arc::new(crate::ws::ConnectionPool::new()),
            config: crate::AppConfig::test_default(),
        })
    }

    fn build_app(state: Arc<AppState>) -> Router {
        Router::new()
            .nest("/auth", auth_routes())
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

    // -----------------------------------------------------------------------
    // Register tests
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_register_success() {
        ensure_env();
        let pool = setup_pool().await;
        let mut app = build_app(make_state(pool));

        let (status, body) = post_json(
            &mut app,
            "/auth/register",
            json!({
                "username": "testuser",
                "password": "TestPass123",
                "invite_code": "TESTINVITE"
            }),
        )
        .await;

        assert_eq!(status, StatusCode::CREATED);
        assert!(body.get("access_token").and_then(|v| v.as_str()).is_some());
        assert!(body.get("refresh_token").and_then(|v| v.as_str()).is_some());
        assert!(body.get("expires_in").and_then(|v| v.as_u64()).is_some());
    }

    #[tokio::test]
    async fn test_register_duplicate_username() {
        ensure_env();
        let pool = setup_pool().await;
        let mut app = build_app(make_state(pool));

        // First registration
        let (status, _) = post_json(
            &mut app,
            "/auth/register",
            json!({
                "username": "testuser",
                "password": "TestPass123",
                "invite_code": "TESTINVITE"
            }),
        )
        .await;
        assert_eq!(status, StatusCode::CREATED);

        // Second registration with same username
        let (status, body) = post_json(
            &mut app,
            "/auth/register",
            json!({
                "username": "testuser",
                "password": "TestPass456",
                "invite_code": "TESTINVITE"
            }),
        )
        .await;

        assert_eq!(status, StatusCode::CONFLICT);
        assert_eq!(body["error"]["code"], "CONFLICT");
    }

    #[tokio::test]
    async fn test_register_invalid_invite() {
        ensure_env();
        let pool = setup_pool().await;
        let mut app = build_app(make_state(pool));

        let (status, body) = post_json(
            &mut app,
            "/auth/register",
            json!({
                "username": "testuser",
                "password": "TestPass123",
                "invite_code": "BADCODE"
            }),
        )
        .await;

        assert_eq!(status, StatusCode::FORBIDDEN);
        assert_eq!(body["error"]["code"], "FORBIDDEN");
    }

    #[tokio::test]
    async fn test_register_weak_password() {
        ensure_env();
        let pool = setup_pool().await;
        let mut app = build_app(make_state(pool));

        // Password too short
        let (status, body) = post_json(
            &mut app,
            "/auth/register",
            json!({
                "username": "testuser",
                "password": "Ab1",
                "invite_code": "TESTINVITE"
            }),
        )
        .await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(body["error"]["code"], "VALIDATION_ERROR");

        // Password without letters
        let (status, _body) = post_json(
            &mut app,
            "/auth/register",
            json!({
                "username": "testuser",
                "password": "12345678",
                "invite_code": "TESTINVITE"
            }),
        )
        .await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);

        // Password without digits
        let (status, _body) = post_json(
            &mut app,
            "/auth/register",
            json!({
                "username": "testuser",
                "password": "abcdefgh",
                "invite_code": "TESTINVITE"
            }),
        )
        .await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    }

    #[tokio::test]
    async fn test_register_invalid_username() {
        ensure_env();
        let pool = setup_pool().await;
        let mut app = build_app(make_state(pool));

        // Too short
        let (status, _) = post_json(
            &mut app,
            "/auth/register",
            json!({
                "username": "ab",
                "password": "TestPass123",
                "invite_code": "TESTINVITE"
            }),
        )
        .await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);

        // Contains non-alphanumeric
        let (status, _) = post_json(
            &mut app,
            "/auth/register",
            json!({
                "username": "test user!",
                "password": "TestPass123",
                "invite_code": "TESTINVITE"
            }),
        )
        .await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    }

    // -----------------------------------------------------------------------
    // Login tests
    // -----------------------------------------------------------------------

    async fn register_user(app: &mut Router, username: &str, password: &str) {
        let (status, _) = post_json(
            app,
            "/auth/register",
            json!({
                "username": username,
                "password": password,
                "invite_code": "TESTINVITE"
            }),
        )
        .await;
        assert_eq!(status, StatusCode::CREATED);
    }

    #[tokio::test]
    async fn test_login_success() {
        ensure_env();
        let pool = setup_pool().await;
        let mut app = build_app(make_state(pool));

        register_user(&mut app, "loginuser", "LoginPass123").await;

        let (status, body) = post_json(
            &mut app,
            "/auth/login",
            json!({
                "username": "loginuser",
                "password": "LoginPass123"
            }),
        )
        .await;

        assert_eq!(status, StatusCode::OK);
        assert!(body.get("access_token").and_then(|v| v.as_str()).is_some());
        assert!(body.get("refresh_token").and_then(|v| v.as_str()).is_some());
        assert!(body.get("expires_in").and_then(|v| v.as_u64()).is_some());
    }

    #[tokio::test]
    async fn test_login_wrong_password() {
        ensure_env();
        let pool = setup_pool().await;
        let mut app = build_app(make_state(pool));

        register_user(&mut app, "loginuser2", "LoginPass123").await;

        let (status, body) = post_json(
            &mut app,
            "/auth/login",
            json!({
                "username": "loginuser2",
                "password": "WrongPass123"
            }),
        )
        .await;

        assert_eq!(status, StatusCode::UNAUTHORIZED);
        assert_eq!(body["error"]["code"], "UNAUTHORIZED");
    }

    #[tokio::test]
    async fn test_login_nonexistent_user() {
        ensure_env();
        let pool = setup_pool().await;
        let mut app = build_app(make_state(pool));

        let (status, body) = post_json(
            &mut app,
            "/auth/login",
            json!({
                "username": "ghost",
                "password": "GhostPass123"
            }),
        )
        .await;

        assert_eq!(status, StatusCode::UNAUTHORIZED);
        assert_eq!(body["error"]["code"], "UNAUTHORIZED");
    }

    // -----------------------------------------------------------------------
    // Refresh tests
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_refresh_success() {
        ensure_env();
        let pool = setup_pool().await;
        let mut app = build_app(make_state(pool));

        register_user(&mut app, "refreshuser", "Refresh123").await;

        let (_, login_body) = post_json(
            &mut app,
            "/auth/login",
            json!({
                "username": "refreshuser",
                "password": "Refresh123"
            }),
        )
        .await;

        let refresh_token = login_body["refresh_token"].as_str().unwrap().to_string();

        let (status, body) = post_json(
            &mut app,
            "/auth/refresh",
            json!({ "refresh_token": refresh_token }),
        )
        .await;

        assert_eq!(status, StatusCode::OK);
        assert!(body.get("access_token").and_then(|v| v.as_str()).is_some());
        assert!(body.get("refresh_token").and_then(|v| v.as_str()).is_some());
    }

    #[tokio::test]
    async fn test_refresh_invalid_token() {
        ensure_env();
        let pool = setup_pool().await;
        let mut app = build_app(make_state(pool));

        let (status, _) = post_json(
            &mut app,
            "/auth/refresh",
            json!({ "refresh_token": "totally-invalid-token" }),
        )
        .await;

        assert_eq!(status, StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn test_refresh_access_token_as_refresh() {
        ensure_env();
        let pool = setup_pool().await;
        let mut app = build_app(make_state(pool));

        register_user(&mut app, "refreshuser2", "Refresh123").await;

        let (_, login_body) = post_json(
            &mut app,
            "/auth/login",
            json!({
                "username": "refreshuser2",
                "password": "Refresh123"
            }),
        )
        .await;

        let access_token = login_body["access_token"].as_str().unwrap().to_string();

        let (status, _) = post_json(
            &mut app,
            "/auth/refresh",
            json!({ "refresh_token": access_token }),
        )
        .await;

        assert_eq!(status, StatusCode::UNAUTHORIZED);
    }

    // -----------------------------------------------------------------------
    // Logout tests
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_logout_success() {
        ensure_env();
        let pool = setup_pool().await;
        let mut app = build_app(make_state(pool));

        register_user(&mut app, "logoutuser", "Logout123").await;

        let (_, login_body) = post_json(
            &mut app,
            "/auth/login",
            json!({
                "username": "logoutuser",
                "password": "Logout123"
            }),
        )
        .await;

        let access_token = login_body["access_token"].as_str().unwrap().to_string();

        let req = Request::builder()
            .method("POST")
            .uri("/auth/logout")
            .header("content-type", "application/json")
            .header("authorization", format!("Bearer {}", access_token))
            .body(Body::from("{}"))
            .unwrap();

        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::NO_CONTENT);
    }

    #[tokio::test]
    async fn test_logout_requires_auth() {
        ensure_env();
        let pool = setup_pool().await;
        let app = build_app(make_state(pool));

        let req = Request::builder()
            .method("POST")
            .uri("/auth/logout")
            .header("content-type", "application/json")
            .body(Body::from("{}"))
            .unwrap();

        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }
}
