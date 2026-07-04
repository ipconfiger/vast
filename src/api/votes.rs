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

/// Internal representation of a single vote option. `voter_ids` is visible
/// only inside the server — never serialized to clients.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VoteOption {
    pub id: String,
    pub text: String,
    pub voter_ids: Vec<String>,
}

/// Anonymous option returned to clients. Replaces `voter_ids` with `count`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VoteOptionResponse {
    pub id: String,
    pub text: String,
    pub count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VoteResponse {
    pub id: String,
    pub channel_id: String,
    pub creator_id: String,
    pub title: String,
    pub options: Vec<VoteOptionResponse>,
    /// option_id the current user voted for, or null if they haven't voted.
    pub my_vote: Option<String>,
    pub created_at: i64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CastVoteRequest {
    pub option_id: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CastVoteResponse {
    pub message: MessageResponse,
    pub vote: VoteResponse,
}

// ---------------------------------------------------------------------------
// Internal DB row
// ---------------------------------------------------------------------------

#[derive(Debug, FromRow)]
struct VoteRow {
    id: String,
    channel_id: String,
    creator_id: String,
    title: String,
    options: String,
    created_at: i64,
}

impl VoteRow {
    fn parse_options(&self) -> Result<Vec<VoteOption>, AppError> {
        if self.options.trim().is_empty() {
            return Ok(Vec::new());
        }
        serde_json::from_str(&self.options)
            .map_err(|e| AppError::Internal(format!("Failed to parse vote options: {e}")))
    }

    /// Build the anonymous response. `user_id` determines `my_vote`.
    fn into_response(self, options: &[VoteOption], user_id: &str) -> VoteResponse {
        let my_vote = options
            .iter()
            .find(|o| o.voter_ids.iter().any(|v| v == user_id))
            .map(|o| o.id.clone());

        let options_resp = options
            .iter()
            .map(|o| VoteOptionResponse {
                id: o.id.clone(),
                text: o.text.clone(),
                count: o.voter_ids.len(),
            })
            .collect();

        VoteResponse {
            id: self.id,
            channel_id: self.channel_id,
            creator_id: self.creator_id,
            title: self.title,
            options: options_resp,
            my_vote,
            created_at: self.created_at,
        }
    }
}

const VOTE_SELECT: &str =
    "SELECT id, channel_id, creator_id, title, options, created_at FROM votes WHERE id = ?";

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

/// GET /api/votes/{vote_id}
///
/// Returns the vote record with anonymous option counts and the caller's
/// own vote (`my_vote`). Auth required — the user id drives `my_vote`.
pub async fn get_vote(
    auth: AuthenticatedUser,
    State(state): State<Arc<AppState>>,
    Path(vote_id): Path<String>,
) -> Result<Json<VoteResponse>, AppError> {
    let user_id = auth.0;

    let row: VoteRow = sqlx::query_as::<_, VoteRow>(VOTE_SELECT)
        .bind(&vote_id)
        .fetch_optional(&state.pool)
        .await?
        .ok_or_else(|| AppError::NotFound("Vote not found".to_string()))?;

    let options = row.parse_options()?;
    Ok(Json(row.into_response(&options, &user_id)))
}

/// POST /api/votes/{vote_id}/vote
///
/// Cast a vote for `option_id`. Auth required. The caller must be a member
/// of the vote's channel and must not have already voted in this poll.
/// Inserts a visible `_vote` message and broadcasts both `NewMsg` and
/// `VoteUpdated`.
pub async fn cast_vote(
    auth: AuthenticatedUser,
    State(state): State<Arc<AppState>>,
    Path(vote_id): Path<String>,
    Json(body): Json<CastVoteRequest>,
) -> Result<(axum::http::StatusCode, Json<CastVoteResponse>), AppError> {
    let user_id = auth.0;

    // 1. Load vote
    let vote: VoteRow = sqlx::query_as::<_, VoteRow>(VOTE_SELECT)
        .bind(&vote_id)
        .fetch_optional(&state.pool)
        .await?
        .ok_or_else(|| AppError::NotFound("Vote not found".to_string()))?;

    // 2. Parse options
    let mut options = vote.parse_options()?;

    // 3. Duplicate-vote guard — across ALL options
    if options.iter().any(|o| o.voter_ids.iter().any(|v| v == &user_id)) {
        return Err(AppError::Conflict("您已投票".to_string()));
    }

    // 4. Channel membership check
    let _: (String,) =
        sqlx::query_as("SELECT user_id FROM channel_members WHERE channel_id = ? AND user_id = ?")
            .bind(&vote.channel_id)
            .bind(&user_id)
            .fetch_optional(&state.pool)
            .await?
            .ok_or_else(|| {
                AppError::Forbidden("You are not a member of this channel".to_string())
            })?;

    // 5. Find the target option and append the voter
    let target = options
        .iter_mut()
        .find(|o| o.id == body.option_id)
        .ok_or_else(|| AppError::BadRequest("Invalid option_id".to_string()))?;
    target.voter_ids.push(user_id.clone());

    let options_json = serde_json::to_string(&options)
        .map_err(|e| AppError::Internal(format!("Failed to serialize vote options: {e}")))?;

    sqlx::query("UPDATE votes SET options = ? WHERE id = ?")
        .bind(&options_json)
        .bind(&vote_id)
        .execute(&state.pool)
        .await?;

    // 6. Hydrate sender info
    let (username, display_name): (String, String) =
        sqlx::query_as::<_, (String, String)>(
            "SELECT username, display_name FROM users WHERE id = ?",
        )
        .bind(&user_id)
        .fetch_one(&state.pool)
        .await?;

    // 7. Insert a visible `_vote` message
    let payload = serde_json::json!({
        "_vote": true,
        "vote_id": vote.id,
        "title": vote.title,
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
    .bind(&vote.channel_id)
    .bind(&user_id)
    .bind(&payload_str)
    .fetch_one(&state.pool)
    .await?;

    let mut message_resp = MessageResponse::from(inserted);
    message_resp.sender_name = username;
    message_resp.sender_display_name = display_name;

    // 8. Broadcast NewMsg + VoteUpdated
    state.ws_pool.notify_channel(
        &vote.channel_id,
        ServerEvent::NewMsg {
            channel_id: vote.channel_id.clone(),
            cursor: message_resp.id,
            sender_id: user_id.clone(),
            msg_type: "text".to_string(),
            preview: String::new(),
        },
    );
    state.ws_pool.notify_channel(
        &vote.channel_id,
        ServerEvent::VoteUpdated {
            vote_id: vote.id.clone(),
            channel_id: vote.channel_id.clone(),
        },
    );

    // 9. Return 201 with { message, vote }
    let vote_response = vote.into_response(&options, &user_id);
    Ok((
        axum::http::StatusCode::CREATED,
        Json(CastVoteResponse {
            message: message_resp,
            vote: vote_response,
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

    /// Mirror of the trains.rs test harness. Returns
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
        let token = crate::auth::create_token_pair(&user_id, secret, 0)
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

    async fn body_to_json(resp: axum::response::Response) -> Value {
        serde_json::from_slice(
            &axum::body::to_bytes(resp.into_body(), usize::MAX)
                .await
                .unwrap(),
        )
        .unwrap()
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
        let body = body_to_json(resp).await;
        body["id"].as_str().unwrap().to_string()
    }

    /// Create a vote via the `_vote_request` payload and return
    /// `(message_body, vote_id)`.
    async fn create_vote(
        app: &mut Router,
        channel_id: &str,
        token: &str,
        title: &str,
        options: &[&str],
    ) -> (Value, String) {
        let opts_arr: Vec<Value> = options.iter().map(|s| json!(s)).collect();
        let resp = request(
            app,
            Method::POST,
            &format!("/channels/{}/messages", channel_id),
            Some(json!({
                "msg_type": "text",
                "payload": {
                    "_vote_request": true,
                    "title": title,
                    "options": opts_arr,
                }
            })),
            token,
        )
        .await;
        assert_eq!(resp.status(), StatusCode::CREATED);
        let body = body_to_json(resp).await;
        let vote_id = body["payload"]["vote_id"]
            .as_str()
            .expect("vote message must carry vote_id")
            .to_string();
        (body, vote_id)
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
        let token = crate::auth::create_token_pair(&user_id, "test-secret", 0)
            .unwrap()
            .access_token;
        (user_id, token)
    }

    // ── _vote_request command ────────────────────────────────────────

    #[tokio::test]
    async fn test_vote_command_creates_vote_and_message() {
        let (mut app, pool, _uid, token, _) = setup().await;
        let channel_id = create_channel(&mut app, &token).await;

        let (msg, vote_id) =
            create_vote(&mut app, &channel_id, &token, "Where to eat?", &["Sushi", "Pizza"]).await;

        // Message carries the _vote payload (NOT _vote_request / options).
        assert_eq!(msg["payload"]["_vote"], true);
        assert_eq!(msg["payload"]["vote_id"], vote_id);
        assert_eq!(msg["payload"]["title"], "Where to eat?");
        assert_eq!(msg["msg_type"], "text");
        assert!(msg["payload"].get("_vote_request").is_none());
        assert!(msg["payload"].get("options").is_none());

        // Vote row exists with 2 options, both empty voter_ids.
        let row: VoteRow = sqlx::query_as::<_, VoteRow>(VOTE_SELECT)
            .bind(&vote_id)
            .fetch_one(&pool)
            .await
            .expect("vote row must exist");
        assert_eq!(row.title, "Where to eat?");
        assert_eq!(row.channel_id, channel_id);
        let opts = row.parse_options().unwrap();
        assert_eq!(opts.len(), 2);
        assert!(opts.iter().all(|o| o.voter_ids.is_empty()));
    }

    #[tokio::test]
    async fn test_vote_command_rejects_empty_title() {
        let (mut app, _pool, _uid, token, _) = setup().await;
        let channel_id = create_channel(&mut app, &token).await;

        let resp = request(
            &mut app,
            Method::POST,
            &format!("/channels/{}/messages", channel_id),
            Some(json!({
                "msg_type": "text",
                "payload": {
                    "_vote_request": true,
                    "title": "   ",
                    "options": ["A"],
                }
            })),
            &token,
        )
        .await;
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_vote_command_rejects_empty_options() {
        let (mut app, _pool, _uid, token, _) = setup().await;
        let channel_id = create_channel(&mut app, &token).await;

        let resp = request(
            &mut app,
            Method::POST,
            &format!("/channels/{}/messages", channel_id),
            Some(json!({
                "msg_type": "text",
                "payload": {
                    "_vote_request": true,
                    "title": "Pick one",
                    "options": [],
                }
            })),
            &token,
        )
        .await;
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    // ── get_vote ─────────────────────────────────────────────────────

    #[tokio::test]
    async fn test_get_vote_empty() {
        let (mut app, _pool, uid, token, _) = setup().await;
        let channel_id = create_channel(&mut app, &token).await;
        let (_msg, vote_id) =
            create_vote(&mut app, &channel_id, &token, "Where to eat?", &["Sushi", "Pizza"]).await;

        let resp = request(
            &mut app,
            Method::GET,
            &format!("/votes/{}", vote_id),
            None,
            &token,
        )
        .await;
        assert_eq!(resp.status(), StatusCode::OK);
        let body = body_to_json(resp).await;

        assert_eq!(body["id"], vote_id);
        assert_eq!(body["title"], "Where to eat?");
        assert_eq!(body["channelId"], channel_id);

        // Options: both count=0, no voter_ids field anywhere.
        let opts = body["options"].as_array().unwrap();
        assert_eq!(opts.len(), 2);
        assert_eq!(opts[0]["count"], 0);
        assert_eq!(opts[1]["count"], 0);
        assert!(opts[0].get("voter_ids").is_none());
        assert!(opts[1].get("voter_ids").is_none());

        // Creator hasn't voted yet → my_vote null. Also verify the
        // authenticated user id matches the creator.
        assert_eq!(body["creatorId"], uid);
        assert!(body["myVote"].is_null());
    }

    #[tokio::test]
    async fn test_get_vote_not_found() {
        let (mut app, _pool, _uid, token, _) = setup().await;
        let resp = request(
            &mut app,
            Method::GET,
            "/votes/no-such-vote",
            None,
            &token,
        )
        .await;
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    // ── cast_vote ────────────────────────────────────────────────────

    #[tokio::test]
    async fn test_cast_vote() {
        let (mut app, pool, _uid, token, _) = setup().await;
        let channel_id = create_channel(&mut app, &token).await;
        let (_msg, vote_id) =
            create_vote(&mut app, &channel_id, &token, "Where to eat?", &["Sushi", "Pizza"]).await;

        let (user2_id, token2) = create_member(&pool, &channel_id, "alice").await;

        // Extract option_id for "Sushi" from the GET response.
        let get_resp = request(
            &mut app,
            Method::GET,
            &format!("/votes/{}", vote_id),
            None,
            &token,
        )
        .await;
        let get_body = body_to_json(get_resp).await;
        let sushi_id = get_body["options"][0]["id"].as_str().unwrap().to_string();

        let resp = request(
            &mut app,
            Method::POST,
            &format!("/votes/{}/vote", vote_id),
            Some(json!({"optionId": sushi_id})),
            &token2,
        )
        .await;
        assert_eq!(resp.status(), StatusCode::CREATED);
        let body = body_to_json(resp).await;

        // Vote has 1 vote on the first option.
        let opts = body["vote"]["options"].as_array().unwrap();
        assert_eq!(opts[0]["count"], 1);
        assert_eq!(opts[1]["count"], 0);
        // myVote reflects the voter's choice.
        assert_eq!(body["vote"]["myVote"], sushi_id);
        // voter_ids must NEVER appear in the response.
        assert!(opts[0].get("voter_ids").is_none());
        assert!(opts[1].get("voter_ids").is_none());

        // A new message was inserted with the vote payload.
        assert_eq!(body["message"]["payload"]["_vote"], true);
        assert_eq!(body["message"]["payload"]["vote_id"], vote_id);
        assert_eq!(body["message"]["sender_id"], user2_id);

        // GET now returns count=1, myVote set for user2.
        let get2 = request(
            &mut app,
            Method::GET,
            &format!("/votes/{}", vote_id),
            None,
            &token2,
        )
        .await;
        let get2_body = body_to_json(get2).await;
        assert_eq!(get2_body["options"][0]["count"], 1);
        assert_eq!(get2_body["myVote"], sushi_id);
    }

    #[tokio::test]
    async fn test_cast_vote_duplicate() {
        let (mut app, pool, _uid, token, _) = setup().await;
        let channel_id = create_channel(&mut app, &token).await;
        let (_msg, vote_id) =
            create_vote(&mut app, &channel_id, &token, "Pick", &["A", "B"]).await;

        let (_user2_id, token2) = create_member(&pool, &channel_id, "alice").await;

        let get_resp = request(
            &mut app,
            Method::GET,
            &format!("/votes/{}", vote_id),
            None,
            &token,
        )
        .await;
        let get_body = body_to_json(get_resp).await;
        let opt_a = get_body["options"][0]["id"].as_str().unwrap().to_string();
        let opt_b = get_body["options"][1]["id"].as_str().unwrap().to_string();

        // First vote on A — OK.
        let resp1 = request(
            &mut app,
            Method::POST,
            &format!("/votes/{}/vote", vote_id),
            Some(json!({"optionId": opt_a})),
            &token2,
        )
        .await;
        assert_eq!(resp1.status(), StatusCode::CREATED);

        // Second vote on B — must 409 (changing vote is forbidden).
        let resp2 = request(
            &mut app,
            Method::POST,
            &format!("/votes/{}/vote", vote_id),
            Some(json!({"optionId": opt_b})),
            &token2,
        )
        .await;
        assert_eq!(resp2.status(), StatusCode::CONFLICT);
    }

    #[tokio::test]
    async fn test_cast_vote_non_member() {
        let (mut app, pool, _uid, token, _) = setup().await;
        let channel_id = create_channel(&mut app, &token).await;
        let (_msg, vote_id) =
            create_vote(&mut app, &channel_id, &token, "Pick", &["A"]).await;

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
        let token2 = crate::auth::create_token_pair(&user2_id, "test-secret", 0)
            .unwrap()
            .access_token;

        let get_resp = request(
            &mut app,
            Method::GET,
            &format!("/votes/{}", vote_id),
            None,
            &token,
        )
        .await;
        let get_body = body_to_json(get_resp).await;
        let opt_id = get_body["options"][0]["id"].as_str().unwrap().to_string();

        let resp = request(
            &mut app,
            Method::POST,
            &format!("/votes/{}/vote", vote_id),
            Some(json!({"optionId": opt_id})),
            &token2,
        )
        .await;
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn test_cast_vote_not_found() {
        let (mut app, _pool, _uid, token, _) = setup().await;
        let resp = request(
            &mut app,
            Method::POST,
            "/votes/no-such-vote/vote",
            Some(json!({"optionId": "x"})),
            &token,
        )
        .await;
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_cast_vote_invalid_option() {
        let (mut app, _pool, _uid, token, _) = setup().await;
        let channel_id = create_channel(&mut app, &token).await;
        let (_msg, vote_id) =
            create_vote(&mut app, &channel_id, &token, "Pick", &["A"]).await;

        let resp = request(
            &mut app,
            Method::POST,
            &format!("/votes/{}/vote", vote_id),
            Some(json!({"optionId": "nonexistent-option"})),
            &token,
        )
        .await;
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    /// Confirm the serialized VoteResponse NEVER contains `voter_ids`,
    /// even after a vote is cast.
    #[tokio::test]
    async fn test_vote_response_is_anonymous() {
        let (mut app, pool, _uid, token, _) = setup().await;
        let channel_id = create_channel(&mut app, &token).await;
        let (_msg, vote_id) =
            create_vote(&mut app, &channel_id, &token, "Pick", &["A", "B"]).await;

        let (_user2_id, token2) = create_member(&pool, &channel_id, "alice").await;

        let get_resp = request(
            &mut app,
            Method::GET,
            &format!("/votes/{}", vote_id),
            None,
            &token,
        )
        .await;
        let get_body = body_to_json(get_resp).await;
        let opt_a = get_body["options"][0]["id"].as_str().unwrap().to_string();

        let _ = request(
            &mut app,
            Method::POST,
            &format!("/votes/{}/vote", vote_id),
            Some(json!({"optionId": opt_a})),
            &token2,
        )
        .await;

        // GET the response as raw JSON and assert no "voter_ids" key appears.
        let resp = request(
            &mut app,
            Method::GET,
            &format!("/votes/{}", vote_id),
            None,
            &token2,
        )
        .await;
        let text = String::from_utf8(
            axum::body::to_bytes(resp.into_body(), usize::MAX)
                .await
                .unwrap()
                .to_vec(),
        )
        .unwrap();
        assert!(
            !text.contains("voter_ids"),
            "VoteResponse leaked voter_ids: {text}"
        );
        assert!(
            !text.contains("voterIds"),
            "VoteResponse leaked voterIds: {text}"
        );
    }

    // ── VoteUpdated WS event ─────────────────────────────────────────

    #[tokio::test]
    async fn test_vote_updated_ws_event() {
        let (mut app, pool, _uid, token, ws_pool) = setup().await;
        let channel_id = create_channel(&mut app, &token).await;
        let (_msg, vote_id) =
            create_vote(&mut app, &channel_id, &token, "Pick", &["A"]).await;

        let (_user2_id, token2) = create_member(&pool, &channel_id, "alice").await;

        let mut rx = ws_pool.register("listener", "conn-1");
        while rx.try_recv().is_ok() {}

        let get_resp = request(
            &mut app,
            Method::GET,
            &format!("/votes/{}", vote_id),
            None,
            &token,
        )
        .await;
        let get_body = body_to_json(get_resp).await;
        let opt_id = get_body["options"][0]["id"].as_str().unwrap().to_string();

        let resp = request(
            &mut app,
            Method::POST,
            &format!("/votes/{}/vote", vote_id),
            Some(json!({"optionId": opt_id})),
            &token2,
        )
        .await;
        assert_eq!(resp.status(), StatusCode::CREATED);

        // Collect events: expect NewMsg then VoteUpdated (broadcast order).
        let mut saw_new_msg = false;
        let mut saw_vote_updated = false;
        for _ in 0..2 {
            let event = rx
                .try_recv()
                .expect("expected at least NewMsg + VoteUpdated events");
            match event {
                ServerEvent::NewMsg {
                    channel_id: ch, ..
                } => {
                    assert_eq!(ch, channel_id);
                    saw_new_msg = true;
                }
                ServerEvent::VoteUpdated {
                    vote_id: vid,
                    channel_id: ch,
                } => {
                    assert_eq!(vid, vote_id);
                    assert_eq!(ch, channel_id);
                    saw_vote_updated = true;
                }
                other => panic!("unexpected WS event: {other:?}"),
            }
        }
        assert!(saw_new_msg, "NewMsg must be broadcast on cast_vote");
        assert!(
            saw_vote_updated,
            "VoteUpdated must be broadcast on cast_vote"
        );
    }

    // ── /vote command broadcasts NewMsg ──────────────────────────────

    #[tokio::test]
    async fn test_vote_command_broadcasts_new_msg() {
        let (mut app, _pool, _uid, token, ws_pool) = setup().await;
        let channel_id = create_channel(&mut app, &token).await;

        let mut rx = ws_pool.register("listener", "conn-1");
        while rx.try_recv().is_ok() {}

        let (_msg, _vote_id) =
            create_vote(&mut app, &channel_id, &token, "Pick", &["A"]).await;

        let event = rx
            .try_recv()
            .expect("expected NewMsg after _vote_request");
        match event {
            ServerEvent::NewMsg {
                channel_id: ch, ..
            } => assert_eq!(ch, channel_id),
            other => panic!("expected NewMsg, got {other:?}"),
        }
    }
}
