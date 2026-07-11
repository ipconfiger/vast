use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use serde::Serialize;
use std::sync::Arc;
use uuid::Uuid;

use crate::auth::middleware::{require_role, AuthenticatedUser};
use crate::error::AppError;
use crate::ws::protocol::ServerEvent;
use crate::AppState;

// ---------------------------------------------------------------------------
// Request / Response types
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
pub struct JoinRequestResponse {
    pub id: String,
    pub channel_id: String,
    pub user_id: String,
    pub username: String,
    pub status: String,
    pub created_at: i64,
}

#[derive(Debug, Serialize)]
pub struct ListJoinRequestsResponse {
    pub requests: Vec<JoinRequestResponse>,
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

/// POST /api/channels/{id}/join-request
///
/// Create a pending join request. Requires authentication.
/// Validates the user is not already a member and no duplicate pending request exists.
/// Notifies the channel owner via WebSocket.
pub async fn create_join_request(
    State(state): State<Arc<AppState>>,
    user: AuthenticatedUser,
    Path(channel_id): Path<String>,
) -> Result<(StatusCode, Json<JoinRequestResponse>), AppError> {
    // Check channel exists
    let _ = sqlx::query_scalar::<_, String>("SELECT id FROM channels WHERE id = ?")
        .bind(&channel_id)
        .fetch_optional(&state.pool)
        .await?
        .ok_or_else(|| AppError::NotFound("Channel not found".to_string()))?;

    // Check not already a member
    let is_member = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM channel_members WHERE channel_id = ? AND user_id = ?",
    )
    .bind(&channel_id)
    .bind(&user.0)
    .fetch_one(&state.pool)
    .await?;

    if is_member > 0 {
        return Err(AppError::Conflict(
            "You are already a member of this channel".to_string(),
        ));
    }

    // Check no duplicate pending request
    let pending = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM join_requests WHERE channel_id = ? AND user_id = ? AND status = 'pending'",
    )
    .bind(&channel_id)
    .bind(&user.0)
    .fetch_one(&state.pool)
    .await?;

    if pending > 0 {
        return Err(AppError::Conflict(
            "You already have a pending join request for this channel".to_string(),
        ));
    }

    let request_id = Uuid::new_v4().to_string();
    let now = chrono::Utc::now().timestamp();

    sqlx::query(
        "INSERT INTO join_requests (id, channel_id, user_id, status, created_at) VALUES (?, ?, ?, 'pending', ?)",
    )
    .bind(&request_id)
    .bind(&channel_id)
    .bind(&user.0)
    .bind(now)
    .execute(&state.pool)
    .await?;

    // Get username for response and WS notification
    let username = sqlx::query_scalar::<_, String>("SELECT username FROM users WHERE id = ?")
        .bind(&user.0)
        .fetch_one(&state.pool)
        .await?;

    // Insert a join_request message into the channel so the owner can see
    // and act on it directly from the message list.
    // Uses msg_type "text" with _join_request marker in payload to avoid
    // needing a schema migration for the CHECK constraint on msg_type.
    let msg_id = Uuid::new_v4().to_string();
    let payload = serde_json::json!({
        "_join_request": true,
        "request_id": &request_id,
        "username": &username,
        "status": "pending"
    });
    sqlx::query(
        "INSERT INTO messages (msg_id, channel_id, sender_id, msg_type, payload) VALUES (?, ?, ?, 'text', ?)",
    )
    .bind(&msg_id)
    .bind(&channel_id)
    .bind(&user.0)
    .bind(serde_json::to_string(&payload).unwrap_or_default())
    .execute(&state.pool)
    .await?;

    // WS notify the channel (owner/admin will see the JoinRequest event)
    state.ws_pool.notify_channel(
        &channel_id,
        ServerEvent::JoinRequest {
            channel_id: channel_id.clone(),
            user_id: user.0.clone(),
            username: username.clone(),
        },
    );

    let response = JoinRequestResponse {
        id: request_id,
        channel_id,
        user_id: user.0,
        username,
        status: "pending".to_string(),
        created_at: now,
    };

    Ok((StatusCode::CREATED, Json(response)))
}

