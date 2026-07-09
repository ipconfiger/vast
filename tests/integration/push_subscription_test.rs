//! Integration test: push subscription lifecycle
//!   subscribe (JWT auth) → unsubscribe (JWT auth) → resubscribe (no auth)
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
// Helpers (duplicated from auth_flow.rs for module independence)
// ---------------------------------------------------------------------------

fn ensure_env() {
    static ENV: std::sync::OnceLock<()> = std::sync::OnceLock::new();
    ENV.get_or_init(|| unsafe {
        std::env::set_var("JWT_SECRET", "integration-test-secret");
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

async fn delete_json(
    app: &mut Router,
    uri: &str,
    auth_token: &str,
) -> (StatusCode, Value) {
    let req = Request::builder()
        .method("DELETE")
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

async fn register_and_get_token(app: &mut Router) -> (String, String) {
    let username = format!(
        "pushuser{}",
        Uuid::new_v4()
            .to_string()
            .replace('-', "")
            .chars()
            .take(12)
            .collect::<String>()
    );

    let (status, body) = post_json(
        app,
        "/auth/register",
        json!({
            "username": &username,
            "password": "PushTest12345!",
            "invite_code": "E2ETEST"
        }),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "Register failed: {body}");

    let token = body["access_token"].as_str().unwrap().to_string();
    (username, token)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

/// POST /push/subscribe with valid JWT → 200 OK, subscription stored
#[tokio::test]
async fn subscribe_with_valid_jwt() {
    ensure_env();
    let pool = setup_pool().await;
    let mut app = build_app(make_state(pool.clone()));

    let (_username, token) = register_and_get_token(&mut app).await;

    let endpoint = "https://push.example.com/endpoint/abc123";
    let p256dh = "BPLIScSOM2fKjF2x4VtVbJ6KX5ZPz8nYRx0=";
    let auth = "a1b2c3d4e5f6g7h8";

    // Subscribe
    let (status, body) = post_json(
        &mut app,
        "/push/subscribe",
        json!({
            "endpoint": endpoint,
            "p256dh": p256dh,
            "auth": auth
        }),
        Some(&token),
    )
    .await;

    assert_eq!(status, StatusCode::OK, "Subscribe should return 200: {body}");
    assert_eq!(body["ok"], true, "Subscribe response should have ok: true");

    // Verify DB state
    let row: (String, String, String) = sqlx::query_as(
        "SELECT endpoint, p256dh, auth FROM push_subscriptions WHERE endpoint = ?",
    )
    .bind(endpoint)
    .fetch_one(&pool)
    .await
    .expect("Subscription should be in DB");

    assert_eq!(row.0, endpoint);
    assert_eq!(row.1, p256dh);
    assert_eq!(row.2, auth);
}

/// POST /push/subscribe without JWT → 401 Unauthorized
#[tokio::test]
async fn subscribe_without_jwt_returns_401() {
    ensure_env();
    let pool = setup_pool().await;
    let mut app = build_app(make_state(pool));

    let (status, body) = post_json(
        &mut app,
        "/push/subscribe",
        json!({
            "endpoint": "https://push.example.com/endpoint/noauth",
            "p256dh": "BPLIScSOM2fKjF2x4VtVbJ6KX5ZPz8nYRx0=",
            "auth": "a1b2c3d4e5f6g7h8"
        }),
        None,
    )
    .await;

    assert_eq!(status, StatusCode::UNAUTHORIZED, "Expected 401 without token: {body}");
}

/// DELETE /push/unsubscribe removes the correct subscription for the
/// authenticated user, and leaves subscriptions of other users untouched.
#[tokio::test]
async fn unsubscribe_removes_correct_subscription() {
    ensure_env();
    let pool = setup_pool().await;
    let mut app = build_app(make_state(pool.clone()));

    // Register two users
    let (_user_a, token_a) = register_and_get_token(&mut app).await;
    let (_user_b, token_b) = register_and_get_token(&mut app).await;

    let endpoint_a = "https://push.example.com/endpoint/user-a";
    let endpoint_b = "https://push.example.com/endpoint/user-b";
    let p256dh = "BKVx3oTcXYrGQm9LRzFPwOq2sWZaHn=";
    let auth = "z9y8x7w6v5u4t3s2";

    // Both users subscribe
    let (s, _) = post_json(
        &mut app,
        "/push/subscribe",
        json!({"endpoint": endpoint_a, "p256dh": p256dh, "auth": auth}),
        Some(&token_a),
    )
    .await;
    assert_eq!(s, StatusCode::OK);

    let (s, _) = post_json(
        &mut app,
        "/push/subscribe",
        json!({"endpoint": endpoint_b, "p256dh": p256dh, "auth": auth}),
        Some(&token_b),
    )
    .await;
    assert_eq!(s, StatusCode::OK);

    // User A unsubscribes their own endpoint
    let (status, body) = delete_json(
        &mut app,
        &format!("/push/unsubscribe?endpoint={}", endpoint_a),
        &token_a,
    )
    .await;

    assert_eq!(status, StatusCode::OK, "Unsubscribe should return 200: {body}");
    assert_eq!(body["ok"], true);

    // User A's subscription should be gone
    let count_a: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM push_subscriptions WHERE endpoint = ?",
    )
    .bind(endpoint_a)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(count_a, 0, "User A's subscription should be deleted");

    // User B's subscription should remain
    let count_b: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM push_subscriptions WHERE endpoint = ?",
    )
    .bind(endpoint_b)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(count_b, 1, "User B's subscription should be untouched");
}

/// DELETE /push/unsubscribe is idempotent — calling it with a nonexistent
/// endpoint returns 200 OK (no-op).
#[tokio::test]
async fn unsubscribe_nonexistent_returns_ok() {
    ensure_env();
    let pool = setup_pool().await;
    let mut app = build_app(make_state(pool));

    let (_username, token) = register_and_get_token(&mut app).await;

    let (status, body) = delete_json(
        &mut app,
        "/push/unsubscribe?endpoint=https://push.example.com/endpoint/nonexistent",
        &token,
    )
    .await;

    assert_eq!(status, StatusCode::OK, "Expected 200 OK (idempotent) for nonexistent: {body}");
}

/// POST /push/resubscribe updates endpoint+p256dh+auth WITHOUT auth, and
/// the old endpoint is replaced by the new one in the database.
#[tokio::test]
async fn resubscribe_updates_endpoint_without_auth() {
    ensure_env();
    let pool = setup_pool().await;
    let mut app = build_app(make_state(pool.clone()));

    let (_username, token) = register_and_get_token(&mut app).await;

    let old_endpoint = "https://push.example.com/endpoint/old-v1";
    let old_p256dh = "BOldP256dhKey1234567890abcdef==";
    let old_auth = "old-auth-secret-12345678";

    // Subscribe first (creates a row)
    let (s, _) = post_json(
        &mut app,
        "/push/subscribe",
        json!({"endpoint": old_endpoint, "p256dh": old_p256dh, "auth": old_auth}),
        Some(&token),
    )
    .await;
    assert_eq!(s, StatusCode::OK);

    // Now resubscribe WITHOUT auth — service worker scenario
    let new_endpoint = "https://push.example.com/endpoint/new-v2";
    let new_p256dh = "BNewP256dhKey9876543210fedcba==";
    let new_auth = "new-auth-secret-87654321";

    let (status, body) = post_json(
        &mut app,
        "/push/resubscribe",
        json!({
            "old_endpoint": old_endpoint,
            "new_endpoint": new_endpoint,
            "new_p256dh": new_p256dh,
            "new_auth": new_auth
        }),
        None, // No auth header
    )
    .await;

    assert_eq!(status, StatusCode::OK, "Resubscribe should return 200: {body}");
    assert_eq!(body["ok"], true);

    // Old endpoint should be absent from DB
    let old_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM push_subscriptions WHERE endpoint = ?",
    )
    .bind(old_endpoint)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(old_count, 0, "Old endpoint should no longer exist in DB");

    // New endpoint should be present with updated fields
    let row: (String, String, String) = sqlx::query_as(
        "SELECT endpoint, p256dh, auth FROM push_subscriptions WHERE endpoint = ?",
    )
    .bind(new_endpoint)
    .fetch_one(&pool)
    .await
    .expect("New subscription should be in DB");

    assert_eq!(row.0, new_endpoint);
    assert_eq!(row.1, new_p256dh);
    assert_eq!(row.2, new_auth);
}

/// POST /push/resubscribe is idempotent — calling it with an unknown
/// old_endpoint returns 200 OK (no-op).
#[tokio::test]
async fn resubscribe_nonexistent_returns_ok() {
    ensure_env();
    let pool = setup_pool().await;
    let mut app = build_app(make_state(pool));

    let (status, body) = post_json(
        &mut app,
        "/push/resubscribe",
        json!({
            "old_endpoint": "https://push.example.com/endpoint/ghost",
            "new_endpoint": "https://push.example.com/endpoint/replacement",
            "new_p256dh": "BReplacementKey1234567890==",
            "new_auth": "replacement-auth"
        }),
        None,
    )
    .await;

    assert_eq!(status, StatusCode::OK, "Expected 200 OK (idempotent) for unknown old_endpoint: {body}");
}

/// Subscribe is idempotent — calling it twice with the same endpoint does
/// not create a duplicate row (INSERT OR IGNORE semantics).
#[tokio::test]
async fn subscribe_is_idempotent() {
    ensure_env();
    let pool = setup_pool().await;
    let mut app = build_app(make_state(pool.clone()));

    let (_username, token) = register_and_get_token(&mut app).await;

    let endpoint = "https://push.example.com/endpoint/idempotent";
    let body = json!({"endpoint": endpoint, "p256dh": "BKey1==", "auth": "auth1"});

    // First subscribe
    let (s, _) = post_json(&mut app, "/push/subscribe", body.clone(), Some(&token)).await;
    assert_eq!(s, StatusCode::OK);

    // Second subscribe with same endpoint
    let (s, _) = post_json(&mut app, "/push/subscribe", body.clone(), Some(&token)).await;
    assert_eq!(s, StatusCode::OK);

    // Only one row should exist
    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM push_subscriptions WHERE endpoint = ?",
    )
    .bind(endpoint)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(count, 1, "Subscribing twice should not duplicate the row");
}
