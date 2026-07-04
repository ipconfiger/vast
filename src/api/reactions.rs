use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use serde::{Deserialize, Serialize};
use sqlx::Row;
use std::sync::Arc;

use crate::auth::middleware::AuthenticatedUser;
use crate::error::AppError;
use crate::ws::protocol::{ReactionSummary, ServerEvent};
use crate::AppState;

// ---------------------------------------------------------------------------
// Request / Response types
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct AddReactionRequest {
    pub emoji: String,
}

#[derive(Debug, Serialize, PartialEq)]
pub struct ReactionResponse {
    pub emoji: String,
    pub count: i64,
    pub reacted_by_me: bool,
}

#[derive(Debug, Serialize)]
pub struct ReactionsListResponse {
    pub reactions: Vec<ReactionResponse>,
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Fetch the current aggregated reactions for a message and broadcast
/// a ReactionUpdate to the channel.
async fn broadcast_reaction_update(
    state: &Arc<AppState>,
    message_id: i64,
    channel_id: &str,
) -> Result<(), AppError> {
    let rows = sqlx::query(
        r#"SELECT emoji, COUNT(*) AS "count" FROM reactions WHERE message_id = ? GROUP BY emoji"#,
    )
    .bind(message_id)
    .fetch_all(&state.pool)
    .await?;

    let reactions: Vec<ReactionSummary> = rows
        .iter()
        .map(|row| ReactionSummary {
            emoji: row.get("emoji"),
            count: row.get("count"),
        })
        .collect();

    state.ws_pool.broadcast_to_channel(
        channel_id,
        &ServerEvent::ReactionUpdate {
            channel_id: channel_id.to_string(),
            message_cursor: message_id,
            reactions,
        },
    );

    Ok(())
}

/// Verify the message exists (not deleted) and return its channel_id.
/// Also verifies the user is a member of that channel.
async fn verify_message_access(
    pool: &sqlx::SqlitePool,
    user_id: &str,
    message_id: i64,
) -> Result<String, AppError> {
    let row = sqlx::query_as::<_, (i64, String)>(
        "SELECT id, channel_id FROM messages WHERE id = ? AND deleted_at IS NULL",
    )
    .bind(message_id)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| AppError::NotFound("Message not found".to_string()))?;