/// GET /api/requests
///
/// List pending join requests for channels where the authenticated user
/// is an owner or admin.
pub async fn list_join_requests(
    State(state): State<Arc<AppState>>,
    user: AuthenticatedUser,
) -> Result<Json<ListJoinRequestsResponse>, AppError> {
    let requests = sqlx::query_as::<_, (String, String, String, String, String, i64)>(
        "SELECT jr.id, jr.channel_id, jr.user_id, u.username, jr.status, jr.created_at \
         FROM join_requests jr \
         JOIN channels c ON c.id = jr.channel_id \
         JOIN users u ON u.id = jr.user_id \
         JOIN channel_members cm ON cm.channel_id = c.id AND cm.user_id = ? \
         WHERE cm.role IN ('owner', 'admin') AND jr.status = 'pending' \
         ORDER BY jr.created_at DESC",
    )
    .bind(&user.0)
    .fetch_all(&state.pool)
    .await?
    .into_iter()
    .map(|(id, channel_id, user_id, username, status, created_at)| JoinRequestResponse {
        id,
        channel_id,
        user_id,
        username,
        status,
        created_at,
    })
    .collect();

    Ok(Json(ListJoinRequestsResponse { requests }))
}

/// PUT /api/requests/{id}/approve
///
/// Approve a pending join request. Requires the current user to be an owner
/// or admin of the channel. Adds the requesting user as a member.
pub async fn approve_join_request(
    State(state): State<Arc<AppState>>,
    user: AuthenticatedUser,
    Path(request_id): Path<String>,
) -> Result<(StatusCode, Json<serde_json::Value>), AppError> {
    // Get the request
    let row = sqlx::query_as::<_, (String, String, String, String)>(
        "SELECT jr.id, jr.channel_id, jr.user_id, jr.status \
         FROM join_requests jr \
         WHERE jr.id = ?",
    )
    .bind(&request_id)
    .fetch_optional(&state.pool)
    .await?
    .ok_or_else(|| AppError::NotFound("Join request not found".to_string()))?;

    let (_, channel_id, requester_id, status) = row;

    let username = sqlx::query_scalar::<_, String>(
        "SELECT username FROM users WHERE id = ?",
    )
    .bind(&requester_id)
    .fetch_one(&state.pool)
    .await
    .unwrap_or_else(|_| "Unknown".to_string());

    // Check user is owner or admin of the channel
    require_role(&state.pool, &user.0, &channel_id, &["owner", "admin"]).await?;

    // Check request is still pending
    if status != "pending" {
        return Err(AppError::Conflict(format!(
            "Join request is already {}",
            status
        )));
    }

    // Add the requester as a member
    sqlx::query(
        "INSERT INTO channel_members (channel_id, user_id, role) VALUES (?, ?, 'member')",
    )
    .bind(&channel_id)
    .bind(&requester_id)
    .execute(&state.pool)
    .await?;

    let approved_username = sqlx::query_scalar::<_, String>("SELECT username FROM users WHERE id = ?")
        .bind(&requester_id).fetch_one(&state.pool).await.unwrap_or_else(|_| "Unknown".to_string());

    state.ws_pool.notify_channel(
        &channel_id,
        ServerEvent::MemberAdded {
            channel_id: channel_id.clone(),
            user_id: requester_id.clone(),
            username: approved_username,
        },
    );

    // Update request status
    sqlx::query("UPDATE join_requests SET status = 'approved' WHERE id = ?")
        .bind(&request_id)
        .execute(&state.pool)
        .await?;

    let msg_payload = serde_json::json!({
        "request_id": &request_id,
        "status": "approved",
        "_join_request": true,
        "username": &username
    });
    sqlx::query(
        "UPDATE messages SET payload = ? WHERE msg_type = 'text' AND sender_id = ? AND channel_id = ? AND payload LIKE ?",
    )
    .bind(serde_json::to_string(&msg_payload).unwrap_or_default())
    .bind(&requester_id)
    .bind(&channel_id)
    .bind(format!("%{}%", request_id))
    .execute(&state.pool)
    .await?;

    state.ws_pool.notify_channel(
        &channel_id,
        ServerEvent::MsgUpdated {
            channel_id: channel_id.clone(),
        },
    );

    Ok((StatusCode::OK, Json(serde_json::json!({"status": "approved", "request_id": request_id}))))
}

