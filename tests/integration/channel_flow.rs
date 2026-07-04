//! Integration test: channel lifecycle
//!   create channel → invite/join → send message → archive → read messages
use axum::{
    body::Body,
    http::{header, Method, Request, StatusCode},
    Router,
};
use im_server::{AppConfig, AppState};
use serde_json::{json, Value};
use std::sync::Arc;
use tower::ServiceExt;

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
    sqlx::query("INSERT OR IGNORE INTO invite_codes (code, max_uses, is_active) VALUES ('E2ETEST', 1000, 1)")
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

async fn request(
    app: &mut Router,
    method: Method,
    uri: &str,
    body: Option<Value>,
    token: &str,
) -> (StatusCode, Value) {
    let mut builder = Request::builder()
        .method(method)
        .uri(uri)
        .header(header::CONTENT_TYPE, "application/json");

    if !token.is_empty() {
        builder = builder.header(header::AUTHORIZATION, format!("Bearer {token}"));
    }

    let req = if let Some(b) = body {
        builder
            .body(Body::from(serde_json::to_string(&b).unwrap()))
            .unwrap()
    } else {
        builder.body(Body::empty()).unwrap()
    };

    let resp = app.oneshot(req).await.unwrap();
    let status = resp.status();
    let body_bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    let val: Value = serde_json::from_slice(&body_bytes).unwrap_or(json!({}));
    (status, val)
}

async fn register_user(app: &mut Router, username: &str, password: &str) -> (String, String) {
    let (status, body) = request(
        app,
        Method::POST,
        "/auth/register",
        Some(json!({
            "username": username,
            "password": password,
            "invite_code": "E2ETEST"
        })),
        "",
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "Register failed: {body}");
    let access = body["access_token"].as_str().unwrap().to_string();
    let refresh = body["refresh_token"].as_str().unwrap().to_string();
    (access, refresh)
}

// ---------------------------------------------------------------------------
// Channel Flow
// ---------------------------------------------------------------------------

