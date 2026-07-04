use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use uuid::Uuid;

use crate::auth::middleware::AuthenticatedUser;
use crate::error::AppError;
use crate::ws::protocol::ServerEvent;
use crate::AppState;

#[derive(Debug, Deserialize)]
pub struct CreateInvitationRequest {
    pub user_id: String,
}

#[derive(Debug, Serialize)]
pub struct InvitationResponse {
    pub id: String,
    pub channel_id: String,
    pub channel_name: String,
    pub inviter_id: String,
    pub inviter_name: String,
    pub status: String,
    pub created_at: i64,
}

#[derive(Debug, Serialize)]
pub struct ListInvitationsResponse {
    pub invitations: Vec<InvitationResponse>,
}

/// POST /api/channels/{id}/invitations
pub async fn create_invitation(
    State(state): State<Arc<AppState>>,
    user: AuthenticatedUser,
    Path(channel_id): Path<String>,
    Json(body): Json<CreateInvitationRequest>,
) -> Result<(StatusCode, Json<InvitationResponse>), AppError> {
    // Fetch channel and verify ownership in one query
    let channel = sqlx::query_as::<_, (String, String, String)>(
        "SELECT id, name, owner_id FROM channels WHERE id = ?",
    )
    .bind(&channel_id)
    .fetch_optional(&state.pool)
    .await?
    .ok_or_else(|| AppError::NotFound("Channel not found".to_string()))?;

    let (_, channel_name, owner_id) = channel;
    if owner_id != user.0 {
        return Err(AppError::Forbidden(
            "Only the channel owner can send invitations".to_string(),
        ));
    }

    // Check invitee exists
    let invitee = sqlx::query_as::<_, (String, String)>(
        "SELECT id, username FROM users WHERE id = ?",
    )
    .bind(&body.user_id)
    .fetch_optional(&state.pool)
    .await?
    .ok_or_else(|| AppError::NotFound("User not found".to_string()))?;

    let (invitee_id, _invitee_username) = invitee;

    // Check invitee not already a member
    let is_member = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM channel_members WHERE channel_id = ? AND user_id = ?",
    )
    .bind(&channel_id)
    .bind(&invitee_id)
    .fetch_one(&state.pool)
    .await?;

    if is_member > 0 {
        return Err(AppError::Conflict(
            "User is already a member of this channel".to_string(),
        ));
    }

    // Check no duplicate pending invitation
    let pending = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM invitations WHERE channel_id = ? AND invitee_id = ? AND status = 'pending'",
    )
    .bind(&channel_id)
    .bind(&invitee_id)
    .fetch_one(&state.pool)
    .await?;

    if pending > 0 {
        return Err(AppError::Conflict(
            "A pending invitation already exists for this user".to_string(),
        ));
    }

    let invitation_id = Uuid::new_v4().to_string();
    let now = chrono::Utc::now().timestamp();

    sqlx::query(
        "INSERT INTO invitations (id, channel_id, inviter_id, invitee_id, status, created_at) VALUES (?, ?, ?, ?, 'pending', ?)",
    )
    .bind(&invitation_id)
    .bind(&channel_id)
    .bind(&user.0)
    .bind(&invitee_id)
    .bind(now)
    .execute(&state.pool)
    .await?;

    let inviter_name =
        sqlx::query_scalar::<_, String>("SELECT username FROM users WHERE id = ?")
            .bind(&user.0)
            .fetch_one(&state.pool)
            .await?;

    // Notify the channel (owner/inviter's clients see confirmation; invitee
    // discovers via polling GET /api/invitations or a future per-user WS channel)
    state.ws_pool.notify_channel(
        &channel_id,
        ServerEvent::Invitation {
            channel_id: channel_id.clone(),
            channel_name: channel_name.clone(),
            inviter_id: user.0.clone(),
            inviter_name: inviter_name.clone(),
        },
    );

    let response = InvitationResponse {
        id: invitation_id,
        channel_id,
        channel_name,
        inviter_id: user.0,
        inviter_name,
        status: "pending".to_string(),
        created_at: now,
    };

    Ok((StatusCode::CREATED, Json(response)))
}

