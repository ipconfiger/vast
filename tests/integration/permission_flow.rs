//! Integration test: permission lifecycle
//!   join request → approve → role change → kick
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
    assert_eq!(status, StatusCode::CREATED, "Register failed for {username}: {body}");
    let access = body["access_token"].as_str().unwrap().to_string();
    let refresh = body["refresh_token"].as_str().unwrap().to_string();
    (access, refresh)
}

// ---------------------------------------------------------------------------
// Permission Flow
// ---------------------------------------------------------------------------

#[tokio::test]
async fn permission_lifecycle_integration_flow() {
    ensure_env();
    let pool = setup_pool().await;
    let mut app = build_app(make_state(pool));

    // ── Step 1: Register owner and create channel ───────────────────
    let (owner_token, _) = register_user(&mut app, "permowner", "PermOwner123").await;
    let (user_token, _) = register_user(&mut app, "permuser", "PermUser1234").await;

    let (_, body) = request(
        &mut app,
        Method::POST,
        "/channels",
        Some(json!({"name": "Permission Test"})),
        &owner_token,
    )
    .await;
    let channel_id = body["id"].as_str().unwrap().to_string();

    // ── Step 2: User submits join request ──────────────────────────
    let (status, body) = request(
        &mut app,
        Method::POST,
        &format!("/channels/{channel_id}/join-request"),
        None,
        &user_token,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "Join request should be created");
    assert_eq!(body["status"], "pending");
    let request_id = body["id"].as_str().unwrap().to_string();

    // ── Step 3: Owner approves join request ────────────────────────
    let (status, _) = request(
        &mut app,
        Method::PUT,
        &format!("/requests/{request_id}/approve"),
        None,
        &owner_token,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "Approve should succeed");

    // Verify user is now a member with "member" role
    let (status, body) = request(
        &mut app,
        Method::GET,
        &format!("/channels/{channel_id}"),
        None,
        &user_token,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["role"], "member", "New member should have 'member' role");

    // ── Step 4: Owner promotes user to admin ───────────────────────
    // First, find the user_id. We can get it from the channels API.
    // The member listing is available at /channels/{channel_id}/members
    let (status, body) = request(
        &mut app,
        Method::GET,
        &format!("/channels/{channel_id}/members"),
        None,
        &owner_token,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let members = body.as_array().expect("Expected array of members");
    // Find the user we just added
    let target_user_id = members
        .iter()
        .find(|m| m["username"] == "permuser")
        .map(|m| m["user_id"].as_str().unwrap().to_string())
        .expect("Should find permuser in members list");

    // Change role to admin
    let (status, _) = request(
        &mut app,
        Method::PATCH,
        &format!("/channels/{channel_id}/members/{target_user_id}/role"),
        Some(json!({"role": "admin"})),
        &owner_token,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "Role change should succeed");

    // Verify role changed
    let (status, body) = request(
        &mut app,
        Method::GET,
        &format!("/channels/{channel_id}"),
        None,
        &user_token,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["role"], "admin", "Role should be 'admin' after promotion");

    // ── Step 5: Owner kicks (removes) the user ────────────────────
    let (status, _) = request(
        &mut app,
        Method::DELETE,
        &format!("/channels/{channel_id}/members/{target_user_id}"),
        None,
        &owner_token,
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT, "Kick should return 204");

    // Verify kicked user can no longer access the channel
    let (status, _) = request(
        &mut app,
        Method::GET,
        &format!("/channels/{channel_id}"),
        None,
        &user_token,
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "Kicked user should be blocked");
}

#[tokio::test]
async fn non_owner_cannot_change_roles() {
    ensure_env();
    let pool = setup_pool().await;
    let mut app = build_app(make_state(pool));

    let (owner_token, _) = register_user(&mut app, "roleowner", "RoleOwner12").await;
    let (member1_token, _) = register_user(&mut app, "rolemember1", "RoleMember12").await;
    let (member2_token, _) = register_user(&mut app, "rolemember2", "RoleMembr34").await;

    let (_, body) = request(
        &mut app,
        Method::POST,
        "/channels",
        Some(json!({"name": "Role Test"})),
        &owner_token,
    )
    .await;
    let channel_id = body["id"].as_str().unwrap().to_string();

    // Add both members via join-request + approve
    for token in [&member1_token, &member2_token] {
        let (_, body) = request(
            &mut app,
            Method::POST,
            &format!("/channels/{channel_id}/join-request"),
            None,
            token,
        )
        .await;
        let req_id = body["id"].as_str().unwrap().to_string();
        let _ = request(
            &mut app,
            Method::PUT,
            &format!("/requests/{req_id}/approve"),
            None,
            &owner_token,
        )
        .await;
    }

    // Get member2's user_id from members list
    let (_, body) = request(
        &mut app,
        Method::GET,
        &format!("/channels/{channel_id}/members"),
        None,
        &owner_token,
    )
    .await;
    let members = body.as_array().unwrap();
    let member2_id = members
        .iter()
        .find(|m| m["username"] == "rolemember2")
        .map(|m| m["user_id"].as_str().unwrap().to_string())
        .unwrap();

    // member1 (not owner) tries to change member2's role
    let (status, _) = request(
        &mut app,
        Method::PATCH,
        &format!("/channels/{channel_id}/members/{member2_id}/role"),
        Some(json!({"role": "admin"})),
        &member1_token,
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "Non-owner should not be able to change roles");
}

#[tokio::test]
async fn non_owner_cannot_kick() {
    ensure_env();
    let pool = setup_pool().await;
    let mut app = build_app(make_state(pool));

    let (owner_token, _) = register_user(&mut app, "kickowner", "KickOwner12").await;
    let (member1_token, _) = register_user(&mut app, "kickmember1", "KickMembr12").await;
    let (member2_token, _) = register_user(&mut app, "kickmember2", "KickMembr34").await;

    let (_, body) = request(
        &mut app,
        Method::POST,
        "/channels",
        Some(json!({"name": "Kick Test"})),
        &owner_token,
    )
    .await;
    let channel_id = body["id"].as_str().unwrap().to_string();

    // Add both members
    for token in [&member1_token, &member2_token] {
        let (_, body) = request(
            &mut app,
            Method::POST,
            &format!("/channels/{channel_id}/join-request"),
            None,
            token,
        )
        .await;
        let req_id = body["id"].as_str().unwrap().to_string();
        let _ = request(
            &mut app,
            Method::PUT,
            &format!("/requests/{req_id}/approve"),
            None,
            &owner_token,
        )
        .await;
    }

    // Get member1's user_id
    let (_, body) = request(
        &mut app,
        Method::GET,
        &format!("/channels/{channel_id}/members"),
        None,
        &owner_token,
    )
    .await;
    let members = body.as_array().unwrap();
    let member2_id = members
        .iter()
        .find(|m| m["username"] == "kickmember2")
        .map(|m| m["user_id"].as_str().unwrap().to_string())
        .unwrap();

    // member1 tries to kick member2
    let (status, _) = request(
        &mut app,
        Method::DELETE,
        &format!("/channels/{channel_id}/members/{member2_id}"),
        None,
        &member1_token,
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "Non-owner should not be able to kick");
}