/// PUT /api/requests/{id}/reject
///
/// Reject a pending join request. Requires the current user to be an owner
/// or admin of the channel.
pub async fn reject_join_request(
    State(state): State<Arc<AppState>>,
    user: AuthenticatedUser,
    Path(request_id): Path<String>,
) -> Result<(StatusCode, Json<serde_json::Value>), AppError> {
    // Get the request
    let row = sqlx::query_as::<_, (String, String, String, String)>(
        "SELECT jr.id, jr.channel_id, jr.user_id, jr.status \
         FROM join_requests jr \
         WHERE jr.id = ?",
    )
    .bind(&request_id)
    .fetch_optional(&state.pool)
    .await?
    .ok_or_else(|| AppError::NotFound("Join request not found".to_string()))?;

    let (_, channel_id, requester_id, status) = row;

    // Check user is owner or admin of the channel
    require_role(&state.pool, &user.0, &channel_id, &["owner", "admin"]).await?;

    // Check request is still pending
    if status != "pending" {
        return Err(AppError::Conflict(format!(
            "Join request is already {}",
            status
        )));
    }

    // Update request status
    sqlx::query("UPDATE join_requests SET status = 'rejected' WHERE id = ?")
        .bind(&request_id)
        .execute(&state.pool)
        .await?;

    let username = sqlx::query_scalar::<_, String>("SELECT username FROM users WHERE id = ?")
        .bind(&requester_id)
        .fetch_one(&state.pool)
        .await
        .unwrap_or_else(|_| "Unknown".to_string());

    let msg_payload = serde_json::json!({
        "request_id": &request_id,
        "status": "rejected",
        "_join_request": true,
        "username": &username
    });
    sqlx::query(
        "UPDATE messages SET payload = ? WHERE msg_type = 'text' AND sender_id = ? AND channel_id = ? AND payload LIKE ?",
    )
    .bind(serde_json::to_string(&msg_payload).unwrap_or_default())
    .bind(&requester_id)
    .bind(&channel_id)
    .bind(format!("%{}%", request_id))
    .execute(&state.pool)
    .await?;

    state.ws_pool.notify_channel(
        &channel_id,
        ServerEvent::MsgUpdated {
            channel_id: channel_id.clone(),
        },
    );

    Ok((StatusCode::OK, Json(serde_json::json!({"status": "rejected", "request_id": request_id}))))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        body::Body,
        http::{header, Method, Request},
        Router,
    };
    use serde_json::Value;
    use tower::ServiceExt;

    use crate::ws;

    /// Helper: build a test app with an in-memory database.
    /// Returns (Router, pool, owner_id, owner_token, channel_id).
    /// Creates a channel owned by the first user.
    async fn setup() -> (Router, sqlx::SqlitePool, String, String, String) {
        let pool = sqlx::SqlitePool::connect("sqlite::memory:")
            .await
            .expect("Failed to create in-memory DB");
        sqlx::migrate!("db/migrations")
            .run(&pool)
            .await
            .expect("Failed to run migrations");

        // Create owner user
        let owner_id = Uuid::new_v4().to_string();
        let pw = crate::auth::hash_password("ownerpass").expect("Failed to hash");
        sqlx::query("INSERT INTO users (id, username, password_hash) VALUES (?, ?, ?)")
            .bind(&owner_id)
            .bind("owner")
            .bind(&pw)
            .execute(&pool)
            .await
            .expect("Failed to insert owner");

        let secret = "test-secret";
        unsafe { std::env::set_var("JWT_SECRET", secret) };
        let owner_token = crate::auth::create_token_pair(&owner_id, secret, 0)
            .unwrap()
            .access_token;

        // Create a channel
        let channel_id = Uuid::new_v4().to_string();
        let now = chrono::Utc::now().timestamp();
        sqlx::query(
            "INSERT INTO channels (id, name, description, owner_id, created_at) VALUES (?, ?, ?, ?, ?)",
        )
        .bind(&channel_id)
        .bind("Test Channel")
        .bind("")
        .bind(&owner_id)
        .bind(now)
        .execute(&pool)
        .await
        .expect("Failed to insert channel");

        sqlx::query(
            "INSERT INTO channel_members (channel_id, user_id, role) VALUES (?, ?, 'owner')",
        )
        .bind(&channel_id)
        .bind(&owner_id)
        .execute(&pool)
        .await
        .expect("Failed to insert owner membership");

        let state = Arc::new(AppState {
            pool: pool.clone(),
            ws_pool: Arc::new(ws::ConnectionPool::new()),
            config: crate::AppConfig {
                jwt_secret: secret.to_string(),
                invite_code: "TEST".to_string(),
                ..crate::AppConfig::test_default()
            },
        });

        let app = crate::api::routes().with_state(state);

        (app, pool, owner_id, owner_token, channel_id)
    }

    /// Helper: make an authenticated JSON request.
    async fn request(
        app: &mut Router,
        method: Method,
        uri: &str,
        body: Option<Value>,
        token: &str,
    ) -> axum::response::Response {
        let mut builder = Request::builder()
            .method(method)
            .uri(uri)
            .header(header::CONTENT_TYPE, "application/json");

        if !token.is_empty() {
            builder = builder.header(header::AUTHORIZATION, format!("Bearer {}", token));
        }

        let req = if let Some(b) = body {
            builder
                .body(Body::from(serde_json::to_string(&b).unwrap()))
                .unwrap()
        } else {
            builder.body(Body::empty()).unwrap()
        };

        app.oneshot(req).await.unwrap()
    }

    /// Helper: create a second user and return (user_id, token).
    async fn create_second_user(
        pool: &sqlx::SqlitePool,
        username: &str,
    ) -> (String, String) {
        let user_id = Uuid::new_v4().to_string();
        let pw = crate::auth::hash_password("pass").unwrap();
        sqlx::query("INSERT INTO users (id, username, password_hash) VALUES (?, ?, ?)")
            .bind(&user_id)
            .bind(username)
            .bind(&pw)
            .execute(pool)
            .await
            .unwrap();

        let secret = "test-secret";
        let token = crate::auth::create_token_pair(&user_id, secret, 0)
            .unwrap()
            .access_token;
        (user_id, token)
    }

    // ── Create join request ──────────────────────────────────────────

    #[tokio::test]
    async fn test_create_join_request_success() {
        let (mut app, pool, _owner_id, _owner_token, channel_id) = setup().await;
        let (_user_id, user_token) = create_second_user(&pool, "requester").await;

        let resp = request(
            &mut app,
            Method::POST,
            &format!("/channels/{}/join-request", channel_id),
            None,
            &user_token,
        )
        .await;

        assert_eq!(resp.status(), StatusCode::CREATED);

        let body: Value = serde_json::from_slice(
            &axum::body::to_bytes(resp.into_body(), usize::MAX)
                .await
                .unwrap(),
        )
        .unwrap();
        assert_eq!(body["status"], "pending");
        assert_eq!(body["username"], "requester");
        assert!(!body["id"].as_str().unwrap().is_empty());
    }

    #[tokio::test]
    async fn test_create_join_request_requires_auth() {
        let (mut app, _, _, _, channel_id) = setup().await;

        let resp = request(
            &mut app,
            Method::POST,
            &format!("/channels/{}/join-request", channel_id),
            None,
            "",
        )
        .await;

        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn test_create_join_request_channel_not_found() {
        let (mut app, pool, _, _owner_token, _) = setup().await;
        let (_user_id, user_token) = create_second_user(&pool, "finder").await;

        let resp = request(
            &mut app,
            Method::POST,
            "/channels/nonexistent-id/join-request",
            None,
            &user_token,
        )
        .await;

        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_create_join_request_already_member() {
        let (mut app, _pool, owner_id, owner_token, channel_id) = setup().await;

        // Owner is already a member, try to request join
        let resp = request(
            &mut app,
            Method::POST,
            &format!("/channels/{}/join-request", channel_id),
            None,
            &owner_token,
        )
        .await;

        assert_eq!(resp.status(), StatusCode::CONFLICT);
        // Check the error message mentions "already a member"
        // (the response body should contain the error)
        let body: Value = serde_json::from_slice(
            &axum::body::to_bytes(resp.into_body(), usize::MAX)
                .await
                .unwrap(),
        )
        .unwrap();
        let msg = body["error"]["message"].as_str().unwrap_or("");
        assert!(msg.contains("already a member"), "Expected 'already a member', got: {msg}");

        drop(owner_id);
    }

    #[tokio::test]
    async fn test_create_join_request_duplicate_pending() {
        let (mut app, pool, _owner_id, _owner_token, channel_id) = setup().await;
        let (_user_id, user_token) = create_second_user(&pool, "requester2").await;

        // First request
        let resp1 = request(
            &mut app,
            Method::POST,
            &format!("/channels/{}/join-request", channel_id),
            None,
            &user_token,
        )
        .await;
        assert_eq!(resp1.status(), StatusCode::CREATED);

        // Duplicate request
        let resp2 = request(
            &mut app,
            Method::POST,
            &format!("/channels/{}/join-request", channel_id),
            None,
            &user_token,
        )
        .await;

        assert_eq!(resp2.status(), StatusCode::CONFLICT);
    }

    // ── List join requests ──────────────────────────────────────────

    #[tokio::test]
    async fn test_list_join_requests_as_owner() {
        let (mut app, pool, _owner_id, owner_token, channel_id) = setup().await;
        let (_user_id, user_token) = create_second_user(&pool, "applicant").await;

        // Create a join request
        let _ = request(
            &mut app,
            Method::POST,
            &format!("/channels/{}/join-request", channel_id),
            None,
            &user_token,
        )
        .await;

        // Owner lists requests
        let resp = request(&mut app, Method::GET, "/requests", None, &owner_token).await;
        assert_eq!(resp.status(), StatusCode::OK);

        let body: Value = serde_json::from_slice(
            &axum::body::to_bytes(resp.into_body(), usize::MAX)
                .await
                .unwrap(),
        )
        .unwrap();
        let requests = body["requests"].as_array().unwrap();
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0]["username"], "applicant");
        assert_eq!(requests[0]["status"], "pending");
    }

    #[tokio::test]
    async fn test_list_join_requests_empty() {
        let (mut app, _, _, owner_token, _) = setup().await;

        let resp = request(&mut app, Method::GET, "/requests", None, &owner_token).await;
        assert_eq!(resp.status(), StatusCode::OK);

        let body: Value = serde_json::from_slice(
            &axum::body::to_bytes(resp.into_body(), usize::MAX)
                .await
                .unwrap(),
        )
        .unwrap();
        assert_eq!(body["requests"].as_array().unwrap().len(), 0);
    }

    #[tokio::test]
    async fn test_list_join_requests_non_owner_sees_none() {
        let (mut app, pool, _owner_id, _owner_token, channel_id) = setup().await;
        let (_user_id, user_token) = create_second_user(&pool, "stranger").await;

        // Create a request from another user
        let (_, stranger_token) = create_second_user(&pool, "applicant2").await;
        let _ = request(
            &mut app,
            Method::POST,
            &format!("/channels/{}/join-request", channel_id),
            None,
            &stranger_token,
        )
        .await;

        // Stranger (not owner/admin) lists requests — should see 0
        let resp = request(&mut app, Method::GET, "/requests", None, &user_token).await;
        assert_eq!(resp.status(), StatusCode::OK);

        let body: Value = serde_json::from_slice(
            &axum::body::to_bytes(resp.into_body(), usize::MAX)
                .await
                .unwrap(),
        )
        .unwrap();
        assert_eq!(body["requests"].as_array().unwrap().len(), 0);
    }

    // ── Approve ─────────────────────────────────────────────────────

    #[tokio::test]
    async fn test_approve_join_request_as_owner() {
        let (mut app, pool, owner_id, owner_token, channel_id) = setup().await;
        let (requester_id, requester_token) = create_second_user(&pool, "requester3").await;

        // Create join request
        let create_resp = request(
            &mut app,
            Method::POST,
            &format!("/channels/{}/join-request", channel_id),
            None,
            &requester_token,
        )
        .await;
        let create_body: Value = serde_json::from_slice(
            &axum::body::to_bytes(create_resp.into_body(), usize::MAX)
                .await
                .unwrap(),
        )
        .unwrap();
        let request_id = create_body["id"].as_str().unwrap().to_string();

        // Owner approves
        let resp = request(
            &mut app,
            Method::PUT,
            &format!("/requests/{}/approve", request_id),
            None,
            &owner_token,
        )
        .await;
        assert_eq!(resp.status(), StatusCode::OK);

        // Verify requester is now a member
        let is_member = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM channel_members WHERE channel_id = ? AND user_id = ?",
        )
        .bind(&channel_id)
        .bind(&requester_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(is_member, 1);

        drop(owner_id);
    }

    #[tokio::test]
    async fn test_approve_join_request_not_owner() {
        let (mut app, pool, _owner_id, _owner_token, channel_id) = setup().await;
        let (requester_id, requester_token) = create_second_user(&pool, "requester4").await;
        let (stranger_id, stranger_token) = create_second_user(&pool, "stranger2").await;

        // Create join request
        let create_resp = request(
            &mut app,
            Method::POST,
            &format!("/channels/{}/join-request", channel_id),
            None,
            &requester_token,
        )
        .await;
        let create_body: Value = serde_json::from_slice(
            &axum::body::to_bytes(create_resp.into_body(), usize::MAX)
                .await
                .unwrap(),
        )
        .unwrap();
        let request_id = create_body["id"].as_str().unwrap().to_string();

        // Stranger (not owner/admin) tries to approve
        let resp = request(
            &mut app,
            Method::PUT,
            &format!("/requests/{}/approve", request_id),
            None,
            &stranger_token,
        )
        .await;
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);

        // Verify requester is NOT a member
        let is_member = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM channel_members WHERE channel_id = ? AND user_id = ?",
        )
        .bind(&channel_id)
        .bind(&requester_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(is_member, 0);

        drop(stranger_id);
    }

    #[tokio::test]
    async fn test_approve_join_request_not_found() {
        let (mut app, _, _, owner_token, _) = setup().await;

        let resp = request(
            &mut app,
            Method::PUT,
            "/requests/nonexistent-id/approve",
            None,
            &owner_token,
        )
        .await;

        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_approve_join_request_already_processed() {
        let (mut app, pool, _owner_id, owner_token, channel_id) = setup().await;
        let (_requester_id, requester_token) = create_second_user(&pool, "requester5").await;

        // Create and approve
        let create_resp = request(
            &mut app,
            Method::POST,
            &format!("/channels/{}/join-request", channel_id),
            None,
            &requester_token,
        )
        .await;
        let create_body: Value = serde_json::from_slice(
            &axum::body::to_bytes(create_resp.into_body(), usize::MAX)
                .await
                .unwrap(),
        )
        .unwrap();
        let request_id = create_body["id"].as_str().unwrap().to_string();

        let _ = request(
            &mut app,
            Method::PUT,
            &format!("/requests/{}/approve", request_id),
            None,
            &owner_token,
        )
        .await;

        // Try to approve again
        let resp = request(
            &mut app,
            Method::PUT,
            &format!("/requests/{}/approve", request_id),
            None,
            &owner_token,
        )
        .await;
        assert_eq!(resp.status(), StatusCode::CONFLICT);
    }

    // ── Reject ──────────────────────────────────────────────────────

    #[tokio::test]
    async fn test_reject_join_request_as_owner() {
        let (mut app, pool, _owner_id, owner_token, channel_id) = setup().await;
        let (requester_id, requester_token) = create_second_user(&pool, "requester6").await;

        // Create request
        let create_resp = request(
            &mut app,
            Method::POST,
            &format!("/channels/{}/join-request", channel_id),
            None,
            &requester_token,
        )
        .await;
        let create_body: Value = serde_json::from_slice(
            &axum::body::to_bytes(create_resp.into_body(), usize::MAX)
                .await
                .unwrap(),
        )
        .unwrap();
        let request_id = create_body["id"].as_str().unwrap().to_string();

        // Reject
        let resp = request(
            &mut app,
            Method::PUT,
            &format!("/requests/{}/reject", request_id),
            None,
            &owner_token,
        )
        .await;
        assert_eq!(resp.status(), StatusCode::OK);

        // Verify requester is NOT a member
        let is_member = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM channel_members WHERE channel_id = ? AND user_id = ?",
        )
        .bind(&channel_id)
        .bind(&requester_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(is_member, 0);
    }

    #[tokio::test]
    async fn test_reject_join_request_not_owner() {
        let (mut app, pool, _owner_id, _owner_token, channel_id) = setup().await;
        let (_requester_id, requester_token) = create_second_user(&pool, "requester7").await;
        let (_stranger_id, stranger_token) = create_second_user(&pool, "stranger3").await;

        // Create request
        let create_resp = request(
            &mut app,
            Method::POST,
            &format!("/channels/{}/join-request", channel_id),
            None,
            &requester_token,
        )
        .await;
        let create_body: Value = serde_json::from_slice(
            &axum::body::to_bytes(create_resp.into_body(), usize::MAX)
                .await
                .unwrap(),
        )
        .unwrap();
        let request_id = create_body["id"].as_str().unwrap().to_string();

        // Stranger tries to reject
        let resp = request(
            &mut app,
            Method::PUT,
            &format!("/requests/{}/reject", request_id),
            None,
            &stranger_token,
        )
        .await;
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn test_reject_join_request_not_found() {
        let (mut app, _, _, owner_token, _) = setup().await;

        let resp = request(
            &mut app,
            Method::PUT,
            "/requests/nonexistent-id/reject",
            None,
            &owner_token,
        )
        .await;

        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }
}
