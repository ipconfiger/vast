use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use std::sync::Arc;
use uuid::Uuid;

use crate::auth::middleware::AuthenticatedUser;
use crate::error::AppError;
use crate::AppState;

// ---------------------------------------------------------------------------
// Request / Response types
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize, FromRow)]
pub struct Channel {
    pub id: String,
    pub name: String,
    pub description: String,
    pub owner_id: String,
    pub is_direct: bool,
    pub is_group_dm: bool,
    pub is_archived: bool,
    pub created_at: i64,
}

#[derive(Debug, Serialize, FromRow)]
pub struct ChannelWithRole {
    pub id: String,
    pub name: String,
    pub description: String,
    pub owner_id: String,
    pub is_direct: bool,
    pub is_group_dm: bool,
    pub is_archived: bool,
    pub created_at: i64,
    pub role: String,
}

#[derive(Debug, Deserialize)]
pub struct CreateChannelRequest {
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateChannelRequest {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ChannelListResponse {
    pub channels: Vec<ChannelWithRole>,
}

#[derive(Debug, Serialize, FromRow)]
pub struct DiscoverChannel {
    pub id: String,
    pub name: String,
    pub description: String,
    pub owner_name: String,
    pub member_count: i64,
    pub is_member: bool,
}

#[derive(Debug, Serialize)]
pub struct DiscoverChannelResponse {
    pub channels: Vec<DiscoverChannel>,
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

/// POST /api/channels
///
/// Create a new channel. Requires authentication.
/// Validates name length (1-80 chars). Inserts channel and creates the
/// creator as owner in channel_members.
pub async fn create_channel(
    State(state): State<Arc<AppState>>,
    user: AuthenticatedUser,
    Json(req): Json<CreateChannelRequest>,
) -> Result<(StatusCode, Json<Channel>), AppError> {
    let name = req.name.trim().to_string();
    if name.is_empty() || name.len() > 80 {
        return Err(AppError::BadRequest(
            "Channel name must be between 1 and 80 characters".to_string(),
        ));
    }

    let description = req.description.unwrap_or_default();
    let channel_id = Uuid::new_v4().to_string();
    let now = chrono::Utc::now().timestamp();

    sqlx::query(
        "INSERT INTO channels (id, name, description, owner_id, created_at) VALUES (?, ?, ?, ?, ?)",
    )
    .bind(&channel_id)
    .bind(&name)
    .bind(&description)
    .bind(&user.0)
    .bind(now)
    .execute(&state.pool)
    .await?;

    sqlx::query("INSERT INTO channel_members (channel_id, user_id, role) VALUES (?, ?, 'owner')")
        .bind(&channel_id)
        .bind(&user.0)
        .execute(&state.pool)
        .await?;

    let channel = sqlx::query_as::<_, Channel>(
        "SELECT id, name, description, owner_id, is_direct, is_group_dm, is_archived, created_at FROM channels WHERE id = ?",
    )
    .bind(&channel_id)
    .fetch_one(&state.pool)
    .await?;

    Ok((StatusCode::CREATED, Json(channel)))
}

/// GET /api/channels
///
/// List channels where the authenticated user is a member.
/// Returns channels with the member's role.
pub async fn list_channels(
    State(state): State<Arc<AppState>>,
    user: AuthenticatedUser,
) -> Result<Json<ChannelListResponse>, AppError> {
    let channels = sqlx::query_as::<_, ChannelWithRole>(
        "SELECT c.id, c.name, c.description, c.owner_id, c.is_direct, c.is_group_dm, c.is_archived, c.created_at, cm.role \
         FROM channels c \
         JOIN channel_members cm ON c.id = cm.channel_id \
         WHERE cm.user_id = ? AND c.is_direct = 0 \
         ORDER BY c.created_at DESC",
    )
    .bind(&user.0)
    .fetch_all(&state.pool)
    .await?;

    Ok(Json(ChannelListResponse { channels }))
}

/// GET /api/channels/discover
///
/// List channels available for discovery — all non-DM, non-archived channels.
/// Shows whether the current user is already a member.
pub async fn discover_channels(
    State(state): State<Arc<AppState>>,
    user: AuthenticatedUser,
) -> Result<Json<DiscoverChannelResponse>, AppError> {
    let channels = sqlx::query_as::<_, DiscoverChannel>(
        "SELECT c.id, c.name, c.description, \
                u.username as owner_name, \
                (SELECT COUNT(*) FROM channel_members WHERE channel_id = c.id) as member_count, \
                EXISTS(SELECT 1 FROM channel_members WHERE channel_id = c.id AND user_id = ?) as is_member \
         FROM channels c \
         JOIN users u ON c.owner_id = u.id \
         WHERE c.is_direct = 0 AND c.is_archived = 0 \
         ORDER BY member_count DESC, c.created_at DESC",
    )
    .bind(&user.0)
    .fetch_all(&state.pool)
    .await?;

    Ok(Json(DiscoverChannelResponse { channels }))
}

/// GET /api/channels/{id}
///
/// Get a single channel by ID. Requires the user to be a member.
/// Returns 404 if the channel doesn't exist, 403 if not a member.
pub async fn get_channel(
    State(state): State<Arc<AppState>>,
    user: AuthenticatedUser,
    Path(id): Path<String>,
) -> Result<Json<ChannelWithRole>, AppError> {
    let channel = sqlx::query_as::<_, ChannelWithRole>(
        "SELECT c.id, c.name, c.description, COALESCE(c.owner_id, '') as owner_id, c.is_direct, c.is_group_dm, c.is_archived, c.created_at, cm.role \
         FROM channels c \
         JOIN channel_members cm ON c.id = cm.channel_id \
         WHERE c.id = ? AND cm.user_id = ?",
    )
    .bind(&id)
    .bind(&user.0)
    .fetch_optional(&state.pool)
    .await?;

    match channel {
        Some(c) => Ok(Json(c)),
        None => {
            // Check if channel exists at all to differentiate 404 vs 403
            let exists = sqlx::query_scalar::<_, i64>(
                "SELECT COUNT(*) FROM channels WHERE id = ?",
            )
            .bind(&id)
            .fetch_one(&state.pool)
            .await?;

            if exists == 0 {
                Err(AppError::NotFound("Channel not found".to_string()))
            } else {
                Err(AppError::Forbidden(
                    "You are not a member of this channel".to_string(),
                ))
            }
        }
    }
}

/// PATCH /api/channels/{id}
///
/// Update a channel's name and/or description. Only the channel owner can update.
/// Returns 403 for non-owners, 404 if channel not found.
pub async fn update_channel(
    State(state): State<Arc<AppState>>,
    user: AuthenticatedUser,
    Path(id): Path<String>,
    Json(req): Json<UpdateChannelRequest>,
) -> Result<Json<ChannelWithRole>, AppError> {
    // Verify channel exists and fetch current data
    let channel = sqlx::query_as::<_, Channel>(
        "SELECT id, name, description, COALESCE(owner_id, '') as owner_id, is_direct, is_group_dm, is_archived, created_at FROM channels WHERE id = ?",
    )
    .bind(&id)
    .fetch_optional(&state.pool)
    .await?
    .ok_or_else(|| AppError::NotFound("Channel not found".to_string()))?;

    if channel.is_direct {
        return Err(AppError::Forbidden(
            "DM channels cannot be updated".to_string(),
        ));
    }

    // Only owner can update
    if channel.owner_id != user.0 {
        return Err(AppError::Forbidden(
            "Only the channel owner can update this channel".to_string(),
        ));
    }

    // Validate and apply updates
    if let Some(ref name) = req.name {
        let trimmed = name.trim();
        if trimmed.is_empty() || trimmed.len() > 80 {
            return Err(AppError::BadRequest(
                "Channel name must be between 1 and 80 characters".to_string(),
            ));
        }
    }

    let new_name = req
        .name
        .map(|n| n.trim().to_string())
        .filter(|n| !n.is_empty())
        .unwrap_or(channel.name);
    let new_description = req.description.unwrap_or(channel.description);

    sqlx::query("UPDATE channels SET name = ?, description = ? WHERE id = ?")
        .bind(&new_name)
        .bind(&new_description)
        .bind(&id)
        .execute(&state.pool)
        .await?;

    // Return the updated channel with the user's role
    let updated = sqlx::query_as::<_, ChannelWithRole>(
        "SELECT c.id, c.name, c.description, COALESCE(c.owner_id, '') as owner_id, c.is_direct, c.is_group_dm, c.is_archived, c.created_at, cm.role \
         FROM channels c \
         JOIN channel_members cm ON c.id = cm.channel_id \
         WHERE c.id = ? AND cm.user_id = ?",
    )
    .bind(&id)
    .bind(&user.0)
    .fetch_one(&state.pool)
    .await?;

    Ok(Json(updated))
}

/// POST /api/channels/{id}/archive
///
/// Archive a channel. Only the channel owner can archive.
/// Sets is_archived=true and broadcasts ChannelArchived via WebSocket.
pub async fn archive_channel(
    State(state): State<Arc<AppState>>,
    user: AuthenticatedUser,
    Path(id): Path<String>,
) -> Result<StatusCode, AppError> {
    let channel = sqlx::query_as::<_, Channel>(
        "SELECT id, name, description, COALESCE(owner_id, '') as owner_id, is_direct, is_group_dm, is_archived, created_at FROM channels WHERE id = ?",
    )
    .bind(&id)
    .fetch_optional(&state.pool)
    .await?
    .ok_or_else(|| AppError::NotFound("Channel not found".to_string()))?;

    if channel.is_direct {
        return Err(AppError::Forbidden(
            "DM channels cannot be archived".to_string(),
        ));
    }

    if channel.owner_id != user.0 {
        return Err(AppError::Forbidden(
            "Only the channel owner can archive this channel".to_string(),
        ));
    }

    sqlx::query("UPDATE channels SET is_archived = 1 WHERE id = ?")
        .bind(&id)
        .execute(&state.pool)
        .await?;

    let channel_id = id.clone();
    state
        .ws_pool
        .broadcast_to_channel(&id, &crate::ws::protocol::ServerEvent::ChannelArchived { channel_id });

    Ok(StatusCode::OK)
}

/// POST /api/channels/{id}/unarchive
///
/// Unarchive a channel. Only the channel owner can unarchive.
/// Sets is_archived=false and broadcasts ChannelUnarchived via WebSocket.
pub async fn unarchive_channel(
    State(state): State<Arc<AppState>>,
    user: AuthenticatedUser,
    Path(id): Path<String>,
) -> Result<StatusCode, AppError> {
    let channel = sqlx::query_as::<_, Channel>(
        "SELECT id, name, description, COALESCE(owner_id, '') as owner_id, is_direct, is_group_dm, is_archived, created_at FROM channels WHERE id = ?",
    )
    .bind(&id)
    .fetch_optional(&state.pool)
    .await?
    .ok_or_else(|| AppError::NotFound("Channel not found".to_string()))?;

    if channel.is_direct {
        return Err(AppError::Forbidden(
            "DM channels cannot be unarchived".to_string(),
        ));
    }

    if channel.owner_id != user.0 {
        return Err(AppError::Forbidden(
            "Only the channel owner can unarchive this channel".to_string(),
        ));
    }

    sqlx::query("UPDATE channels SET is_archived = 0 WHERE id = ?")
        .bind(&id)
        .execute(&state.pool)
        .await?;

    let channel_id = id.clone();
    state
        .ws_pool
        .broadcast_to_channel(&id, &crate::ws::protocol::ServerEvent::ChannelUnarchived { channel_id });

    Ok(StatusCode::OK)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        body::Body,
        http::{header, Method, Request, StatusCode},
        Router,
    };
    use serde_json::{json, Value};
    use tower::ServiceExt;

    use crate::ws;

    /// Helper to build a test app with an in-memory database.
    /// Returns (Router, pool, user_id, auth_token).
    async fn setup() -> (Router, sqlx::SqlitePool, String, String) {
        let pool = sqlx::SqlitePool::connect("sqlite::memory:")
            .await
            .expect("Failed to create in-memory DB");
        sqlx::migrate!("db/migrations")
            .run(&pool)
            .await
            .expect("Failed to run migrations");

        // Create a test user
        let user_id = Uuid::new_v4().to_string();
        let password_hash =
            crate::auth::hash_password("testpass").expect("Failed to hash password");
        sqlx::query("INSERT INTO users (id, username, password_hash) VALUES (?, ?, ?)")
            .bind(&user_id)
            .bind("testuser")
            .bind(&password_hash)
            .execute(&pool)
            .await
            .expect("Failed to insert test user");

        let secret = "test-secret";
        // SAFETY: test-only; single-threaded, no concurrent readers
        unsafe { std::env::set_var("JWT_SECRET", secret) };
        let token = crate::auth::create_token_pair(&user_id, secret, 0)
            .expect("Failed to create token")
            .access_token;

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

        (app, pool, user_id, token)
    }

    /// Helper to make an authenticated JSON request and return the response.
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

    #[tokio::test]
    async fn test_create_channel_success() {
        let (mut app, _, _user_id, token) = setup().await;

        let resp = request(
            &mut app,
            Method::POST,
            "/channels",
            Some(json!({"name": "Test Channel", "description": "A test channel"})),
            &token,
        )
        .await;

        assert_eq!(resp.status(), StatusCode::CREATED);

        let body: Value = serde_json::from_slice(
            &axum::body::to_bytes(resp.into_body(), usize::MAX)
                .await
                .unwrap(),
        )
        .unwrap();
        assert_eq!(body["name"], "Test Channel");
        assert_eq!(body["description"], "A test channel");
        assert!(!body["id"].as_str().unwrap().is_empty());
        assert_eq!(body["is_direct"], false);
        assert_eq!(body["is_archived"], false);
    }

    #[tokio::test]
    async fn test_create_channel_minimal() {
        let (mut app, _, _, token) = setup().await;

        let resp = request(
            &mut app,
            Method::POST,
            "/channels",
            Some(json!({"name": "Minimal"})),
            &token,
        )
        .await;

        assert_eq!(resp.status(), StatusCode::CREATED);

        let body: Value = serde_json::from_slice(
            &axum::body::to_bytes(resp.into_body(), usize::MAX)
                .await
                .unwrap(),
        )
        .unwrap();
        assert_eq!(body["name"], "Minimal");
        assert_eq!(body["description"], "");
    }

    #[tokio::test]
    async fn test_create_channel_empty_name() {
        let (mut app, _, _, token) = setup().await;

        let resp = request(
            &mut app,
            Method::POST,
            "/channels",
            Some(json!({"name": ""})),
            &token,
        )
        .await;

        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_create_channel_name_too_long() {
        let (mut app, _, _, token) = setup().await;

        let resp = request(
            &mut app,
            Method::POST,
            "/channels",
            Some(json!({"name": "a".repeat(81)})),
            &token,
        )
        .await;

        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_create_channel_requires_auth() {
        let (mut app, _, _, _) = setup().await;

        let resp = request(
            &mut app,
            Method::POST,
            "/channels",
            Some(json!({"name": "No Auth"})),
            "", // no token
        )
        .await;

        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn test_list_channels() {
        let (mut app, _, user_id, token) = setup().await;

        // Create two channels via API
        let _ = request(
            &mut app,
            Method::POST,
            "/channels",
            Some(json!({"name": "Channel Alpha"})),
            &token,
        )
        .await;

        let _ = request(
            &mut app,
            Method::POST,
            "/channels",
            Some(json!({"name": "Channel Beta"})),
            &token,
        )
        .await;

        // List channels
        let resp = request(&mut app, Method::GET, "/channels", None, &token).await;
        assert_eq!(resp.status(), StatusCode::OK);

        let body: Value = serde_json::from_slice(
            &axum::body::to_bytes(resp.into_body(), usize::MAX)
                .await
                .unwrap(),
        )
        .unwrap();
        let channels = body["channels"].as_array().unwrap();
        assert_eq!(channels.len(), 2);

        // Each channel should have the user's role
        for ch in channels {
            assert_eq!(ch["role"], "owner");
            assert_eq!(ch["owner_id"], user_id);
        }
    }

    #[tokio::test]
    async fn test_list_channels_excludes_non_member() {
        let (mut app, pool, _, token1) = setup().await;

        // Create a channel with user1
        let _ = request(
            &mut app,
            Method::POST,
            "/channels",
            Some(json!({"name": "User1 Channel"})),
            &token1,
        )
        .await;

        // Create a second user (NOT via API — insert directly)
        let user2_id = Uuid::new_v4().to_string();
        let password_hash =
            crate::auth::hash_password("pass2").expect("Failed to hash password");
        sqlx::query("INSERT INTO users (id, username, password_hash) VALUES (?, ?, ?)")
            .bind(&user2_id)
            .bind("user2")
            .bind(&password_hash)
            .execute(&pool)
            .await
            .expect("Failed to insert user2");

        let secret = "test-secret";
        let token2 = crate::auth::create_token_pair(&user2_id, secret, 0)
            .unwrap()
            .access_token;

        // User2 lists channels — should see 0 (not a member of any)
        let resp = request(&mut app, Method::GET, "/channels", None, &token2).await;
        assert_eq!(resp.status(), StatusCode::OK);

        let body: Value = serde_json::from_slice(
            &axum::body::to_bytes(resp.into_body(), usize::MAX)
                .await
                .unwrap(),
        )
        .unwrap();
        assert_eq!(body["channels"].as_array().unwrap().len(), 0);
    }

    #[tokio::test]
    async fn test_get_channel_success() {
        let (mut app, _, _, token) = setup().await;

        // Create a channel
        let create_resp = request(
            &mut app,
            Method::POST,
            "/channels",
            Some(json!({"name": "Get Test", "description": "testing"})),
            &token,
        )
        .await;

        let create_body: Value = serde_json::from_slice(
            &axum::body::to_bytes(create_resp.into_body(), usize::MAX)
                .await
                .unwrap(),
        )
        .unwrap();
        let channel_id = create_body["id"].as_str().unwrap().to_string();

        // Get the channel
        let resp = request(
            &mut app,
            Method::GET,
            &format!("/channels/{}", channel_id),
            None,
            &token,
        )
        .await;

        assert_eq!(resp.status(), StatusCode::OK);

        let body: Value = serde_json::from_slice(
            &axum::body::to_bytes(resp.into_body(), usize::MAX)
                .await
                .unwrap(),
        )
        .unwrap();
        assert_eq!(body["id"], channel_id);
        assert_eq!(body["name"], "Get Test");
        assert_eq!(body["description"], "testing");
        assert_eq!(body["role"], "owner");
    }

    #[tokio::test]
    async fn test_get_channel_not_found() {
        let (mut app, _, _, token) = setup().await;

        let resp = request(
            &mut app,
            Method::GET,
            "/channels/nonexistent-id",
            None,
            &token,
        )
        .await;

        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_get_channel_not_member() {
        let (mut app, pool, _, token1) = setup().await;

        // Create a channel with user1
        let create_resp = request(
            &mut app,
            Method::POST,
            "/channels",
            Some(json!({"name": "Private Channel"})),
            &token1,
        )
        .await;

        let create_body: Value = serde_json::from_slice(
            &axum::body::to_bytes(create_resp.into_body(), usize::MAX)
                .await
                .unwrap(),
        )
        .unwrap();
        let channel_id = create_body["id"].as_str().unwrap().to_string();

        // Create user2
        let user2_id = Uuid::new_v4().to_string();
        let pw = crate::auth::hash_password("pass2").unwrap();
        sqlx::query("INSERT INTO users (id, username, password_hash) VALUES (?, ?, ?)")
            .bind(&user2_id)
            .bind("user2")
            .bind(&pw)
            .execute(&pool)
            .await
            .unwrap();

        let secret = "test-secret";
        let token2 = crate::auth::create_token_pair(&user2_id, secret, 0)
            .unwrap()
            .access_token;

        // User2 tries to get the channel
        let resp = request(
            &mut app,
            Method::GET,
            &format!("/channels/{}", channel_id),
            None,
            &token2,
        )
        .await;

        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn test_update_channel_success() {
        let (mut app, _, _, token) = setup().await;

        // Create channel
        let create_resp = request(
            &mut app,
            Method::POST,
            "/channels",
            Some(json!({"name": "Original Name", "description": "Original desc"})),
            &token,
        )
        .await;

        let create_body: Value = serde_json::from_slice(
            &axum::body::to_bytes(create_resp.into_body(), usize::MAX)
                .await
                .unwrap(),
        )
        .unwrap();
        let channel_id = create_body["id"].as_str().unwrap().to_string();

        // Update name and description
        let resp = request(
            &mut app,
            Method::PATCH,
            &format!("/channels/{}", channel_id),
            Some(json!({"name": "Updated Name", "description": "Updated desc"})),
            &token,
        )
        .await;

        assert_eq!(resp.status(), StatusCode::OK);

        let body: Value = serde_json::from_slice(
            &axum::body::to_bytes(resp.into_body(), usize::MAX)
                .await
                .unwrap(),
        )
        .unwrap();
        assert_eq!(body["name"], "Updated Name");
        assert_eq!(body["description"], "Updated desc");
        assert_eq!(body["role"], "owner");
    }

    #[tokio::test]
    async fn test_update_channel_partial() {
        let (mut app, _, _, token) = setup().await;

        // Create channel
        let create_resp = request(
            &mut app,
            Method::POST,
            "/channels",
            Some(json!({"name": "Original", "description": "Original desc"})),
            &token,
        )
        .await;

        let create_body: Value = serde_json::from_slice(
            &axum::body::to_bytes(create_resp.into_body(), usize::MAX)
                .await
                .unwrap(),
        )
        .unwrap();
        let channel_id = create_body["id"].as_str().unwrap().to_string();

        // Update only name
        let resp = request(
            &mut app,
            Method::PATCH,
            &format!("/channels/{}", channel_id),
            Some(json!({"name": "Only Name Changed"})),
            &token,
        )
        .await;

        assert_eq!(resp.status(), StatusCode::OK);

        let body: Value = serde_json::from_slice(
            &axum::body::to_bytes(resp.into_body(), usize::MAX)
                .await
                .unwrap(),
        )
        .unwrap();
        assert_eq!(body["name"], "Only Name Changed");
        assert_eq!(body["description"], "Original desc");
    }

    #[tokio::test]
    async fn test_update_channel_not_owner() {
        let (mut app, pool, _, token1) = setup().await;

        // Create channel with user1
        let create_resp = request(
            &mut app,
            Method::POST,
            "/channels",
            Some(json!({"name": "Owned Channel"})),
            &token1,
        )
        .await;

        let create_body: Value = serde_json::from_slice(
            &axum::body::to_bytes(create_resp.into_body(), usize::MAX)
                .await
                .unwrap(),
        )
        .unwrap();
        let channel_id = create_body["id"].as_str().unwrap().to_string();

        // Create user2
        let user2_id = Uuid::new_v4().to_string();
        let pw = crate::auth::hash_password("pass2").unwrap();
        sqlx::query("INSERT INTO users (id, username, password_hash) VALUES (?, ?, ?)")
            .bind(&user2_id)
            .bind("user2")
            .bind(&pw)
            .execute(&pool)
            .await
            .unwrap();

        let secret = "test-secret";
        let token2 = crate::auth::create_token_pair(&user2_id, secret, 0)
            .unwrap()
            .access_token;

        // User2 tries to update
        let resp = request(
            &mut app,
            Method::PATCH,
            &format!("/channels/{}", channel_id),
            Some(json!({"name": "Hacked"})),
            &token2,
        )
        .await;

        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn test_update_channel_not_found() {
        let (mut app, _, _, token) = setup().await;

        let resp = request(
            &mut app,
            Method::PATCH,
            "/channels/nonexistent-id",
            Some(json!({"name": "Nope"})),
            &token,
        )
        .await;

        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_update_channel_requires_auth() {
        let (mut app, _, _, _) = setup().await;

        let resp = request(
            &mut app,
            Method::PATCH,
            "/channels/some-id",
            Some(json!({"name": "No Auth"})),
            "", // no token
        )
        .await;

        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    // ── Archive / Unarchive ───────────────────────────────────────────

    #[tokio::test]
    async fn test_archive_channel_success() {
        let (mut app, _, _, token) = setup().await;

        let create_resp = request(
            &mut app,
            Method::POST,
            "/channels",
            Some(json!({"name": "Archive Test"})),
            &token,
        )
        .await;
        let create_body: Value = serde_json::from_slice(
            &axum::body::to_bytes(create_resp.into_body(), usize::MAX)
                .await
                .unwrap(),
        )
        .unwrap();
        let channel_id = create_body["id"].as_str().unwrap().to_string();

        // Archive
        let resp = request(
            &mut app,
            Method::POST,
            &format!("/channels/{}/archive", channel_id),
            None,
            &token,
        )
        .await;
        assert_eq!(resp.status(), StatusCode::OK);

        // Verify is_archived = true
        let get_resp = request(
            &mut app,
            Method::GET,
            &format!("/channels/{}", channel_id),
            None,
            &token,
        )
        .await;
        let body: Value = serde_json::from_slice(
            &axum::body::to_bytes(get_resp.into_body(), usize::MAX)
                .await
                .unwrap(),
        )
        .unwrap();
        assert_eq!(body["is_archived"], true);
    }

    #[tokio::test]
    async fn test_unarchive_channel_success() {
        let (mut app, _, _, token) = setup().await;

        let create_resp = request(
            &mut app,
            Method::POST,
            "/channels",
            Some(json!({"name": "Unarchive Test"})),
            &token,
        )
        .await;
        let create_body: Value = serde_json::from_slice(
            &axum::body::to_bytes(create_resp.into_body(), usize::MAX)
                .await
                .unwrap(),
        )
        .unwrap();
        let channel_id = create_body["id"].as_str().unwrap().to_string();

        // Archive first
        let _ = request(
            &mut app,
            Method::POST,
            &format!("/channels/{}/archive", channel_id),
            None,
            &token,
        )
        .await;

        // Unarchive
        let resp = request(
            &mut app,
            Method::POST,
            &format!("/channels/{}/unarchive", channel_id),
            None,
            &token,
        )
        .await;
        assert_eq!(resp.status(), StatusCode::OK);

        // Verify is_archived = false
        let get_resp = request(
            &mut app,
            Method::GET,
            &format!("/channels/{}", channel_id),
            None,
            &token,
        )
        .await;
        let body: Value = serde_json::from_slice(
            &axum::body::to_bytes(get_resp.into_body(), usize::MAX)
                .await
                .unwrap(),
        )
        .unwrap();
        assert_eq!(body["is_archived"], false);
    }

    #[tokio::test]
    async fn test_archive_not_owner() {
        let (mut app, pool, _, token1) = setup().await;

        // Create channel with user1
        let create_resp = request(
            &mut app,
            Method::POST,
            "/channels",
            Some(json!({"name": "Private"})),
            &token1,
        )
        .await;
        let create_body: Value = serde_json::from_slice(
            &axum::body::to_bytes(create_resp.into_body(), usize::MAX)
                .await
                .unwrap(),
        )
        .unwrap();
        let channel_id = create_body["id"].as_str().unwrap().to_string();

        // Create user2
        let user2_id = Uuid::new_v4().to_string();
        let pw = crate::auth::hash_password("pass2").unwrap();
        sqlx::query("INSERT INTO users (id, username, password_hash) VALUES (?, ?, ?)")
            .bind(&user2_id)
            .bind("user2")
            .bind(&pw)
            .execute(&pool)
            .await
            .unwrap();
        let secret = "test-secret";
        let token2 = crate::auth::create_token_pair(&user2_id, secret, 0)
            .unwrap()
            .access_token;

        // User2 tries to archive
        let resp = request(
            &mut app,
            Method::POST,
            &format!("/channels/{}/archive", channel_id),
            None,
            &token2,
        )
        .await;
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn test_unarchive_not_owner() {
        let (mut app, pool, _, token1) = setup().await;

        // Create + archive with user1
        let create_resp = request(
            &mut app,
            Method::POST,
            "/channels",
            Some(json!({"name": "Private"})),
            &token1,
        )
        .await;
        let create_body: Value = serde_json::from_slice(
            &axum::body::to_bytes(create_resp.into_body(), usize::MAX)
                .await
                .unwrap(),
        )
        .unwrap();
        let channel_id = create_body["id"].as_str().unwrap().to_string();
        let _ = request(
            &mut app,
            Method::POST,
            &format!("/channels/{}/archive", channel_id),
            None,
            &token1,
        )
        .await;

        // Create user2
        let user2_id = Uuid::new_v4().to_string();
        let pw = crate::auth::hash_password("pass2").unwrap();
        sqlx::query("INSERT INTO users (id, username, password_hash) VALUES (?, ?, ?)")
            .bind(&user2_id)
            .bind("user2")
            .bind(&pw)
            .execute(&pool)
            .await
            .unwrap();
        let secret = "test-secret";
        let token2 = crate::auth::create_token_pair(&user2_id, secret, 0)
            .unwrap()
            .access_token;

        // User2 tries to unarchive
        let resp = request(
            &mut app,
            Method::POST,
            &format!("/channels/{}/unarchive", channel_id),
            None,
            &token2,
        )
        .await;
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn test_archive_not_found() {
        let (mut app, _, _, token) = setup().await;

        let resp = request(
            &mut app,
            Method::POST,
            "/channels/nonexistent-id/archive",
            None,
            &token,
        )
        .await;
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_unarchive_not_found() {
        let (mut app, _, _, token) = setup().await;

        let resp = request(
            &mut app,
            Method::POST,
            "/channels/nonexistent-id/unarchive",
            None,
            &token,
        )
        .await;
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_archive_requires_auth() {
        let (mut app, _, _, _) = setup().await;

        let resp = request(
            &mut app,
            Method::POST,
            "/channels/some-id/archive",
            None,
            "",
        )
        .await;
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn test_archive_blocks_writes() {
        let (mut app, _, _, token) = setup().await;

        // Create channel
        let create_resp = request(
            &mut app,
            Method::POST,
            "/channels",
            Some(json!({"name": "Write Block"})),
            &token,
        )
        .await;
        let create_body: Value = serde_json::from_slice(
            &axum::body::to_bytes(create_resp.into_body(), usize::MAX)
                .await
                .unwrap(),
        )
        .unwrap();
        let channel_id = create_body["id"].as_str().unwrap().to_string();

        // Archive it
        let _ = request(
            &mut app,
            Method::POST,
            &format!("/channels/{}/archive", channel_id),
            None,
            &token,
        )
        .await;

        // Try to send a message — should be FORBIDDEN
        let msg_resp = request(
            &mut app,
            Method::POST,
            &format!("/channels/{}/messages", channel_id),
            Some(json!({"msg_type": "text", "payload": {"text": "hello"}})),
            &token,
        )
        .await;
        assert_eq!(msg_resp.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn test_archive_allows_reads() {
        let (mut app, _, _, token) = setup().await;

        // Create channel
        let create_resp = request(
            &mut app,
            Method::POST,
            "/channels",
            Some(json!({"name": "Read Test"})),
            &token,
        )
        .await;
        let create_body: Value = serde_json::from_slice(
            &axum::body::to_bytes(create_resp.into_body(), usize::MAX)
                .await
                .unwrap(),
        )
        .unwrap();
        let channel_id = create_body["id"].as_str().unwrap().to_string();

        // Archive it
        let _ = request(
            &mut app,
            Method::POST,
            &format!("/channels/{}/archive", channel_id),
            None,
            &token,
        )
        .await;

        // GET channel should still work
        let get_resp = request(
            &mut app,
            Method::GET,
            &format!("/channels/{}", channel_id),
            None,
            &token,
        )
        .await;
        assert_eq!(get_resp.status(), StatusCode::OK);

        // List channels should still include it
        let list_resp = request(&mut app, Method::GET, "/channels", None, &token).await;
        assert_eq!(list_resp.status(), StatusCode::OK);
        let list_body: Value = serde_json::from_slice(
            &axum::body::to_bytes(list_resp.into_body(), usize::MAX)
                .await
                .unwrap(),
        )
        .unwrap();
        let channels = list_body["channels"].as_array().unwrap();
        assert!(channels.iter().any(|c| c["id"] == channel_id));
    }
}
