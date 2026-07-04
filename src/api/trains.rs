use axum::{
    extract::{Path, State},
    Json,
};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use std::sync::Arc;
use uuid::Uuid;

use crate::api::messages::{MessageResponse, MessageRow};
use crate::auth::middleware::AuthenticatedUser;
use crate::error::AppError;
use crate::ws::protocol::ServerEvent;
use crate::AppState;

// ---------------------------------------------------------------------------
// Response / Request types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrainReply {
    pub user_id: String,
    pub username: String,
    pub display_name: Option<String>,
    pub avatar_url: Option<String>,
    pub content: String,
    pub created_at: i64,
}

#[derive(Debug, Serialize)]
pub struct TrainResponse {
    pub id: String,
    pub channel_id: String,
    pub creator_id: String,
    pub title: String,
    pub replies: Vec<TrainReply>,
    pub created_at: i64,
}

#[derive(Debug, Deserialize)]
pub struct JoinTrainRequest {
    pub content: String,
}

#[derive(Debug, Serialize)]
pub struct JoinTrainResponse {
    pub message: MessageResponse,
    pub train: TrainResponse,
}

// ---------------------------------------------------------------------------
// Internal DB row
// ---------------------------------------------------------------------------

#[derive(Debug, FromRow)]
struct TrainRow {
    id: String,
    channel_id: String,
    creator_id: String,
    title: String,
    replies: String,
    created_at: i64,
}

impl TrainRow {
    fn parse_replies(&self) -> Result<Vec<TrainReply>, AppError> {
        if self.replies.trim().is_empty() {
            return Ok(Vec::new());
        }
        serde_json::from_str(&self.replies)
            .map_err(|e| AppError::Internal(format!("Failed to parse train replies: {e}")))
    }

    fn into_response(self, replies: Vec<TrainReply>) -> TrainResponse {
        TrainResponse {
            id: self.id,
            channel_id: self.channel_id,
            creator_id: self.creator_id,
            title: self.title,
            replies,
            created_at: self.created_at,
        }
    }
}

const TRAIN_SELECT: &str =
    "SELECT id, channel_id, creator_id, title, replies, created_at FROM trains WHERE id = ?";

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

/// GET /api/trains/{train_id}
///
/// Returns the train record (title + replies). Public read (no auth required
/// for fetching — same as message listing is gated by channel existence).
pub async fn get_train(
    State(state): State<Arc<AppState>>,
    Path(train_id): Path<String>,
) -> Result<Json<TrainResponse>, AppError> {
    let row: TrainRow = sqlx::query_as::<_, TrainRow>(TRAIN_SELECT)
        .bind(&train_id)
        .fetch_optional(&state.pool)
        .await?
        .ok_or_else(|| AppError::NotFound("Train not found".to_string()))?;

    let replies = row.parse_replies()?;
    Ok(Json(row.into_response(replies)))
}