/// GET /api/invitations
pub async fn list_invitations(
    State(state): State<Arc<AppState>>,
    user: AuthenticatedUser,
) -> Result<Json<ListInvitationsResponse>, AppError> {
    let invitations = sqlx::query_as::<_, (String, String, String, String, String, String, i64)>(
        "SELECT i.id, i.channel_id, c.name, i.inviter_id, u.username, i.status, i.created_at \
         FROM invitations i \
         JOIN channels c ON c.id = i.channel_id \
         JOIN users u ON u.id = i.inviter_id \
         WHERE i.invitee_id = ? AND i.status = 'pending' \
         ORDER BY i.created_at DESC",
    )
    .bind(&user.0)
    .fetch_all(&state.pool)
    .await?
    .into_iter()
    .map(
        |(id, channel_id, channel_name, inviter_id, inviter_name, status, created_at)| {
            InvitationResponse {
                id,
                channel_id,
                channel_name,
                inviter_id,
                inviter_name,
                status,
                created_at,
            }
        },
    )
    .collect();

    Ok(Json(ListInvitationsResponse { invitations }))
}

/// PUT /api/invitations/{id}/accept
pub async fn accept_invitation(
    State(state): State<Arc<AppState>>,
    user: AuthenticatedUser,
    Path(invitation_id): Path<String>,
) -> Result<StatusCode, AppError> {
    let row = sqlx::query_as::<_, (String, String, String, String)>(
        "SELECT i.id, i.channel_id, i.invitee_id, i.status \
         FROM invitations i \
         WHERE i.id = ?",
    )
    .bind(&invitation_id)
    .fetch_optional(&state.pool)
    .await?
    .ok_or_else(|| AppError::NotFound("Invitation not found".to_string()))?;

    let (_, channel_id, invitee_id, status) = row;

    // Only the invitee can accept
    if invitee_id != user.0 {
        return Err(AppError::Forbidden(
            "You are not the recipient of this invitation".to_string(),
        ));
    }

    if status != "pending" {
        return Err(AppError::Conflict(format!(
            "Invitation is already {}",
            status
        )));
    }

    sqlx::query(
        "INSERT INTO channel_members (channel_id, user_id, role) VALUES (?, ?, 'member')",
    )
    .bind(&channel_id)
    .bind(&invitee_id)
    .execute(&state.pool)
    .await?;

    sqlx::query("UPDATE invitations SET status = 'accepted' WHERE id = ?")
        .bind(&invitation_id)
        .execute(&state.pool)
        .await?;

    Ok(StatusCode::OK)
}

