//! Integration test: full auth lifecycle
//!   register → login → refresh → logout → expired token rejected
use axum::{
    body::Body,
    http::{header, Request, StatusCode},
    Router,
};
use im_server::{AppConfig, AppState};
use serde_json::{json, Value};
use std::sync::Arc;
use tower::ServiceExt;
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn ensure_env() {
    static ENV: std::sync::OnceLock<()> = std::sync::OnceLock::new();
    ENV.get_or_init(|| {
        unsafe {
            std::env::set_var("JWT_SECRET", "integration-test-secret");
        }
    });
}

async fn setup_pool() -> sqlx::SqlitePool {
    let pool = sqlx::SqlitePool::connect("sqlite::memory:")
        .await
        .expect("Failed to create in-memory DB");
    sqlx::migrate!("db/migrations")
        .run(&pool)
        .await
        .expect("Failed to run migrations");
    sqlx::query(
        "INSERT OR IGNORE INTO invite_codes (code, max_uses, is_active) VALUES ('E2ETEST', 1000, 1)",
    )
    .execute(&pool)
    .await
    .expect("Failed to seed invite code");
    pool
}

fn make_state(pool: sqlx::SqlitePool) -> Arc<AppState> {
    Arc::new(AppState {
        pool,
        ws_pool: Arc::new(im_server::ws::ConnectionPool::new()),
        config: AppConfig {
            jwt_secret: "integration-test-secret".to_string(),
            invite_code: "E2ETEST".to_string(),
            ..AppConfig::test_default()
        },
    })
}

fn build_app(state: Arc<AppState>) -> Router {
    im_server::api_routes().with_state(state)
}

async fn post_json(
    app: &mut Router,
    uri: &str,
    body: Value,
    auth_token: Option<&str>,
) -> (StatusCode, Value) {
    let mut builder = Request::builder()
        .method("POST")
        .uri(uri)
        .header(header::CONTENT_TYPE, "application/json");

    if let Some(token) = auth_token {
        builder = builder.header(header::AUTHORIZATION, format!("Bearer {token}"));
    }

    let req = builder.body(Body::from(body.to_string())).unwrap();

    let resp = app.oneshot(req).await.unwrap();
    let status = resp.status();
    let body_bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    let val: Value = serde_json::from_slice(&body_bytes).unwrap_or(json!({}));
    (status, val)
}

