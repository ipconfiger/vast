//! Integration test: message lifecycle
//!   send → cursor pagination → soft delete → FTS5 search
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
        // SAFETY: test-only; single-threaded
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

async fn create_channel(app: &mut Router, token: &str, name: &str) -> String {
    let (_, body) = request(
        app,
        Method::POST,
        "/channels",
        Some(json!({"name": name})),
        token,
    )
    .await;
    body["id"].as_str().unwrap().to_string()
}

async fn send_message(app: &mut Router, token: &str, channel_id: &str, text: &str) -> Value {
    let (status, body) = request(
        app,
        Method::POST,
        &format!("/channels/{channel_id}/messages"),
        Some(json!({
            "msg_type": "text",
            "payload": {"text": text}
        })),
        token,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "Send message failed: {body}");
    body
}

// ---------------------------------------------------------------------------
// Message Flow
// ---------------------------------------------------------------------------

#[tokio::test]
async fn message_lifecycle_integration_flow() {
    ensure_env();
    let pool = setup_pool().await;
    let mut app = build_app(make_state(pool));

    let (token, _) = register_user(&mut app, "msguser", "MsgUserPass1").await;
    let channel_id = create_channel(&mut app, &token, "Message Test").await;

    // ── Step 1: Send messages ──────────────────────────────────────
    let msg1 = send_message(&mut app, &token, &channel_id, "first message").await;
    let msg2 = send_message(&mut app, &token, &channel_id, "second message").await;
    let msg3 = send_message(&mut app, &token, &channel_id, "third message").await;
    let msg1_id = msg1["id"].as_i64().unwrap();
    let msg2_id = msg2["id"].as_i64().unwrap();
    let msg3_id = msg3["id"].as_i64().unwrap();

    // Verify ids are sequential
    assert!(msg2_id > msg1_id);
    assert!(msg3_id > msg2_id);

    // ── Step 2: Cursor pagination ──────────────────────────────────
    // Get messages with cursor=msg1_id, limit=1
    let (status, body) = request(
        &mut app,
        Method::GET,
        &format!("/channels/{channel_id}/messages?after_cursor={msg1_id}&limit=1"),
        None,
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let paginated = body["messages"].as_array().unwrap();
    assert_eq!(paginated.len(), 1, "Limit should be 1");
    assert_eq!(paginated[0]["payload"]["text"], "second message");
    assert!(body["has_more"].as_bool().unwrap(), "Should have more messages");
    assert_eq!(body["next_cursor"].as_i64().unwrap(), msg2_id);

    // Page again to get the third
    let (status, body) = request(
        &mut app,
        Method::GET,
        &format!("/channels/{channel_id}/messages?after_cursor={msg2_id}&limit=1"),
        None,
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let paginated = body["messages"].as_array().unwrap();
    assert_eq!(paginated.len(), 1);
    assert_eq!(paginated[0]["payload"]["text"], "third message");
    assert!(!body["has_more"].as_bool().unwrap(), "Should have no more");

    // Get all messages (no cursor, default limit)
    let (status, body) = request(
        &mut app,
        Method::GET,
        &format!("/channels/{channel_id}/messages"),
        None,
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["messages"].as_array().unwrap().len(), 3);

    // ── Step 3: Soft delete ────────────────────────────────────────
    // Delete the second message
    let (status, _) = request(
        &mut app,
        Method::DELETE,
        &format!("/messages/{msg2_id}"),
        None,
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);

    // Deleted message no longer appears in listing
    let (status, body) = request(
        &mut app,
        Method::GET,
        &format!("/channels/{channel_id}/messages"),
        None,
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let messages = body["messages"].as_array().unwrap();
    let texts: Vec<&str> = messages
        .iter()
        .map(|m| m["payload"]["text"].as_str().unwrap())
        .collect();
    assert_eq!(texts, vec!["first message", "third message"],
        "Deleted message should be excluded from listing");

    // Deleting already-deleted returns 404
    let (status, _) = request(
        &mut app,
        Method::DELETE,
        &format!("/messages/{msg2_id}"),
        None,
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    // Deleting non-existent returns 404
    let (status, _) = request(
        &mut app,
        Method::DELETE,
        "/messages/999999",
        None,
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    // ── Step 4: FTS5 search ──────────────────────────────────────
    // Wait a brief moment to ensure FTS5 triggers have completed
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    // Search for "first"
    let (status, body) = request(
        &mut app,
        Method::GET,
        "/search?q=first",
        None,
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "Search failed: {body}");
    let results = body["results"].as_array().unwrap();
    assert_eq!(results.len(), 1, "Should find exactly 1 result for 'first'");
    assert!(results[0]["snippet"].as_str().unwrap().contains("first"));

    // Search for "message" — should match both remaining messages
    let (status, body) = request(
        &mut app,
        Method::GET,
        "/search?q=message",
        None,
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let results = body["results"].as_array().unwrap();
    assert_eq!(results.len(), 2, "Should find both messages containing 'message'");

    // Deleted message should NOT appear in search
    let (status, body) = request(
        &mut app,
        Method::GET,
        "/search?q=second",
        None,
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let results = body["results"].as_array().unwrap();
    assert_eq!(results.len(), 0, "Deleted message 'second' should not appear in search");

    // Search with channel filter
    let (status, body) = request(
        &mut app,
        Method::GET,
        &format!("/search?q=message&channel_id={channel_id}"),
        None,
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let results = body["results"].as_array().unwrap();
    assert_eq!(results.len(), 2, "Channel filter should return results");

    // Prefix search
    let (status, body) = request(
        &mut app,
        Method::GET,
        "/search?q=thi*",
        None,
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let results = body["results"].as_array().unwrap();
    assert_eq!(results.len(), 1, "Prefix 'thi*' should match 'third'");
    assert!(results[0]["snippet"].as_str().unwrap().contains("third"));
}

#[tokio::test]
async fn message_cannot_be_deleted_by_other_user() {
    ensure_env();
    let pool = setup_pool().await;
    let mut app = build_app(make_state(pool));

    let (token1, _) = register_user(&mut app, "deluser1", "DelUser1Pass").await;
    let (token2, _) = register_user(&mut app, "deluser2", "DelUser2Pass").await;

    let channel_id = create_channel(&mut app, &token1, "Delete Test").await;

    // Add user2 via join-request
    let (_, body) = request(
        &mut app,
        Method::POST,
        &format!("/channels/{channel_id}/join-request"),
        None,
        &token2,
    )
    .await;
    let req_id = body["id"].as_str().unwrap().to_string();
    let _ = request(
        &mut app,
        Method::PUT,
        &format!("/requests/{req_id}/approve"),
        None,
        &token1,
    )
    .await;

    // user1 sends a message
    let msg = send_message(&mut app, &token1, &channel_id, "user1 message").await;
    let msg_id = msg["id"].as_i64().unwrap();

    // user2 tries to delete user1's message
    let (status, _) = request(
        &mut app,
        Method::DELETE,
        &format!("/messages/{msg_id}"),
        None,
        &token2,
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "Cannot delete another user's message");
}

#[tokio::test]
async fn thread_replies_flow() {
    ensure_env();
    let pool = setup_pool().await;
    let mut app = build_app(make_state(pool));

    let (token, _) = register_user(&mut app, "threaduser", "ThreadUser1").await;
    let channel_id = create_channel(&mut app, &token, "Thread Test").await;

    // Send parent message
    let parent = send_message(&mut app, &token, &channel_id, "parent message").await;
    let parent_id = parent["id"].as_i64().unwrap();

    // Send thread replies
    for text in &["reply one", "reply two"] {
        let (status, _) = request(
            &mut app,
            Method::POST,
            &format!("/channels/{channel_id}/messages"),
            Some(json!({
                "msg_type": "text",
                "payload": {"text": text},
                "thread_parent_id": parent_id,
            })),
            &token,
        )
        .await;
        assert_eq!(status, StatusCode::CREATED);
    }

    // Thread replies excluded from channel listing
    let (_, body) = request(
        &mut app,
        Method::GET,
        &format!("/channels/{channel_id}/messages"),
        None,
        &token,
    )
    .await;
    let messages = body["messages"].as_array().unwrap();
    // Only parent message should be in the channel listing
    let top_level: Vec<&str> = messages
        .iter()
        .map(|m| m["payload"]["text"].as_str().unwrap())
        .collect();
    assert_eq!(top_level, vec!["parent message"], "Thread replies excluded from list");

    // Get thread replies
    let (status, body) = request(
        &mut app,
        Method::GET,
        &format!("/channels/{channel_id}/messages/{parent_id}/thread"),
        None,
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let replies = body["messages"].as_array().unwrap();
    assert_eq!(replies.len(), 2);
    assert_eq!(replies[0]["payload"]["text"], "reply one");
    assert_eq!(replies[1]["payload"]["text"], "reply two");
    for reply in replies {
        assert_eq!(reply["thread_parent_id"].as_i64().unwrap(), parent_id);
    }
}