#[tokio::test]
async fn channel_lifecycle_integration_flow() {
    ensure_env();
    let pool = setup_pool().await;
    let mut app = build_app(make_state(pool.clone()));

    // ── Step 1: Register two users ──────────────────────────────────
    let (owner_token, _) = register_user(&mut app, "channelowner", "OwnerPass123").await;
    let (member_token, _) = register_user(&mut app, "channeljoiner", "JoinerPass123").await;

    // ── Step 2: Create channel ─────────────────────────────────────
    let (status, body) = request(
        &mut app,
        Method::POST,
        "/channels",
        Some(json!({
            "name": "Integration Channel",
            "description": "E2E test channel"
        })),
        &owner_token,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let channel_id = body["id"].as_str().unwrap().to_string();
    assert_eq!(body["name"], "Integration Channel");
    assert!(!body["is_archived"].as_bool().unwrap());

    // ── Step 3: Second user joins via join-request + invite ─────
    // Create a join request
    let (status, body) = request(
        &mut app,
        Method::POST,
        &format!("/channels/{channel_id}/join-request"),
        None,
        &member_token,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "Join request failed: {body}");
    let request_id = body["id"].as_str().unwrap().to_string();

    // Owner approves the request
    let (status, _) = request(
        &mut app,
        Method::PUT,
        &format!("/requests/{request_id}/approve"),
        None,
        &owner_token,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "Approve should succeed");

    // Verify second user can now list the channel
    let (status, body) = request(
        &mut app,
        Method::GET,
        &format!("/channels/{channel_id}"),
        None,
        &member_token,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "Member should be able to GET channel");
    assert_eq!(body["role"], "member");

    // ── Step 4: Send messages ─────────────────────────────────────
    let (status, body) = request(
        &mut app,
        Method::POST,
        &format!("/channels/{channel_id}/messages"),
        Some(json!({
            "msg_type": "text",
            "payload": {"text": "Hello from integration test!"}
        })),
        &owner_token,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "Send message failed: {body}");
    let _first_msg_id = body["id"].as_i64().unwrap();
    assert_eq!(body["msg_type"], "text");
    assert_eq!(body["payload"]["text"], "Hello from integration test!");

    // Member also sends a message
    let (status, _) = request(
        &mut app,
        Method::POST,
        &format!("/channels/{channel_id}/messages"),
        Some(json!({
            "msg_type": "text",
            "payload": {"text": "Member reply!"}
        })),
        &member_token,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);

    // ── Step 5: Read messages ─────────────────────────────────────
    let (status, body) = request(
        &mut app,
        Method::GET,
        &format!("/channels/{channel_id}/messages"),
        None,
        &owner_token,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let messages = body["messages"].as_array().unwrap();
    // 3 messages expected: join_request (from join-request flow) + 2 text messages
    assert_eq!(messages.len(), 3);
    assert_eq!(messages[1]["payload"]["text"], "Hello from integration test!");
    assert_eq!(messages[2]["payload"]["text"], "Member reply!");
    assert!(body["next_cursor"].as_i64().unwrap() >= 2);
    assert!(!body["has_more"].as_bool().unwrap());

    // ── Step 6: Archive channel ───────────────────────────────────
    let (status, _) = request(
        &mut app,
        Method::POST,
        &format!("/channels/{channel_id}/archive"),
        None,
        &owner_token,
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    // Verify archived
    let (status, body) = request(
        &mut app,
        Method::GET,
        &format!("/channels/{channel_id}"),
        None,
        &owner_token,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(body["is_archived"].as_bool().unwrap());

    // Archived channel blocks new messages
    let (status, _) = request(
        &mut app,
        Method::POST,
        &format!("/channels/{channel_id}/messages"),
        Some(json!({
            "msg_type": "text",
            "payload": {"text": "should be blocked"}
        })),
        &owner_token,
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "Archived channel should block writes");

    // Archived channel still allows reads
    let (status, _) = request(
        &mut app,
        Method::GET,
        &format!("/channels/{channel_id}/messages"),
        None,
        &owner_token,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "Archived channel should allow reads");

    // Unarchive
    let (status, _) = request(
        &mut app,
        Method::POST,
        &format!("/channels/{channel_id}/unarchive"),
        None,
        &owner_token,
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    // Verify unarchived
    let (_status, body) = request(
        &mut app,
        Method::GET,
        &format!("/channels/{channel_id}"),
        None,
        &owner_token,
    )
    .await;
    assert!(!body["is_archived"].as_bool().unwrap());
}

#[tokio::test]
async fn non_member_cannot_access_channel() {
    ensure_env();
    let pool = setup_pool().await;
    let mut app = build_app(make_state(pool));

    let (owner_token, _) = register_user(&mut app, "privateowner", "PrivateOwner1").await;
    let (stranger_token, _) = register_user(&mut app, "stranger", "StrangerPass1").await;

    // Create private channel
    let (_, body) = request(
        &mut app,
        Method::POST,
        "/channels",
        Some(json!({"name": "Private"})),
        &owner_token,
    )
    .await;
    let channel_id = body["id"].as_str().unwrap().to_string();

    // Stranger tries to get the channel
    let (status, _) = request(
        &mut app,
        Method::GET,
        &format!("/channels/{channel_id}"),
        None,
        &stranger_token,
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn non_owner_cannot_archive() {
    ensure_env();
    let pool = setup_pool().await;
    let mut app = build_app(make_state(pool));

    let (owner_token, _) = register_user(&mut app, "archiveowner", "ArchiveOwner1").await;
    let (member_token, _) = register_user(&mut app, "archivemember", "ArchiveMember1").await;

    let (_, body) = request(
        &mut app,
        Method::POST,
        "/channels",
        Some(json!({"name": "Archive Test"})),
        &owner_token,
    )
    .await;
    let channel_id = body["id"].as_str().unwrap().to_string();

    // Add member
    let (_status, body) = request(
        &mut app,
        Method::POST,
        &format!("/channels/{channel_id}/join-request"),
        None,
        &member_token,
    )
    .await;
    let request_id = body["id"].as_str().unwrap().to_string();
    let _ = request(
        &mut app,
        Method::PUT,
        &format!("/requests/{request_id}/approve"),
        None,
        &owner_token,
    )
    .await;

    // Member tries to archive
    let (status, _) = request(
        &mut app,
        Method::POST,
        &format!("/channels/{channel_id}/archive"),
        None,
        &member_token,
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "Non-owner should not be able to archive");
}