/// PUT /api/invitations/{id}/reject
pub async fn reject_invitation(
    State(state): State<Arc<AppState>>,
    user: AuthenticatedUser,
    Path(invitation_id): Path<String>,
) -> Result<StatusCode, AppError> {
    let row = sqlx::query_as::<_, (String, String, String)>(
        "SELECT i.id, i.invitee_id, i.status \
         FROM invitations i \
         WHERE i.id = ?",
    )
    .bind(&invitation_id)
    .fetch_optional(&state.pool)
    .await?
    .ok_or_else(|| AppError::NotFound("Invitation not found".to_string()))?;

    let (_, invitee_id, status) = row;

    if invitee_id != user.0 {
        return Err(AppError::Forbidden(
            "You are not the recipient of this invitation".to_string(),
        ));
    }

    if status != "pending" {
        return Err(AppError::Conflict(format!(
            "Invitation is already {}",
            status
        )));
    }

    sqlx::query("UPDATE invitations SET status = 'rejected' WHERE id = ?")
        .bind(&invitation_id)
        .execute(&state.pool)
        .await?;

    Ok(StatusCode::OK)
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        body::Body,
        http::{header, Method, Request},
        Router,
    };
    use serde_json::{json, Value};
    use tower::ServiceExt;

    use crate::ws;

    async fn setup() -> (Router, sqlx::SqlitePool, String, String, String) {
        let pool = sqlx::SqlitePool::connect("sqlite::memory:")
            .await
            .expect("Failed to create in-memory DB");
        sqlx::migrate!("db/migrations")
            .run(&pool)
            .await
            .expect("Failed to run migrations");

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

        let channel_id = Uuid::new_v4().to_string();
        let now = chrono::Utc::now().timestamp();
        sqlx::query(
            "INSERT INTO channels (id, name, description, owner_id, created_at) VALUES (?, ?, ?, ?, ?)",
        )
        .bind(&channel_id)
        .bind("Invite Channel")
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

    // ── Create invitation ───────────────────────────────────────────

    #[tokio::test]
    async fn test_create_invitation_success() {
        let (mut app, pool, _owner_id, owner_token, channel_id) = setup().await;
        let (invitee_id, _invitee_token) = create_second_user(&pool, "invitee").await;

        let resp = request(
            &mut app,
            Method::POST,
            &format!("/channels/{}/invitations", channel_id),
            Some(json!({"user_id": invitee_id})),
            &owner_token,
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
        assert_eq!(body["channel_name"], "Invite Channel");
        assert!(!body["id"].as_str().unwrap().is_empty());
    }

    #[tokio::test]
    async fn test_create_invitation_requires_auth() {
        let (mut app, pool, _owner_id, _owner_token, channel_id) = setup().await;
        let (invitee_id, _) = create_second_user(&pool, "noauth").await;

        let resp = request(
            &mut app,
            Method::POST,
            &format!("/channels/{}/invitations", channel_id),
            Some(json!({"user_id": invitee_id})),
            "",
        )
        .await;

        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn test_create_invitation_not_owner() {
        let (mut app, pool, _owner_id, _owner_token, channel_id) = setup().await;
        let (_stranger_id, stranger_token) = create_second_user(&pool, "stranger").await;
        let (invitee_id, _) = create_second_user(&pool, "target").await;

        let resp = request(
            &mut app,
            Method::POST,
            &format!("/channels/{}/invitations", channel_id),
            Some(json!({"user_id": invitee_id})),
            &stranger_token,
        )
        .await;

        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn test_create_invitation_nonexistent_user() {
        let (mut app, _, _owner_id, owner_token, channel_id) = setup().await;

        let resp = request(
            &mut app,
            Method::POST,
            &format!("/channels/{}/invitations", channel_id),
            Some(json!({"user_id": "nonexistent-id"})),
            &owner_token,
        )
        .await;

        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_create_invitation_already_member() {
        let (mut app, _pool, owner_id, owner_token, channel_id) = setup().await;

        // Owner is already a member
        let resp = request(
            &mut app,
            Method::POST,
            &format!("/channels/{}/invitations", channel_id),
            Some(json!({"user_id": owner_id})),
            &owner_token,
        )
        .await;

        assert_eq!(resp.status(), StatusCode::CONFLICT);
    }

    #[tokio::test]
    async fn test_create_invitation_duplicate_pending() {
        let (mut app, pool, _owner_id, owner_token, channel_id) = setup().await;
        let (invitee_id, _invitee_token) = create_second_user(&pool, "dup_target").await;

        let resp1 = request(
            &mut app,
            Method::POST,
            &format!("/channels/{}/invitations", channel_id),
            Some(json!({"user_id": invitee_id})),
            &owner_token,
        )
        .await;
        assert_eq!(resp1.status(), StatusCode::CREATED);

        let resp2 = request(
            &mut app,
            Method::POST,
            &format!("/channels/{}/invitations", channel_id),
            Some(json!({"user_id": invitee_id})),
            &owner_token,
        )
        .await;
        assert_eq!(resp2.status(), StatusCode::CONFLICT);
    }

    #[tokio::test]
    async fn test_create_invitation_channel_not_found() {
        let (mut app, pool, _owner_id, owner_token, _) = setup().await;
        let (invitee_id, _) = create_second_user(&pool, "ghost").await;

        let resp = request(
            &mut app,
            Method::POST,
            "/channels/nonexistent/invitations",
            Some(json!({"user_id": invitee_id})),
            &owner_token,
        )
        .await;

        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    // ── List invitations ────────────────────────────────────────────

    #[tokio::test]
    async fn test_list_invitations() {
        let (mut app, pool, _owner_id, owner_token, channel_id) = setup().await;
        let (invitee_id, invitee_token) = create_second_user(&pool, "invitee2").await;

        let _ = request(
            &mut app,
            Method::POST,
            &format!("/channels/{}/invitations", channel_id),
            Some(json!({"user_id": invitee_id})),
            &owner_token,
        )
        .await;

        // Invitee lists pending invitations
        let resp = request(&mut app, Method::GET, "/invitations", None, &invitee_token).await;
        assert_eq!(resp.status(), StatusCode::OK);

        let body: Value = serde_json::from_slice(
            &axum::body::to_bytes(resp.into_body(), usize::MAX)
                .await
                .unwrap(),
        )
        .unwrap();
        let invs = body["invitations"].as_array().unwrap();
        assert_eq!(invs.len(), 1);
        assert_eq!(invs[0]["status"], "pending");
        assert_eq!(invs[0]["channel_name"], "Invite Channel");
    }

    #[tokio::test]
    async fn test_list_invitations_empty() {
        let (mut app, pool, _owner_id, _owner_token, _) = setup().await;
        let (_user_id, user_token) = create_second_user(&pool, "lonely").await;

        let resp = request(&mut app, Method::GET, "/invitations", None, &user_token).await;
        assert_eq!(resp.status(), StatusCode::OK);

        let body: Value = serde_json::from_slice(
            &axum::body::to_bytes(resp.into_body(), usize::MAX)
                .await
                .unwrap(),
        )
        .unwrap();
        assert_eq!(body["invitations"].as_array().unwrap().len(), 0);
    }

    // ── Accept ──────────────────────────────────────────────────────

    #[tokio::test]
    async fn test_accept_invitation_as_target() {
        let (mut app, pool, _owner_id, owner_token, channel_id) = setup().await;
        let (invitee_id, invitee_token) = create_second_user(&pool, "accept_test").await;

        let create_resp = request(
            &mut app,
            Method::POST,
            &format!("/channels/{}/invitations", channel_id),
            Some(json!({"user_id": invitee_id})),
            &owner_token,
        )
        .await;
        let create_body: Value = serde_json::from_slice(
            &axum::body::to_bytes(create_resp.into_body(), usize::MAX)
                .await
                .unwrap(),
        )
        .unwrap();
        let inv_id = create_body["id"].as_str().unwrap().to_string();

        let resp = request(
            &mut app,
            Method::PUT,
            &format!("/invitations/{}/accept", inv_id),
            None,
            &invitee_token,
        )
        .await;
        assert_eq!(resp.status(), StatusCode::OK);

        let is_member = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM channel_members WHERE channel_id = ? AND user_id = ?",
        )
        .bind(&channel_id)
        .bind(&invitee_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(is_member, 1);
    }

    #[tokio::test]
    async fn test_accept_invitation_not_target() {
        let (mut app, pool, _owner_id, owner_token, channel_id) = setup().await;
        let (invitee_id, _invitee_token) = create_second_user(&pool, "real_target").await;
        let (_stranger_id, stranger_token) = create_second_user(&pool, "impostor").await;

        let create_resp = request(
            &mut app,
            Method::POST,
            &format!("/channels/{}/invitations", channel_id),
            Some(json!({"user_id": invitee_id})),
            &owner_token,
        )
        .await;
        let create_body: Value = serde_json::from_slice(
            &axum::body::to_bytes(create_resp.into_body(), usize::MAX)
                .await
                .unwrap(),
        )
        .unwrap();
        let inv_id = create_body["id"].as_str().unwrap().to_string();

        let resp = request(
            &mut app,
            Method::PUT,
            &format!("/invitations/{}/accept", inv_id),
            None,
            &stranger_token,
        )
        .await;
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn test_accept_invitation_not_found() {
        let (mut app, pool, _owner_id, _owner_token, _) = setup().await;
        let (_user_id, user_token) = create_second_user(&pool, "nobody").await;

        let resp = request(
            &mut app,
            Method::PUT,
            "/invitations/nonexistent-id/accept",
            None,
            &user_token,
        )
        .await;

        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_accept_invitation_already_processed() {
        let (mut app, pool, _owner_id, owner_token, channel_id) = setup().await;
        let (invitee_id, invitee_token) = create_second_user(&pool, "late_accept").await;

        let create_resp = request(
            &mut app,
            Method::POST,
            &format!("/channels/{}/invitations", channel_id),
            Some(json!({"user_id": invitee_id})),
            &owner_token,
        )
        .await;
        let create_body: Value = serde_json::from_slice(
            &axum::body::to_bytes(create_resp.into_body(), usize::MAX)
                .await
                .unwrap(),
        )
        .unwrap();
        let inv_id = create_body["id"].as_str().unwrap().to_string();

        let _ = request(
            &mut app,
            Method::PUT,
            &format!("/invitations/{}/accept", inv_id),
            None,
            &invitee_token,
        )
        .await;

        let resp = request(
            &mut app,
            Method::PUT,
            &format!("/invitations/{}/accept", inv_id),
            None,
            &invitee_token,
        )
        .await;
        assert_eq!(resp.status(), StatusCode::CONFLICT);
    }

    // ── Reject ──────────────────────────────────────────────────────

    #[tokio::test]
    async fn test_reject_invitation_as_target() {
        let (mut app, pool, _owner_id, owner_token, channel_id) = setup().await;
        let (invitee_id, invitee_token) = create_second_user(&pool, "reject_test").await;

        let create_resp = request(
            &mut app,
            Method::POST,
            &format!("/channels/{}/invitations", channel_id),
            Some(json!({"user_id": invitee_id})),
            &owner_token,
        )
        .await;
        let create_body: Value = serde_json::from_slice(
            &axum::body::to_bytes(create_resp.into_body(), usize::MAX)
                .await
                .unwrap(),
        )
        .unwrap();
        let inv_id = create_body["id"].as_str().unwrap().to_string();

        let resp = request(
            &mut app,
            Method::PUT,
            &format!("/invitations/{}/reject", inv_id),
            None,
            &invitee_token,
        )
        .await;
        assert_eq!(resp.status(), StatusCode::OK);

        // Verify NOT added as member
        let is_member = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM channel_members WHERE channel_id = ? AND user_id = ?",
        )
        .bind(&channel_id)
        .bind(&invitee_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(is_member, 0);
    }

    #[tokio::test]
    async fn test_reject_invitation_not_target() {
        let (mut app, pool, _owner_id, owner_token, channel_id) = setup().await;
        let (invitee_id, _invitee_token) = create_second_user(&pool, "real_target2").await;
        let (_stranger_id, stranger_token) = create_second_user(&pool, "impostor2").await;

        let create_resp = request(
            &mut app,
            Method::POST,
            &format!("/channels/{}/invitations", channel_id),
            Some(json!({"user_id": invitee_id})),
            &owner_token,
        )
        .await;
        let create_body: Value = serde_json::from_slice(
            &axum::body::to_bytes(create_resp.into_body(), usize::MAX)
                .await
                .unwrap(),
        )
        .unwrap();
        let inv_id = create_body["id"].as_str().unwrap().to_string();

        let resp = request(
            &mut app,
            Method::PUT,
            &format!("/invitations/{}/reject", inv_id),
            None,
            &stranger_token,
        )
        .await;
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn test_reject_invitation_not_found() {
        let (mut app, pool, _owner_id, _owner_token, _) = setup().await;
        let (_user_id, user_token) = create_second_user(&pool, "nobody2").await;

        let resp = request(
            &mut app,
            Method::PUT,
            "/invitations/nonexistent-id/reject",
            None,
            &user_token,
        )
        .await;

        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }
}