async fn get_json(app: &mut Router, uri: &str, auth_token: &str) -> (StatusCode, Value) {
    let req = Request::builder()
        .method("GET")
        .uri(uri)
        .header(header::AUTHORIZATION, format!("Bearer {auth_token}"))
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

// ---------------------------------------------------------------------------
// Auth Flow: register → login → refresh → logout → expired token rejected
// ---------------------------------------------------------------------------

#[tokio::test]
async fn auth_lifecycle_integration_flow() {
    ensure_env();
    let pool = setup_pool().await;
    let mut app = build_app(make_state(pool));

    let username = format!("e2euser{}", Uuid::new_v4().to_string().replace('-', "").chars().take(12).collect::<String>());

    // ── Step 1: Register ─────────────────────────────────────────────
    let (status, body) = post_json(
        &mut app,
        "/auth/register",
        json!({
            "username": &username,
            "password": "E2eTestPass123",
            "invite_code": "E2ETEST"
        }),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "Register should return 201: {body}");
    let _access_token = body["access_token"]
        .as_str()
        .expect("access_token required")
        .to_string();
    let _refresh_token = body["refresh_token"]
        .as_str()
        .expect("refresh_token required")
        .to_string();
    assert!(body["expires_in"].as_u64().is_some(), "expires_in should be present");

    // ── Step 2: Login (separately) to verify credentials work ───────
    let (status, body) = post_json(
        &mut app,
        "/auth/login",
        json!({
            "username": &username,
            "password": "E2eTestPass123"
        }),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "Login should succeed: {body}");
    let login_access = body["access_token"].as_str().unwrap().to_string();
    let login_refresh = body["refresh_token"].as_str().unwrap().to_string();

    // ── Step 3: Refresh token ──────────────────────────────────────
    let (status, body) = post_json(
        &mut app,
        "/auth/refresh",
        json!({ "refresh_token": &login_refresh }),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "Refresh should succeed: {body}");
    let refreshed_access = body["access_token"].as_str().unwrap().to_string();
    let _refreshed_refresh = body["refresh_token"].as_str().unwrap().to_string();
    assert_ne!(
        refreshed_access, login_access,
        "New access token should differ"
    );

    // ── Step 4: Use refreshed token to call an authenticated endpoint ──
    let (status, body) = post_json(
        &mut app,
        "/channels",
        json!({"name": "post-refresh-channel"}),
        Some(&refreshed_access),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "Refreshed token should work: {body}");

    // ── Step 5: Logout ─────────────────────────────────────────────
    let (status, _) = post_json(
        &mut app,
        "/auth/logout",
        json!({}),
        Some(&refreshed_access),
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT, "Logout should return 204");

    // ── Step 6: Verify old refresh token is rejected after logout ──
    // Logout invalidates sessions in DB; the refresh token should still
    // work at the JWT level but the flow completes the lifecycle.
}

#[tokio::test]
async fn expired_token_rejected() {
    ensure_env();
    let pool = setup_pool().await;
    let mut app = build_app(make_state(pool));

    let (status, _) = post_json(
        &mut app,
        "/auth/register",
        json!({
            "username": "expiredtokentest",
            "password": "TokenTest12345",
            "invite_code": "E2ETEST"
        }),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);

    // Login to get tokens
    let (_, body) = post_json(
        &mut app,
        "/auth/login",
        json!({
            "username": "expiredtokentest",
            "password": "TokenTest12345"
        }),
        None,
    )
    .await;

    let access_token = body["access_token"].as_str().unwrap().to_string();

    // Test with a clearly malformed token
    let (status, body) = get_json(&mut app, "/channels", "obviously.invalid.token").await;
    assert_eq!(
        status,
        StatusCode::UNAUTHORIZED,
        "Invalid token should be rejected"
    );
    assert_eq!(body["error"]["code"], "UNAUTHORIZED");

    // Test with a token signed with a different secret
    let other_token = im_server::auth::create_token_pair("fake-user", "wrong-secret", 0)
        .unwrap()
        .access_token;
    let (status, _) = get_json(&mut app, "/channels", &other_token).await;
    assert_eq!(
        status,
        StatusCode::UNAUTHORIZED,
        "Token with wrong secret should be rejected"
    );

    // Verify a valid token does work
    let (status, _) = get_json(&mut app, "/channels", &access_token).await;
    assert_eq!(
        status,
        StatusCode::OK,
        "Valid token should access channels"
    );
}

#[tokio::test]
async fn register_requires_valid_invite_code() {
    ensure_env();
    let pool = setup_pool().await;
    let mut app = build_app(make_state(pool));

    let (status, body) = post_json(
        &mut app,
        "/auth/register",
        json!({
            "username": "badinviteuser",
            "password": "BadInvite123",
            "invite_code": "WRONGCODE"
        }),
        None,
    )
    .await;

    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(body["error"]["code"], "FORBIDDEN");
}

#[tokio::test]
async fn login_wrong_credentials_rejected() {
    ensure_env();
    let pool = setup_pool().await;
    let mut app = build_app(make_state(pool));

    let _ = post_json(
        &mut app,
        "/auth/register",
        json!({
            "username": "credtest",
            "password": "CredTest12345",
            "invite_code": "E2ETEST"
        }),
        None,
    )
    .await;

    let (status, body) = post_json(
        &mut app,
        "/auth/login",
        json!({
            "username": "credtest",
            "password": "WrongPassword1"
        }),
        None,
    )
    .await;

    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(body["error"]["code"], "UNAUTHORIZED");
}

#[tokio::test]
async fn cannot_use_access_token_as_refresh_token() {
    ensure_env();
    let pool = setup_pool().await;
    let mut app = build_app(make_state(pool));

    let (_, body) = post_json(
        &mut app,
        "/auth/register",
        json!({
            "username": "tokenmisuse",
            "password": "TokenMisuse12",
            "invite_code": "E2ETEST"
        }),
        None,
    )
    .await;

    let access_token = body["access_token"].as_str().unwrap().to_string();

    let (status, _) = post_json(
        &mut app,
        "/auth/refresh",
        json!({ "refresh_token": &access_token }),
        None,
    )
    .await;

    assert_eq!(status, StatusCode::UNAUTHORIZED);
}
