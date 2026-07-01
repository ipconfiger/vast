//! Integration test: Reactions lifecycle
//!   add reaction → count increments → idempotent → per-user removal
use axum::{
    body::Body,
    http::{header, Method, Request, StatusCode},
    Router,
};
use im_server::{AppConfig, AppState, TlsMode};
use serde_json::{json, Value};
use std::sync::Arc;
use tower::ServiceExt;

// ---------------------------------------------------------------------------
// Helpers
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
            data_dir: std::path::PathBuf::from("/tmp"),
            jwt_secret: "integration-test-secret".to_string(),
            invite_code: "E2ETEST".to_string(),
            tls_mode: TlsMode::None,
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

/// Percent-encode a string for use in a URI path (emoji support).
fn pct_encode(s: &str) -> String {
    s.bytes()
        .map(|b| {
            if b.is_ascii_alphanumeric() || b == b'-' || b == b'_' || b == b'.' || b == b'~' {
                (b as char).to_string()
            } else {
                format!("%{:02X}", b)
            }
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Reactions Flow
// ---------------------------------------------------------------------------

#[tokio::test]
async fn reaction_add_count_remove() {
    ensure_env();
    let pool = setup_pool().await;
    let mut app = build_app(make_state(pool));

    let (token1, _) = register_user(&mut app, "rct1a", "Rct1aPass1").await;
    let (token2, _) = register_user(&mut app, "rct2a", "Rct2aPass1").await;

    // Create channel and add user2 via join-request
    let (_, body) = request(
        &mut app,
        Method::POST,
        "/channels",
        Some(json!({"name": "Reaction Test"})),
        &token1,
    )
    .await;
    let channel_id = body["id"].as_str().unwrap().to_string();

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

    // User1 sends a message
    let (_, msg_body) = request(
        &mut app,
        Method::POST,
        &format!("/channels/{channel_id}/messages"),
        Some(json!({"msg_type": "text", "payload": {"text": "react to this"}})),
        &token1,
    )
    .await;
    let message_id = msg_body["id"].as_i64().unwrap();

    // User1 adds thumbs up
    let (status, _) = request(
        &mut app,
        Method::POST,
        &format!("/messages/{message_id}/reactions"),
        Some(json!({"emoji": "👍"})),
        &token1,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);

    // Verify count=1, reacted_by_me=true for user1
    let (_, body) = request(
        &mut app,
        Method::GET,
        &format!("/messages/{message_id}/reactions"),
        None,
        &token1,
    )
    .await;
    let reactions = body["reactions"].as_array().unwrap();
    assert_eq!(reactions.len(), 1);
    assert_eq!(reactions[0]["emoji"], "👍");
    assert_eq!(reactions[0]["count"], 1);
    assert_eq!(reactions[0]["reacted_by_me"], true);

    // User2 adds thumbs up
    let (status, _) = request(
        &mut app,
        Method::POST,
        &format!("/messages/{message_id}/reactions"),
        Some(json!({"emoji": "👍"})),
        &token2,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);

    // Verify count=2
    let (_, body) = request(
        &mut app,
        Method::GET,
        &format!("/messages/{message_id}/reactions"),
        None,
        &token1,
    )
    .await;
    let reactions = body["reactions"].as_array().unwrap();
    assert_eq!(reactions.len(), 1);
    assert_eq!(reactions[0]["count"], 2, "Count should be 2 after second user");

    // User1 removes their thumbs up
    let (status, _) = request(
        &mut app,
        Method::DELETE,
        &format!("/messages/{message_id}/reactions/{}", pct_encode("👍")),
        None,
        &token1,
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    // Verify count=1, reacted_by_me=false for user1
    let (_, body) = request(
        &mut app,
        Method::GET,
        &format!("/messages/{message_id}/reactions"),
        None,
        &token1,
    )
    .await;
    let reactions = body["reactions"].as_array().unwrap();
    assert_eq!(reactions.len(), 1, "Should still have user2's reaction");
    assert_eq!(reactions[0]["count"], 1, "Count should be 1 after user1 removed");
    assert_eq!(reactions[0]["reacted_by_me"], false);
}

#[tokio::test]
async fn reaction_idempotent_duplicate() {
    ensure_env();
    let pool = setup_pool().await;
    let mut app = build_app(make_state(pool));

    let (token, _) = register_user(&mut app, "rctidem", "RctIdem12").await;

    let (_, body) = request(
        &mut app,
        Method::POST,
        "/channels",
        Some(json!({"name": "Idempotent Test"})),
        &token,
    )
    .await;
    let channel_id = body["id"].as_str().unwrap().to_string();

    let (_, msg_body) = request(
        &mut app,
        Method::POST,
        &format!("/channels/{channel_id}/messages"),
        Some(json!({"msg_type": "text", "payload": {"text": "idempotent reaction"}})),
        &token,
    )
    .await;
    let message_id = msg_body["id"].as_i64().unwrap();

    // Add heart twice
    for _ in 0..2 {
        let (status, _) = request(
            &mut app,
            Method::POST,
            &format!("/messages/{message_id}/reactions"),
            Some(json!({"emoji": "❤️"})),
            &token,
        )
        .await;
        assert_eq!(status, StatusCode::CREATED);
    }

    // Count should still be 1 (INSERT OR IGNORE makes it idempotent)
    let (_, body) = request(
        &mut app,
        Method::GET,
        &format!("/messages/{message_id}/reactions"),
        None,
        &token,
    )
    .await;
    let reactions = body["reactions"].as_array().unwrap();
    assert_eq!(reactions.len(), 1);
    assert_eq!(reactions[0]["emoji"], "❤️");
    assert_eq!(reactions[0]["count"], 1, "Duplicate add should be idempotent");
}

#[tokio::test]
async fn reaction_other_user_cannot_remove() {
    ensure_env();
    let pool = setup_pool().await;
    let mut app = build_app(make_state(pool));

    let (token1, _) = register_user(&mut app, "rct3a", "Rct3aPass1").await;
    let (token2, _) = register_user(&mut app, "rct4a", "Rct4aPass1").await;

    // Create channel and add user2
    let (_, body) = request(
        &mut app,
        Method::POST,
        "/channels",
        Some(json!({"name": "Remove Test"})),
        &token1,
    )
    .await;
    let channel_id = body["id"].as_str().unwrap().to_string();

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

    // User1 sends a message
    let (_, msg_body) = request(
        &mut app,
        Method::POST,
        &format!("/channels/{channel_id}/messages"),
        Some(json!({"msg_type": "text", "payload": {"text": "my reaction"}})),
        &token1,
    )
    .await;
    let message_id = msg_body["id"].as_i64().unwrap();

    // User1 adds fire
    let _ = request(
        &mut app,
        Method::POST,
        &format!("/messages/{message_id}/reactions"),
        Some(json!({"emoji": "🔥"})),
        &token1,
    )
    .await;

    // User2 tries to remove user1's reaction
    let (status, _) = request(
        &mut app,
        Method::DELETE,
        &format!("/messages/{message_id}/reactions/{}", pct_encode("🔥")),
        None,
        &token2,
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    // User1's reaction should still be there
    let (_, body) = request(
        &mut app,
        Method::GET,
        &format!("/messages/{message_id}/reactions"),
        None,
        &token1,
    )
    .await;
    let reactions = body["reactions"].as_array().unwrap();
    assert_eq!(reactions.len(), 1, "User1's reaction should still exist");
    assert_eq!(reactions[0]["emoji"], "🔥");
    assert_eq!(reactions[0]["count"], 1, "Count should remain 1");
}