/// POST /api/trains/{train_id}/join
///
/// Append a reply to an existing train. Auth required. The caller must be a
/// member of the train's channel and must not already be in `replies`.
/// Inserts a new `_train` message and broadcasts both `NewMsg` and
/// `TrainUpdated`.
pub async fn join_train(
    auth: AuthenticatedUser,
    State(state): State<Arc<AppState>>,
    Path(train_id): Path<String>,
    Json(body): Json<JoinTrainRequest>,
) -> Result<(axum::http::StatusCode, Json<JoinTrainResponse>), AppError> {
    let user_id = auth.0;

    // 1. Load train
    let train: TrainRow = sqlx::query_as::<_, TrainRow>(TRAIN_SELECT)
        .bind(&train_id)
        .fetch_optional(&state.pool)
        .await?
        .ok_or_else(|| AppError::NotFound("Train not found".to_string()))?;

    // 2. Parse replies
    let mut replies = train.parse_replies()?;

    // 3. Duplicate-join guard
    if replies.iter().any(|r| r.user_id == user_id) {
        return Err(AppError::Conflict(
            "You have already joined this train".to_string(),
        ));
    }

    // 4. Channel membership check
    let _: (String,) =
        sqlx::query_as("SELECT user_id FROM channel_members WHERE channel_id = ? AND user_id = ?")
            .bind(&train.channel_id)
            .bind(&user_id)
            .fetch_optional(&state.pool)
            .await?
            .ok_or_else(|| {
                AppError::Forbidden("You are not a member of this channel".to_string())
            })?;

    // 5. Hydrate sender info
    let (username, display_name, avatar_url): (String, String, String) =
        sqlx::query_as::<_, (String, String, String)>(
            "SELECT username, display_name, avatar_url FROM users WHERE id = ?",
        )
        .bind(&user_id)
        .fetch_one(&state.pool)
        .await?;

    // 6. Append reply
    let reply = TrainReply {
        user_id: user_id.clone(),
        username: username.clone(),
        display_name: if display_name.is_empty() {
            None
        } else {
            Some(display_name.clone())
        },
        avatar_url: if avatar_url.is_empty() {
            None
        } else {
            Some(avatar_url.clone())
        },
        content: body.content,
        created_at: chrono::Utc::now().timestamp(),
    };
    replies.push(reply);

    let replies_json = serde_json::to_string(&replies)
        .map_err(|e| AppError::Internal(format!("Failed to serialize replies: {e}")))?;

    sqlx::query("UPDATE trains SET replies = ? WHERE id = ?")
        .bind(&replies_json)
        .bind(&train_id)
        .execute(&state.pool)
        .await?;

    // 7. Insert a visible `_train` message (same card payload as the original)
    let payload = serde_json::json!({
        "_train": true,
        "train_id": train.id,
        "title": train.title,
    });
    let payload_str = serde_json::to_string(&payload).unwrap_or_default();
    let msg_id = Uuid::new_v4().to_string();

    let inserted: MessageRow = sqlx::query_as::<_, MessageRow>(
        "INSERT INTO messages (msg_id, channel_id, sender_id, msg_type, payload) \
         VALUES (?, ?, ?, 'text', ?) \
         RETURNING id, msg_id, channel_id, sender_id, msg_type, payload, \
                   thread_parent_id, deleted_at, edited_at, created_at",
    )
    .bind(&msg_id)
    .bind(&train.channel_id)
    .bind(&user_id)
    .bind(&payload_str)
    .fetch_one(&state.pool)
    .await?;

    let mut message_resp = MessageResponse::from(inserted);
    message_resp.sender_name = username;
    message_resp.sender_display_name = display_name;

    // 8 + 9. Broadcast NewMsg + TrainUpdated
    state.ws_pool.notify_channel(
        &train.channel_id,
        ServerEvent::NewMsg {
            channel_id: train.channel_id.clone(),
            cursor: message_resp.id,
            sender_id: user_id.clone(),
            msg_type: "text".to_string(),
            preview: String::new(),
        },
    );
    state.ws_pool.notify_channel(
        &train.channel_id,
        ServerEvent::TrainUpdated {
            train_id: train.id.clone(),
            channel_id: train.channel_id.clone(),
        },
    );

    // 10. Return 201 with { message, train }
    let train_response = train.into_response(replies);
    Ok((
        axum::http::StatusCode::CREATED,
        Json(JoinTrainResponse {
            message: message_resp,
            train: train_response,
        }),
    ))
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
    use crate::ws::protocol::ServerEvent;

    /// Mirror of the messages.rs test harness. Returns
    /// `(Router, pool, user_id, token, ws_pool)`.
    async fn setup() -> (Router, sqlx::SqlitePool, String, String, Arc<ws::ConnectionPool>) {
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
        // SAFETY: test-only; single-threaded within each test process.
        unsafe {
            std::env::set_var("JWT_SECRET", secret);
        }
        let token = crate::auth::create_token_pair(&user_id, secret)
            .expect("Failed to create token")
            .access_token;

        let ws_pool = Arc::new(ws::ConnectionPool::new());
        let state = Arc::new(AppState {
            pool: pool.clone(),
            ws_pool: ws_pool.clone(),
            config: crate::AppConfig {
                jwt_secret: secret.to_string(),
                invite_code: "TEST".to_string(),
                ..crate::AppConfig::test_default()
            },
        });

        let app = crate::api::routes().with_state(state);
        (app, pool, user_id, token, ws_pool)
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

    async fn create_channel(app: &mut Router, token: &str) -> String {
        let resp = request(
            app,
            Method::POST,
            "/channels",
            Some(json!({"name": "Test Channel"})),
            token,
        )
        .await;
        assert_eq!(resp.status(), StatusCode::CREATED);
        let body: Value = serde_json::from_slice(
            &axum::body::to_bytes(resp.into_body(), usize::MAX)
                .await
                .unwrap(),
        )
        .unwrap();
        body["id"].as_str().unwrap().to_string()
    }

    /// Create a train via the `/train` slash command and return
    /// `(message_body, train_id)`.
    async fn create_train(
        app: &mut Router,
        channel_id: &str,
        token: &str,
        title: &str,
    ) -> (Value, String) {
        let resp = request(
            app,
            Method::POST,
            &format!("/channels/{}/messages", channel_id),
            Some(json!({
                "msg_type": "text",
                "payload": {
                    "_command": true,
                    "command": "train",
                    "args": title,
                }
            })),
            token,
        )
        .await;
        assert_eq!(resp.status(), StatusCode::CREATED);
        let body: Value = serde_json::from_slice(
            &axum::body::to_bytes(resp.into_body(), usize::MAX)
                .await
                .unwrap(),
        )
        .unwrap();
        let train_id = body["payload"]["train_id"]
            .as_str()
            .expect("train message must carry train_id")
            .to_string();
        (body, train_id)
    }

    /// Create a second user and add them as a channel member. Returns
    /// `(user_id, token)`.
    async fn create_member(
        pool: &sqlx::SqlitePool,
        channel_id: &str,
        username: &str,
    ) -> (String, String) {
        let user_id = Uuid::new_v4().to_string();
        let pw = crate::auth::hash_password("pass2").unwrap();
        sqlx::query("INSERT INTO users (id, username, password_hash) VALUES (?, ?, ?)")
            .bind(&user_id)
            .bind(username)
            .bind(&pw)
            .execute(pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO channel_members (channel_id, user_id, role) VALUES (?, ?, 'member')")
            .bind(channel_id)
            .bind(&user_id)
            .execute(pool)
            .await
            .unwrap();
        let token = crate::auth::create_token_pair(&user_id, "test-secret")
            .unwrap()
            .access_token;
        (user_id, token)
    }

    // ── /train command ───────────────────────────────────────────────

    #[tokio::test]
    async fn test_train_command_creates_train_and_message() {
        let (mut app, pool, _uid, token, _) = setup().await;
        let channel_id = create_channel(&mut app, &token).await;

        let (msg, train_id) = create_train(&mut app, &channel_id, &token, "午餐接龙").await;

        // Message carries the _train payload and the same title.
        assert_eq!(msg["payload"]["_train"], true);
        assert_eq!(msg["payload"]["train_id"], train_id);
        assert_eq!(msg["payload"]["title"], "午餐接龙");
        assert_eq!(msg["msg_type"], "text");

        // Train row exists with empty replies.
        let row: TrainRow = sqlx::query_as::<_, TrainRow>(TRAIN_SELECT)
            .bind(&train_id)
            .fetch_one(&pool)
            .await
            .expect("train row must exist");
        assert_eq!(row.title, "午餐接龙");
        assert_eq!(row.channel_id, channel_id);
        assert_eq!(row.replies, "[]", "new train must start with empty replies");
    }

    #[tokio::test]
    async fn test_train_command_rejects_empty_title() {
        let (mut app, _pool, _uid, token, _) = setup().await;
        let channel_id = create_channel(&mut app, &token).await;

        let resp = request(
            &mut app,
            Method::POST,
            &format!("/channels/{}/messages", channel_id),
            Some(json!({
                "msg_type": "text",
                "payload": {"_command": true, "command": "train", "args": "   "}
            })),
            &token,
        )
        .await;
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    // ── join_train ───────────────────────────────────────────────────

    #[tokio::test]
    async fn test_join_train() {
        let (mut app, pool, _uid, token, _) = setup().await;
        let channel_id = create_channel(&mut app, &token).await;
        let (_msg, train_id) = create_train(&mut app, &channel_id, &token, "午餐接龙").await;

        let (user2_id, token2) = create_member(&pool, &channel_id, "alice").await;

        let resp = request(
            &mut app,
            Method::POST,
            &format!("/trains/{}/join", train_id),
            Some(json!({"content": "+1"})),
            &token2,
        )
        .await;
        assert_eq!(resp.status(), StatusCode::CREATED);
        let body: Value = serde_json::from_slice(
            &axum::body::to_bytes(resp.into_body(), usize::MAX)
                .await
                .unwrap(),
        )
        .unwrap();

        // Train has 1 reply referencing user2.
        let replies = body["train"]["replies"].as_array().unwrap();
        assert_eq!(replies.len(), 1);
        assert_eq!(replies[0]["user_id"], user2_id);
        assert_eq!(replies[0]["username"], "alice");
        assert_eq!(replies[0]["content"], "+1");

        // A new message was inserted with the train payload.
        assert_eq!(body["message"]["payload"]["_train"], true);
        assert_eq!(body["message"]["payload"]["train_id"], train_id);
        assert_eq!(body["message"]["sender_id"], user2_id);
    }

    #[tokio::test]
    async fn test_join_train_duplicate() {
        let (mut app, pool, _uid, token, _) = setup().await;
        let channel_id = create_channel(&mut app, &token).await;
        let (_msg, train_id) = create_train(&mut app, &channel_id, &token, "午餐接龙").await;

        let (_user2_id, token2) = create_member(&pool, &channel_id, "alice").await;

        let resp1 = request(
            &mut app,
            Method::POST,
            &format!("/trains/{}/join", train_id),
            Some(json!({"content": "+1"})),
            &token2,
        )
        .await;
        assert_eq!(resp1.status(), StatusCode::CREATED);

        let resp2 = request(
            &mut app,
            Method::POST,
            &format!("/trains/{}/join", train_id),
            Some(json!({"content": "+1 again"})),
            &token2,
        )
        .await;
        assert_eq!(resp2.status(), StatusCode::CONFLICT);
    }

    #[tokio::test]
    async fn test_join_train_non_member() {
        let (mut app, pool, _uid, token, _) = setup().await;
        let channel_id = create_channel(&mut app, &token).await;
        let (_msg, train_id) = create_train(&mut app, &channel_id, &token, "午餐接龙").await;

        // Second user exists but is NOT a channel member.
        let user2_id = Uuid::new_v4().to_string();
        let pw = crate::auth::hash_password("pass2").unwrap();
        sqlx::query("INSERT INTO users (id, username, password_hash) VALUES (?, ?, ?)")
            .bind(&user2_id)
            .bind("outsider")
            .bind(&pw)
            .execute(&pool)
            .await
            .unwrap();
        let token2 = crate::auth::create_token_pair(&user2_id, "test-secret")
            .unwrap()
            .access_token;

        let resp = request(
            &mut app,
            Method::POST,
            &format!("/trains/{}/join", train_id),
            Some(json!({"content": "+1"})),
            &token2,
        )
        .await;
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn test_join_train_not_found() {
        let (mut app, _pool, _uid, token, _) = setup().await;
        let resp = request(
            &mut app,
            Method::POST,
            "/trains/no-such-train/join",
            Some(json!({"content": "+1"})),
            &token,
        )
        .await;
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    // ── get_train ────────────────────────────────────────────────────

    #[tokio::test]
    async fn test_get_train() {
        let (mut app, pool, _uid, token, _) = setup().await;
        let channel_id = create_channel(&mut app, &token).await;
        let (_msg, train_id) = create_train(&mut app, &channel_id, &token, "午餐接龙").await;

        let (_user2_id, token2) = create_member(&pool, &channel_id, "alice").await;
        let _ = request(
            &mut app,
            Method::POST,
            &format!("/trains/{}/join", train_id),
            Some(json!({"content": "+1"})),
            &token2,
        )
        .await;

        let resp = request(
            &mut app,
            Method::GET,
            &format!("/trains/{}", train_id),
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
        assert_eq!(body["id"], train_id);
        assert_eq!(body["title"], "午餐接龙");
        assert_eq!(body["channel_id"], channel_id);
        let replies = body["replies"].as_array().unwrap();
        assert_eq!(replies.len(), 1);
        assert_eq!(replies[0]["username"], "alice");
        assert_eq!(replies[0]["content"], "+1");
    }

    #[tokio::test]
    async fn test_get_train_not_found() {
        let (mut app, _pool, _uid, token, _) = setup().await;
        let resp = request(
            &mut app,
            Method::GET,
            "/trains/no-such-train",
            None,
            &token,
        )
        .await;
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    // ── TrainUpdated WS event ────────────────────────────────────────

    #[tokio::test]
    async fn test_train_updated_ws_event() {
        let (mut app, pool, _uid, token, ws_pool) = setup().await;
        let channel_id = create_channel(&mut app, &token).await;
        let (_msg, train_id) = create_train(&mut app, &channel_id, &token, "午餐接龙").await;

        let (_user2_id, token2) = create_member(&pool, &channel_id, "alice").await;

        let mut rx = ws_pool.register("listener", "conn-1");
        while rx.try_recv().is_ok() {}

        let resp = request(
            &mut app,
            Method::POST,
            &format!("/trains/{}/join", train_id),
            Some(json!({"content": "+1"})),
            &token2,
        )
        .await;
        assert_eq!(resp.status(), StatusCode::CREATED);

        // Collect events: expect NewMsg then TrainUpdated (broadcast order).
        let mut saw_new_msg = false;
        let mut saw_train_updated = false;
        for _ in 0..2 {
            let event = rx
                .try_recv()
                .expect("expected at least NewMsg + TrainUpdated events");
            match event {
                ServerEvent::NewMsg {
                    channel_id: ch, ..
                } => {
                    assert_eq!(ch, channel_id);
                    saw_new_msg = true;
                }
                ServerEvent::TrainUpdated {
                    train_id: tid,
                    channel_id: ch,
                } => {
                    assert_eq!(tid, train_id);
                    assert_eq!(ch, channel_id);
                    saw_train_updated = true;
                }
                other => panic!("unexpected WS event: {other:?}"),
            }
        }
        assert!(saw_new_msg, "NewMsg must be broadcast on join");
        assert!(
            saw_train_updated,
            "TrainUpdated must be broadcast on join"
        );
    }

    // ── /train command broadcasts NewMsg ─────────────────────────────

    #[tokio::test]
    async fn test_train_command_broadcasts_new_msg() {
        let (mut app, _pool, _uid, token, ws_pool) = setup().await;
        let channel_id = create_channel(&mut app, &token).await;

        let mut rx = ws_pool.register("listener", "conn-1");
        while rx.try_recv().is_ok() {}

        let (_msg, _train_id) =
            create_train(&mut app, &channel_id, &token, "午餐接龙").await;

        let event = rx.try_recv().expect("expected NewMsg after /train");
        match event {
            ServerEvent::NewMsg {
                channel_id: ch, ..
            } => assert_eq!(ch, channel_id),
            other => panic!("expected NewMsg, got {other:?}"),
        }
    }
}
