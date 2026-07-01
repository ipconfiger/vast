//! Integration test: Direct Message lifecycle
//!   create DM → reuse existing → stranger blocked → group DM
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

fn user_id_from_token(token: &str) -> String {
    let secret = std::env::var("JWT_SECRET").unwrap();
    im_server::auth::validate_token(token, &secret)
        .expect("Valid token expected")
        .sub
}

// ---------------------------------------------------------------------------
// DM Flow
// ---------------------------------------------------------------------------

#[tokio::test]
async fn dm_create_and_reuse() {
    ensure_env();
    let pool = setup_pool().await;
    let mut app = build_app(make_state(pool));

    let (token1, _) = register_user(&mut app, "dmcu1", "DmCuPass1").await;
    let (token2, _) = register_user(&mut app, "dmcu2", "DmCuPass2").await;

    let id1 = user_id_from_token(&token1);
    let id2 = user_id_from_token(&token2);

    // Create DM → expect 201
    let (status, body) = request(
        &mut app,
        Method::POST,
        "/dm",
        Some(json!({"user_ids": [id1, id2]})),
        &token1,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "First DM create: {body}");
    let channel_id = body["id"].as_str().unwrap().to_string();
    assert_eq!(body["is_direct"], true);
    assert_eq!(body["is_group_dm"], false);

    // Create same DM again → same channel ID returned, status 200
    let (status, body) = request(
        &mut app,
        Method::POST,
        "/dm",
        Some(json!({"user_ids": [id1, id2]})),
        &token1,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "Reuse DM: {body}");
    assert_eq!(
        body["id"], channel_id,
        "Same channel ID should be returned on reuse"
    );

    // List DMs via GET /dm → should contain the DM
    let (status, body) = request(&mut app, Method::GET, "/dm", None, &token1).await;
    assert_eq!(status, StatusCode::OK);
    let dms = body["channels"].as_array().unwrap();
    assert_eq!(dms.len(), 1, "Should have 1 DM");
    assert_eq!(dms[0]["is_direct"], true);

    // DM should NOT appear in regular GET /channels listing
    let (status, body) = request(&mut app, Method::GET, "/channels", None, &token1).await;
    assert_eq!(status, StatusCode::OK);
    let channels = body["channels"].as_array().unwrap();
    let dm_in_channels = channels.iter().any(|c| c["id"] == channel_id);
    assert!(!dm_in_channels, "DM should not appear in regular channel listing");
}

#[tokio::test]
async fn dm_stranger_cannot_access() {
    ensure_env();
    let pool = setup_pool().await;
    let mut app = build_app(make_state(pool));

    let (token1, _) = register_user(&mut app, "dmsa1", "DmSaPass1").await;
    let (token2, _) = register_user(&mut app, "dmsa2", "DmSaPass2").await;
    let (token3, _) = register_user(&mut app, "dmsa3", "DmSaPass3").await;

    let id1 = user_id_from_token(&token1);
    let id2 = user_id_from_token(&token2);

    // Create DM between user1 and user2
    let (_, body) = request(
        &mut app,
        Method::POST,
        "/dm",
        Some(json!({"user_ids": [id1, id2]})),
        &token1,
    )
    .await;
    let channel_id = body["id"].as_str().unwrap().to_string();

    // User3 tries to send a message to the DM channel → 403 FORBIDDEN
    let (status, body) = request(
        &mut app,
        Method::POST,
        &format!("/channels/{channel_id}/messages"),
        Some(json!({"msg_type": "text", "payload": {"text": "intruder!"}})),
        &token3,
    )
    .await;
    assert_eq!(
        status, StatusCode::FORBIDDEN,
        "Stranger should be blocked: {body}"
    );
}

#[tokio::test]
async fn group_dm_create_and_send() {
    ensure_env();
    let pool = setup_pool().await;
    let mut app = build_app(make_state(pool));

    let (token1, _) = register_user(&mut app, "gdmu1", "GdmPasswd1").await;
    let (token2, _) = register_user(&mut app, "gdmu2", "GdmPasswd2").await;
    let (token3, _) = register_user(&mut app, "gdmu3", "GdmPasswd3").await;

    let id1 = user_id_from_token(&token1);
    let id2 = user_id_from_token(&token2);
    let id3 = user_id_from_token(&token3);

    // Create group DM with 3 users
    let (status, body) = request(
        &mut app,
        Method::POST,
        "/dm",
        Some(json!({"user_ids": [id1, id2, id3], "name": "Team Chat"})),
        &token1,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "Group DM create: {body}");
    let channel_id = body["id"].as_str().unwrap().to_string();
    assert_eq!(body["is_direct"], true);
    assert_eq!(body["is_group_dm"], true, "Should be a group DM");
    assert_eq!(body["name"], "Team Chat");

    // All 3 users can send messages
    for (token, user) in [(&token1, "user1"), (&token2, "user2"), (&token3, "user3")]
    {
        let (status, body) = request(
            &mut app,
            Method::POST,
            &format!("/channels/{channel_id}/messages"),
            Some(json!({"msg_type": "text", "payload": {"text": format!("{user} says hello")}})),
            token,
        )
        .await;
        assert_eq!(status, StatusCode::CREATED, "User {user} send failed: {body}");
    }

    // Verify all 3 messages are visible
    let (status, body) = request(
        &mut app,
        Method::GET,
        &format!("/channels/{channel_id}/messages"),
        None,
        &token1,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let messages = body["messages"].as_array().unwrap();
    assert_eq!(messages.len(), 3, "All 3 messages should be visible");
}