    let channel_id = row.1;
    crate::auth::middleware::require_membership(pool, user_id, &channel_id).await?;
    Ok(channel_id)
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

/// POST /api/messages/{message_id}/reactions
///
/// Add a reaction (emoji) to a message. Idempotent — duplicate adds are
/// silently ignored via INSERT OR IGNORE. Broadcasts ReactionUpdate.
pub async fn add_reaction(
    user: AuthenticatedUser,
    State(state): State<Arc<AppState>>,
    Path(message_id): Path<i64>,
    Json(req): Json<AddReactionRequest>,
) -> Result<(StatusCode, Json<serde_json::Value>), AppError> {
    let user_id = user.0;

    if req.emoji.trim().is_empty() {
        return Err(AppError::BadRequest("Emoji must not be empty".to_string()));
    }

    let channel_id = verify_message_access(&state.pool, &user_id, message_id).await?;

    sqlx::query(
        "INSERT OR IGNORE INTO reactions (message_id, user_id, emoji) VALUES (?, ?, ?)",
    )
    .bind(message_id)
    .bind(&user_id)
    .bind(&req.emoji)
    .execute(&state.pool)
    .await?;

    broadcast_reaction_update(&state, message_id, &channel_id).await?;

    Ok((StatusCode::CREATED, Json(serde_json::json!({"status": "ok"}))))
}

/// DELETE /api/messages/{message_id}/reactions/{emoji}
///
/// Remove the authenticated user's own reaction. Other users' reactions
/// are never affected. Broadcasts ReactionUpdate.
pub async fn remove_reaction(
    user: AuthenticatedUser,
    State(state): State<Arc<AppState>>,
    Path((message_id, emoji)): Path<(i64, String)>,
) -> Result<(StatusCode, Json<serde_json::Value>), AppError> {
    let user_id = user.0;

    let channel_id = verify_message_access(&state.pool, &user_id, message_id).await?;

    sqlx::query(
        "DELETE FROM reactions WHERE message_id = ? AND user_id = ? AND emoji = ?",
    )
    .bind(message_id)
    .bind(&user_id)
    .bind(&emoji)
    .execute(&state.pool)
    .await?;

    broadcast_reaction_update(&state, message_id, &channel_id).await?;

    Ok((StatusCode::OK, Json(serde_json::json!({"status": "ok"}))))
}

/// GET /api/messages/{message_id}/reactions
///
/// Return all reactions on a message, aggregated by emoji, with count
/// and whether the current user has reacted with that emoji.
pub async fn get_reactions(
    user: AuthenticatedUser,
    State(state): State<Arc<AppState>>,
    Path(message_id): Path<i64>,
) -> Result<Json<ReactionsListResponse>, AppError> {
    let user_id = user.0;

    let _channel_id = verify_message_access(&state.pool, &user_id, message_id).await?;

    let rows = sqlx::query(
        r#"
        SELECT
            r.emoji,
            COUNT(*) AS "count",
            MAX(CASE WHEN r.user_id = ? THEN 1 ELSE 0 END) AS "reacted_by_me"
        FROM reactions r
        WHERE r.message_id = ?
        GROUP BY r.emoji
        ORDER BY r.emoji
        "#,
    )
    .bind(&user_id)
    .bind(message_id)
    .fetch_all(&state.pool)
    .await?;

    let reactions: Vec<ReactionResponse> = rows
        .iter()
        .map(|row| ReactionResponse {
            emoji: row.get("emoji"),
            count: row.get("count"),
            reacted_by_me: row.get::<i64, _>("reacted_by_me") != 0,
        })
        .collect();

    Ok(Json(ReactionsListResponse { reactions }))
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
    use uuid::Uuid;

    use crate::ws;

    /// Percent-encode a string for use in a URI path.
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

    /// Helper to build a test app with an in-memory database.
    async fn setup() -> (Router, sqlx::SqlitePool, String, String) {
        let pool = sqlx::SqlitePool::connect("sqlite::memory:")
            .await
            .expect("Failed to create in-memory DB");
        sqlx::migrate!("db/migrations")
            .run(&pool)
            .await
            .expect("Failed to run migrations");

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
        let token = crate::auth::create_token_pair(&user_id, secret)
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

    /// Create a test channel via API and return its ID.
    async fn create_channel(app: &mut Router, token: &str) -> String {
        let resp = request(
            app,
            Method::POST,
            "/channels",
            Some(json!({"name": "Test Channel"})),
            token,
        )
        .await;
        let body: Value = serde_json::from_slice(
            &axum::body::to_bytes(resp.into_body(), usize::MAX)
                .await
                .unwrap(),
        )
        .unwrap();
        body["id"].as_str().unwrap().to_string()
    }

    /// Create a test message in a channel, return its id.
    async fn create_message(
        pool: &sqlx::SqlitePool,
        channel_id: &str,
        sender_id: &str,
    ) -> i64 {
        let msg_id = Uuid::new_v4().to_string();
        sqlx::query(
            "INSERT INTO messages (msg_id, channel_id, sender_id, msg_type, payload) VALUES (?, ?, ?, 'text', '{}')",
        )
        .bind(&msg_id)
        .bind(channel_id)
        .bind(sender_id)
        .execute(pool)
        .await
        .expect("Failed to insert test message")
        .last_insert_rowid()
    }

    // ── Tests ──────────────────────────────────────────────────────────

    #[tokio::test]
    async fn test_add_reaction() {
        let (mut app, pool, user_id, token) = setup().await;
        let channel_id = create_channel(&mut app, &token).await;
        let message_id = create_message(&pool, &channel_id, &user_id).await;

        let resp = request(
            &mut app,
            Method::POST,
            &format!("/messages/{}/reactions", message_id),
            Some(json!({"emoji": "👍"})),
            &token,
        )
        .await;
        assert_eq!(resp.status(), StatusCode::CREATED);

        // Verify count is 1 and reacted_by_me is true
        let get_resp = request(
            &mut app,
            Method::GET,
            &format!("/messages/{}/reactions", message_id),
            None,
            &token,
        )
        .await;
        assert_eq!(get_resp.status(), StatusCode::OK);
        let body: Value = serde_json::from_slice(
            &axum::body::to_bytes(get_resp.into_body(), usize::MAX)
                .await
                .unwrap(),
        )
        .unwrap();
        let reactions = body["reactions"].as_array().unwrap();
        assert_eq!(reactions.len(), 1);
        assert_eq!(reactions[0]["emoji"], "👍");
        assert_eq!(reactions[0]["count"], 1);
        assert_eq!(reactions[0]["reacted_by_me"], true);
    }

    #[tokio::test]
    async fn test_add_reaction_duplicate() {
        let (mut app, pool, user_id, token) = setup().await;
        let channel_id = create_channel(&mut app, &token).await;
        let message_id = create_message(&pool, &channel_id, &user_id).await;

        // Add same reaction twice
        for _ in 0..2 {
            let resp = request(
                &mut app,
                Method::POST,
                &format!("/messages/{}/reactions", message_id),
                Some(json!({"emoji": "👍"})),
                &token,
            )
            .await;
            assert_eq!(resp.status(), StatusCode::CREATED);
        }

        // Verify count is still 1 (idempotent)
        let get_resp = request(
            &mut app,
            Method::GET,
            &format!("/messages/{}/reactions", message_id),
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
        let reactions = body["reactions"].as_array().unwrap();
        assert_eq!(reactions.len(), 1);
        assert_eq!(reactions[0]["emoji"], "👍");
        assert_eq!(reactions[0]["count"], 1);
    }

    #[tokio::test]
    async fn test_remove_reaction() {
        let (mut app, pool, user_id, token) = setup().await;
        let channel_id = create_channel(&mut app, &token).await;
        let message_id = create_message(&pool, &channel_id, &user_id).await;

        // Add reaction
        let _ = request(
            &mut app,
            Method::POST,
            &format!("/messages/{}/reactions", message_id),
            Some(json!({"emoji": "👍"})),
            &token,
        )
        .await;

        // Remove it
        let resp = request(
            &mut app,
            Method::DELETE,
            &format!(
                "/messages/{}/reactions/{}",
                message_id,
                pct_encode("👍")
            ),
            None,
            &token,
        )
        .await;
        assert_eq!(resp.status(), StatusCode::OK);

        // Verify count is 0
        let get_resp = request(
            &mut app,
            Method::GET,
            &format!("/messages/{}/reactions", message_id),
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
        let reactions = body["reactions"].as_array().unwrap();
        assert!(reactions.is_empty());
    }

    #[tokio::test]
    async fn test_other_user_cannot_remove_my_reaction() {
        let (mut app, pool, user_id, token) = setup().await;
        let channel_id = create_channel(&mut app, &token).await;
        let message_id = create_message(&pool, &channel_id, &user_id).await;

        // Add reaction as user1
        let _ = request(
            &mut app,
            Method::POST,
            &format!("/messages/{}/reactions", message_id),
            Some(json!({"emoji": "👍"})),
            &token,
        )
        .await;

        // Create a second user and add them as channel member
        let user2_id = Uuid::new_v4().to_string();
        let pw = crate::auth::hash_password("pass2").unwrap();
        sqlx::query("INSERT INTO users (id, username, password_hash) VALUES (?, ?, ?)")
            .bind(&user2_id)
            .bind("user2")
            .bind(&pw)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query(
            "INSERT INTO channel_members (channel_id, user_id, role) VALUES (?, ?, 'member')",
        )
        .bind(&channel_id)
        .bind(&user2_id)
        .execute(&pool)
        .await
        .unwrap();
        let secret = "test-secret";
        let token2 = crate::auth::create_token_pair(&user2_id, secret)
            .unwrap()
            .access_token;

        // User2 tries to remove user1's reaction
        let resp = request(
            &mut app,
            Method::DELETE,
            &format!(
                "/messages/{}/reactions/{}",
                message_id,
                pct_encode("👍")
            ),
            None,
            &token2,
        )
        .await;
        assert_eq!(resp.status(), StatusCode::OK); // succeeds but removes nothing

        // Verify user1's reaction is still there
        let get_resp = request(
            &mut app,
            Method::GET,
            &format!("/messages/{}/reactions", message_id),
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
        let reactions = body["reactions"].as_array().unwrap();
        assert_eq!(reactions.len(), 1);
        assert_eq!(reactions[0]["count"], 1);
    }

    #[tokio::test]
    async fn test_reacted_by_me_correct_per_user() {
        let (mut app, pool, user_id, token) = setup().await;
        let channel_id = create_channel(&mut app, &token).await;
        let message_id = create_message(&pool, &channel_id, &user_id).await;

        // User1 adds reaction
        let _ = request(
            &mut app,
            Method::POST,
            &format!("/messages/{}/reactions", message_id),
            Some(json!({"emoji": "👍"})),
            &token,
        )
        .await;

        // Create user2, make them a channel member, add same reaction
        let user2_id = Uuid::new_v4().to_string();
        let pw = crate::auth::hash_password("pass2").unwrap();
        sqlx::query("INSERT INTO users (id, username, password_hash) VALUES (?, ?, ?)")
            .bind(&user2_id)
            .bind("user2")
            .bind(&pw)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query(
            "INSERT INTO channel_members (channel_id, user_id, role) VALUES (?, ?, 'member')",
        )
        .bind(&channel_id)
        .bind(&user2_id)
        .execute(&pool)
        .await
        .unwrap();
        let secret = "test-secret";
        let token2 = crate::auth::create_token_pair(&user2_id, secret)
            .unwrap()
            .access_token;

        let _ = request(
            &mut app,
            Method::POST,
            &format!("/messages/{}/reactions", message_id),
            Some(json!({"emoji": "👍"})),
            &token2,
        )
        .await;

        // Check reacted_by_me for user1
        let get_resp = request(
            &mut app,
            Method::GET,
            &format!("/messages/{}/reactions", message_id),
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
        let reactions = body["reactions"].as_array().unwrap();
        assert_eq!(reactions.len(), 1);
        assert_eq!(reactions[0]["count"], 2);
        assert_eq!(reactions[0]["reacted_by_me"], true);

        // Check reacted_by_me for user2
        let get_resp2 = request(
            &mut app,
            Method::GET,
            &format!("/messages/{}/reactions", message_id),
            None,
            &token2,
        )
        .await;
        let body2: Value = serde_json::from_slice(
            &axum::body::to_bytes(get_resp2.into_body(), usize::MAX)
                .await
                .unwrap(),
        )
        .unwrap();
        let reactions2 = body2["reactions"].as_array().unwrap();
        assert_eq!(reactions2[0]["reacted_by_me"], true);
    }

    #[tokio::test]
    async fn test_reactions_requires_auth() {
        let (mut app, _, _, _) = setup().await;

        // POST without auth
        let resp = request(
            &mut app,
            Method::POST,
            "/messages/1/reactions",
            Some(json!({"emoji": "👍"})),
            "",
        )
        .await;
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);

        // GET without auth
        let resp = request(
            &mut app,
            Method::GET,
            "/messages/1/reactions",
            None,
            "",
        )
        .await;
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);

        // DELETE without auth
        let resp = request(
            &mut app,
            Method::DELETE,
            &format!("/messages/1/reactions/{}", pct_encode("👍")),
            None,
            "",
        )
        .await;
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn test_reactions_message_not_found() {
        let (mut app, _, _, token) = setup().await;

        let resp = request(
            &mut app,
            Method::POST,
            "/messages/99999/reactions",
            Some(json!({"emoji": "👍"})),
            &token,
        )
        .await;
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }
}
