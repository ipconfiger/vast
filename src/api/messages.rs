use axum::{
    extract::{Path, Query, State},
    Json,
};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use std::collections::HashMap;
use std::sync::{Arc, OnceLock};
use uuid::Uuid;

use crate::auth::middleware::AuthenticatedUser;
use crate::bot::hermes::{ChatMessage, HermesClient};
use crate::db;
use crate::error::AppError;
use crate::ws::protocol::ServerEvent;
use crate::AppState;

// ---------------------------------------------------------------------------
// Bot mention bridge — process-global cooldown state
// ---------------------------------------------------------------------------

/// Per-(bot_id, channel_id) last-triggered instant. Process-global so the
/// cooldown persists across requests without touching `AppState` (which
/// would force fixture changes throughout the test suite).
type CooldownKey = (String, String);

static COOLDOWNS: OnceLock<tokio::sync::Mutex<HashMap<CooldownKey, std::time::Instant>>> =
    OnceLock::new();

fn cooldowns() -> &'static tokio::sync::Mutex<HashMap<CooldownKey, std::time::Instant>> {
    COOLDOWNS.get_or_init(|| tokio::sync::Mutex::new(HashMap::new()))
}

/// Minimum seconds between triggers of the same bot in the same channel.
const BOT_COOLDOWN_SECS: u64 = 10;

/// Hard cap on bot-to-bot chain depth (initial user mention = depth 0).
const BOT_MAX_CHAIN_DEPTH: u32 = 3;

/// Bot fields needed to call Hermes and post its reply. Bundled into a
/// struct so `trigger_bot_response` stays under clippy's argument budget.
#[derive(Debug, Clone)]
struct BotConfig {
    id: String,
    user_id: String,
    name: String,
    #[allow(dead_code)]
    display_name: String,
    api_url: String,
    api_key: String,
    system_prompt: String,
    model: String,
}

type BotRow = (String, String, String, String, String, String, String, String);

// ---------------------------------------------------------------------------
// Request / Response types
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct SendMessageRequest {
    pub msg_type: String,
    pub payload: serde_json::Value,
    #[serde(default)]
    pub thread_parent_id: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub struct ListMessagesParams {
    #[serde(default)]
    pub after_cursor: i64,
    #[serde(default = "default_limit")]
    pub limit: i64,
}

fn default_limit() -> i64 {
    50
}

#[derive(Debug, Serialize, FromRow)]
pub struct MessageRow {
    pub id: i64,
    pub msg_id: String,
    pub channel_id: String,
    pub sender_id: String,
    pub msg_type: String,
    pub payload: String,
    pub thread_parent_id: Option<i64>,
    pub deleted_at: Option<i64>,
    pub edited_at: Option<i64>,
    pub created_at: i64,
}

#[derive(Debug, Serialize)]
pub struct MessageResponse {
    pub id: i64,
    pub msg_id: String,
    pub channel_id: String,
    pub sender_id: String,
    pub sender_name: String,
    pub sender_display_name: String,
    pub sender_avatar_url: String,
    pub is_bot: bool,
    pub msg_type: String,
    pub payload: serde_json::Value,
    pub thread_parent_id: Option<i64>,
    pub deleted_at: Option<i64>,
    pub edited_at: Option<i64>,
    pub created_at: i64,
}

#[derive(Debug, Serialize)]
pub struct ListMessagesResponse {
    pub messages: Vec<MessageResponse>,
    pub next_cursor: i64,
    pub has_more: bool,
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

impl From<MessageRow> for MessageResponse {
    fn from(row: MessageRow) -> Self {
        let payload: serde_json::Value =
            serde_json::from_str(&row.payload).unwrap_or_default();
        Self {
            id: row.id,
            msg_id: row.msg_id,
            channel_id: row.channel_id,
            sender_id: row.sender_id.clone(),
            sender_name: row.sender_id,
            sender_display_name: String::new(),
            sender_avatar_url: String::new(),
            is_bot: false,
            msg_type: row.msg_type,
            payload,
            thread_parent_id: row.thread_parent_id,
            deleted_at: row.deleted_at,
            edited_at: row.edited_at,
            created_at: row.created_at,
        }
    }
}

/// Validate the msg_type field is one of the allowed values.
fn validate_msg_type(msg_type: &str) -> Result<(), AppError> {
    match msg_type {
        "text" | "file" | "code" => Ok(()),
        other => Err(AppError::UnsupportedMediaType(format!(
            "Invalid msg_type: '{other}'. Must be one of: text, file, code"
        ))),
    }
}

fn extract_preview(payload: &serde_json::Value) -> String {
    payload
        .get("text")
        .and_then(|v| v.as_str())
        .map(|s| {
            let s = s.trim();
            if s.len() > 100 {
                format!("{}...", &s[..100])
            } else {
                s.to_string()
            }
        })
        .unwrap_or_default()
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

/// POST /api/channels/{channel_id}/messages
///
/// Requires authentication. Verifies user is a channel member and the channel
/// is not archived. Inserts a new message and returns it with 201 Created.
pub async fn send_message(
    auth: AuthenticatedUser,
    State(state): State<Arc<AppState>>,
    Path(channel_id): Path<String>,
    Json(mut body): Json<SendMessageRequest>,
) -> Result<(axum::http::StatusCode, Json<MessageResponse>), AppError> {
    let user_id = auth.0;

    validate_msg_type(&body.msg_type)?;

    let is_archived: (bool,) = sqlx::query_as(
        "SELECT is_archived FROM channels WHERE id = ?",
    )
    .bind(&channel_id)
    .fetch_optional(&state.pool)
    .await?
    .ok_or_else(|| AppError::NotFound("Channel not found".to_string()))?;

    if is_archived.0 {
        return Err(AppError::Forbidden("Channel is archived".to_string()));
    }

    let _membership: (String,) = sqlx::query_as(
        "SELECT role FROM channel_members WHERE channel_id = ? AND user_id = ?",
    )
    .bind(&channel_id)
    .bind(&user_id)
    .fetch_optional(&state.pool)
    .await?
    .ok_or_else(|| AppError::Forbidden("You are not a member of this channel".to_string()))?;

    // ── _vote_request: create vote record, rewrite payload, fall through ──
    if body.payload.get("_vote_request").and_then(|v| v.as_bool()).unwrap_or(false) {
        let title = body
            .payload
            .get("title")
            .and_then(|v| v.as_str())
            .map(str::trim)
            .unwrap_or("");
        let opts_in = body.payload.get("options").and_then(|v| v.as_array());

        if title.is_empty() {
            return Err(AppError::BadRequest(
                "Usage: vote requires a non-empty title".to_string(),
            ));
        }
        let opts_arr = opts_in.ok_or_else(|| {
            AppError::BadRequest("Vote requires an options array".to_string())
        })?;
        if opts_arr.is_empty() {
            return Err(AppError::BadRequest(
                "Vote requires at least one option".to_string(),
            ));
        }

        let vote_id = Uuid::new_v4().to_string();
        let vote_options: Vec<serde_json::Value> = opts_arr
            .iter()
            .map(|opt| {
                let text = opt.as_str().unwrap_or("");
                serde_json::json!({
                    "id": Uuid::new_v4().to_string(),
                    "text": text,
                    "voter_ids": [],
                })
            })
            .collect();
        let options_json = serde_json::to_string(&vote_options).map_err(|e| {
            AppError::Internal(format!("Failed to serialize vote options: {e}"))
        })?;

        sqlx::query(
            "INSERT INTO votes (id, channel_id, creator_id, title, options) \
             VALUES (?, ?, ?, ?, ?)",
        )
        .bind(&vote_id)
        .bind(&channel_id)
        .bind(&user_id)
        .bind(title)
        .bind(&options_json)
        .execute(&state.pool)
        .await?;

        // Rewrite payload — the normal message flow below will INSERT
        // the row and broadcast NewMsg with this card payload.
        body.payload = serde_json::json!({
            "_vote": true,
            "vote_id": vote_id,
            "title": title,
        });
    }

    // ── Slash command handling ──────────────────────────────────────
    if body.payload.get("_command").and_then(|v| v.as_bool()).unwrap_or(false) {
        let cmd = body.payload.get("command").and_then(|v| v.as_str()).unwrap_or("");
        let args = body.payload.get("args").and_then(|v| v.as_str()).unwrap_or("");
        let role = _membership.0;

        // `/train` is special: it creates a train record + a real visible
        // `_train` message (NOT a `_command_result` echo). Short-circuit
        // before the regular command-result block below.
        if cmd == "train" {
            let title = args.trim();
            if title.is_empty() {
                return Err(AppError::BadRequest(
                    "Usage: /train <title>".to_string(),
                ));
            }
            let train_id = Uuid::new_v4().to_string();
            sqlx::query(
                "INSERT INTO trains (id, channel_id, creator_id, title, replies) \
                 VALUES (?, ?, ?, ?, '[]')",
            )
            .bind(&train_id)
            .bind(&channel_id)
            .bind(&user_id)
            .bind(title)
            .execute(&state.pool)
            .await?;

            let payload = serde_json::json!({
                "_train": true,
                "train_id": train_id,
                "title": title,
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
            .bind(&channel_id)
            .bind(&user_id)
            .bind(&payload_str)
            .fetch_one(&state.pool)
            .await?;

            state.ws_pool.notify_channel(
                &channel_id,
                ServerEvent::NewMsg {
                    channel_id: channel_id.clone(),
                    cursor: inserted.id,
                    sender_id: user_id.clone(),
                    msg_type: "text".to_string(),
                    preview: String::new(),
                },
            );

            let mut resp = MessageResponse::from(inserted);
            let (username, display_name, avatar_url, is_bot) =
                sqlx::query_as::<_, (String, String, String, bool)>(
                    "SELECT username, display_name, avatar_url, is_bot FROM users WHERE id = ?",
                )
                .bind(&user_id)
                .fetch_one(&state.pool)
                .await
                .unwrap_or_default();
            resp.sender_name = username;
            resp.sender_display_name = display_name;
            resp.sender_avatar_url = avatar_url;
            resp.is_bot = is_bot;
            return Ok((axum::http::StatusCode::CREATED, Json(resp)));
        }

        let result = match cmd {
            "quit" => {
                let is_dm: (bool,) = sqlx::query_as("SELECT is_direct FROM channels WHERE id = ?")
                    .bind(&channel_id).fetch_one(&state.pool).await?;
                if is_dm.0 {
                    Err(AppError::Forbidden("Cannot delete DM channels".into()))
                } else if role != "owner" {
                    Err(AppError::Forbidden("Only the channel owner can delete this channel".into()))
                } else {
                    sqlx::query("UPDATE channels SET is_archived = 1 WHERE id = ?")
                        .bind(&channel_id).execute(&state.pool).await?;
                    state.ws_pool.notify_channel(&channel_id, ServerEvent::ChannelArchived { channel_id: channel_id.clone() });
                    Ok(serde_json::json!({"_command_result": true, "text": "Channel has been archived."}))
                }
            }
            "list" => {
                if role != "owner" && role != "admin" {
                    Err(AppError::Forbidden("Only channel owner/admin can list members".into()))
                } else {
                    let members: Vec<(String, String)> = sqlx::query_as(
                        "SELECT u.username, cm.role FROM channel_members cm JOIN users u ON cm.user_id = u.id WHERE cm.channel_id = ? ORDER BY cm.role, u.username",
                    ).bind(&channel_id).fetch_all(&state.pool).await?;
                    let list: Vec<String> = members.iter().map(|(n, r)| format!("@{} [{}]", n, r)).collect();
                    Ok(serde_json::json!({"_command_result": true, "_owner_only": true, "text": list.join("\n")}))
                }
            }
            "kick" => {
                if role != "owner" && role != "admin" {
                    Err(AppError::Forbidden("Only channel owner/admin can kick members".into()))
                } else if args.trim().is_empty() {
                    Err(AppError::BadRequest("Usage: /kick <username>".into()))
                } else {
                    let target_row: Option<(String, String)> = sqlx::query_as(
                        "SELECT u.id, COALESCE(cm.role, '') FROM users u LEFT JOIN channel_members cm ON cm.user_id = u.id AND cm.channel_id = ? WHERE u.username = ?"
                    ).bind(&channel_id).bind(args.trim()).fetch_optional(&state.pool).await?;
                    match target_row {
                        None => Err(AppError::NotFound("User not found".into())),
                        Some((_tid, ref trole)) if trole == "owner" => Err(AppError::Forbidden("Cannot kick the channel owner".into())),
                        Some((tid, _)) if tid == user_id => Err(AppError::BadRequest("You cannot kick yourself".into())),
                        Some((_, ref trole)) if trole.is_empty() => Err(AppError::NotFound("User is not a member of this channel".into())),
                        Some((tid, _)) => {
                            sqlx::query("DELETE FROM channel_members WHERE channel_id = ? AND user_id = ?")
                                .bind(&channel_id).bind(&tid).execute(&state.pool).await?;
                            state.ws_pool.notify_channel(&channel_id, ServerEvent::MemberRemoved { channel_id: channel_id.clone(), user_id: tid.clone() });
                            Ok(serde_json::json!({"_command_result": true, "text": format!("Kicked @{}.", args.trim())}))
                        }
                    }
                }
            }
            _ => Err(AppError::BadRequest(format!("Unknown command: /{}. Try /quit, /list, or /kick <username>", cmd)))
        };

        let payload = result?;
        let payload_str = serde_json::to_string(&payload).unwrap_or_default();
        let msg_id = Uuid::new_v4().to_string();
        let inserted: MessageRow = sqlx::query_as::<_, MessageRow>(
            "INSERT INTO messages (msg_id, channel_id, sender_id, msg_type, payload) VALUES (?, ?, ?, 'text', ?) RETURNING id, msg_id, channel_id, sender_id, msg_type, payload, thread_parent_id, deleted_at, edited_at, created_at",
        )
        .bind(&msg_id).bind(&channel_id).bind(&user_id).bind(&payload_str)
        .fetch_one(&state.pool).await?;
        let mut resp = MessageResponse::from(inserted);
        let (username, display_name, avatar_url, is_bot) = sqlx::query_as::<_, (String, String, String, bool)>(
            "SELECT username, display_name, avatar_url, is_bot FROM users WHERE id = ?",
        )
        .bind(&user_id)
        .fetch_one(&state.pool)
        .await
        .unwrap_or_default();
        resp.sender_name = username;
        resp.sender_display_name = display_name;
        resp.sender_avatar_url = avatar_url;
        resp.is_bot = is_bot;
        return Ok((axum::http::StatusCode::CREATED, Json(resp)));
    }

    db::check_disk_space(&state.config.data_dir)
        .map_err(|e| AppError::Internal(format!("Disk space check failed: {e}")))?;

    let msg_id = Uuid::new_v4().to_string();
    let payload_str = serde_json::to_string(&body.payload)
        .map_err(|e| AppError::Internal(format!("Failed to serialize payload: {e}")))?;

    let inserted: MessageRow = sqlx::query_as::<_, MessageRow>(
        r#"
        INSERT INTO messages (msg_id, channel_id, sender_id, msg_type, payload, thread_parent_id)
        VALUES (?, ?, ?, ?, ?, ?)
        RETURNING id, msg_id, channel_id, sender_id, msg_type, payload,
                  thread_parent_id, deleted_at, edited_at, created_at
        "#,
    )
    .bind(&msg_id)
    .bind(&channel_id)
    .bind(&user_id)
    .bind(&body.msg_type)
    .bind(&payload_str)
    .bind(body.thread_parent_id)
    .fetch_one(&state.pool)
    .await?;

    let preview = extract_preview(&body.payload);
    if let Some(parent_id) = body.thread_parent_id {
        state.ws_pool.notify_channel(
            &channel_id,
            ServerEvent::ThreadReply {
                channel_id: channel_id.clone(),
                thread_parent_cursor: parent_id,
                cursor: inserted.id,
                sender_id: user_id.clone(),
                preview,
            },
        );
    } else {
        state.ws_pool.notify_channel(
            &channel_id,
            ServerEvent::NewMsg {
                channel_id: channel_id.clone(),
                cursor: inserted.id,
                sender_id: user_id.clone(),
                msg_type: body.msg_type.clone(),
                preview,
            },
        );
    }

    spawn_bot_mentions(&state, &channel_id, &body.payload).await;

    Ok((axum::http::StatusCode::CREATED, Json(inserted.into())))
}

/// GET /api/channels/{channel_id}/messages
///
/// Requires authentication. Returns paginated messages for a channel using
/// cursor-based pagination. Messages are ordered by ascending cursor id.
///
/// Query params:
///   - `after_cursor`: minimum message id (default 0)
///   - `limit`:        max results (default 50, max 100)
pub async fn get_messages(
    auth: AuthenticatedUser,
    State(state): State<Arc<AppState>>,
    Path(channel_id): Path<String>,
    Query(params): Query<ListMessagesParams>,
) -> Result<Json<ListMessagesResponse>, AppError> {
    let _user_id = auth.0;

    let limit = params.limit.clamp(1, 100);
    let after_cursor = params.after_cursor.max(0);

    let _ = sqlx::query_as::<_, (String,)>("SELECT id FROM channels WHERE id = ?")
        .bind(&channel_id)
        .fetch_optional(&state.pool)
        .await?
        .ok_or_else(|| AppError::NotFound("Channel not found".to_string()))?;

    let rows: Vec<MessageRow> = sqlx::query_as::<_, MessageRow>(
        r#"
        SELECT id, msg_id, channel_id, sender_id, msg_type, payload,
               thread_parent_id, deleted_at, edited_at, created_at
        FROM messages
        WHERE channel_id = ? AND id > ? AND deleted_at IS NULL
              AND thread_parent_id IS NULL
        ORDER BY id ASC
        LIMIT ?
        "#,
    )
    .bind(&channel_id)
    .bind(after_cursor)
    .bind(limit + 1)
    .fetch_all(&state.pool)
    .await?;

    let has_more = (rows.len() as i64) > limit;

    let sender_ids: Vec<String> = rows.iter().map(|r| r.sender_id.clone()).collect();
    let usernames: HashMap<String, (String, String, String, bool)> = if sender_ids.is_empty() {
        HashMap::new()
    } else {
        let placeholders: Vec<String> = sender_ids.iter().enumerate().map(|(i, _)| format!("?{}", i + 1)).collect();
        let query = format!(
            "SELECT id, username, display_name, avatar_url, is_bot FROM users WHERE id IN ({})",
            placeholders.join(",")
        );
        let mut q = sqlx::query_as::<_, (String, String, String, String, bool)>(&query);
        for id in &sender_ids {
            q = q.bind(id);
        }
        q.fetch_all(&state.pool).await.unwrap_or_default().into_iter().map(|(id, u, d, a, b)| (id, (u, d, a, b))).collect()
    };

    let messages: Vec<MessageResponse> = rows
        .into_iter()
        .take(limit as usize)
        .map(|r| {
            let mut msg = MessageResponse::from(r);
            if let Some((name, dname, avatar, is_bot)) = usernames.get(&msg.sender_id) {
                msg.sender_name = name.clone();
                msg.sender_display_name = dname.clone();
                msg.sender_avatar_url = avatar.clone();
                msg.is_bot = *is_bot;
            }
            msg
        })
        .collect();

    let next_cursor = messages
        .last()
        .map(|m| m.id)
        .unwrap_or(after_cursor);

    Ok(Json(ListMessagesResponse {
        messages,
        next_cursor,
        has_more,
    }))
}

/// GET /api/channels/{channel_id}/messages/{msg_id}/thread
///
/// Requires authentication. Returns paginated thread replies for a given
/// parent message. Replies are ordered by ascending cursor id.
///
/// Query params:
///   - `after_cursor`: minimum reply id (default 0)
///   - `limit`:        max results (default 50, max 100)
pub async fn get_thread(
    auth: AuthenticatedUser,
    State(state): State<Arc<AppState>>,
    Path((channel_id, msg_id)): Path<(String, i64)>,
    Query(params): Query<ListMessagesParams>,
) -> Result<Json<ListMessagesResponse>, AppError> {
    let _user_id = auth.0;

    let limit = params.limit.clamp(1, 100);
    let after_cursor = params.after_cursor.max(0);

    // Verify channel exists
    let _ = sqlx::query_as::<_, (String,)>("SELECT id FROM channels WHERE id = ?")
        .bind(&channel_id)
        .fetch_optional(&state.pool)
        .await?
        .ok_or_else(|| AppError::NotFound("Channel not found".to_string()))?;

    // Verify parent message exists and is not deleted
    let _ = sqlx::query_as::<_, (i64,)>(
        "SELECT id FROM messages WHERE id = ? AND channel_id = ? AND deleted_at IS NULL",
    )
    .bind(msg_id)
    .bind(&channel_id)
    .fetch_optional(&state.pool)
    .await?
    .ok_or_else(|| AppError::NotFound("Parent message not found".to_string()))?;

    let rows: Vec<MessageRow> = sqlx::query_as::<_, MessageRow>(
        r#"
        SELECT id, msg_id, channel_id, sender_id, msg_type, payload,
               thread_parent_id, deleted_at, edited_at, created_at
        FROM messages
        WHERE channel_id = ? AND thread_parent_id = ? AND id > ? AND deleted_at IS NULL
        ORDER BY id ASC
        LIMIT ?
        "#,
    )
    .bind(&channel_id)
    .bind(msg_id)
    .bind(after_cursor)
    .bind(limit + 1)
    .fetch_all(&state.pool)
    .await?;

    let has_more = (rows.len() as i64) > limit;

    let sender_ids: Vec<String> = rows.iter().map(|r| r.sender_id.clone()).collect();
    let senders: HashMap<String, (String, String, String, bool)> = if sender_ids.is_empty() {
        HashMap::new()
    } else {
        let placeholders: Vec<String> = sender_ids
            .iter()
            .enumerate()
            .map(|(i, _)| format!("?{}", i + 1))
            .collect();
        let query = format!(
            "SELECT id, username, display_name, avatar_url, is_bot FROM users WHERE id IN ({})",
            placeholders.join(",")
        );
        let mut q = sqlx::query_as::<_, (String, String, String, String, bool)>(&query);
        for id in &sender_ids {
            q = q.bind(id);
        }
        q.fetch_all(&state.pool)
            .await
            .unwrap_or_default()
            .into_iter()
            .map(|(id, u, d, a, b)| (id, (u, d, a, b)))
            .collect()
    };

    let messages: Vec<MessageResponse> = rows
        .into_iter()
        .take(limit as usize)
        .map(|r| {
            let mut msg = MessageResponse::from(r);
            if let Some((name, dname, avatar, is_bot)) = senders.get(&msg.sender_id) {
                msg.sender_name = name.clone();
                msg.sender_display_name = dname.clone();
                msg.sender_avatar_url = avatar.clone();
                msg.is_bot = *is_bot;
            }
            msg
        })
        .collect();

    let next_cursor = messages
        .last()
        .map(|m| m.id)
        .unwrap_or(after_cursor);

    Ok(Json(ListMessagesResponse {
        messages,
        next_cursor,
        has_more,
    }))
}

/// DELETE /api/messages/{message_id}
pub async fn delete_message(
    auth: AuthenticatedUser,
    State(state): State<Arc<AppState>>,
    Path(message_id): Path<i64>,
) -> Result<axum::http::StatusCode, AppError> {
    let user_id = auth.0;

    let (channel_id, sender_id, deleted_at): (String, String, Option<i64>) =
        sqlx::query_as(
            "SELECT channel_id, sender_id, deleted_at FROM messages WHERE id = ?",
        )
        .bind(message_id)
        .fetch_optional(&state.pool)
        .await?
        .ok_or_else(|| AppError::NotFound("Message not found".to_string()))?;

    if deleted_at.is_some() {
        return Err(AppError::NotFound("Message not found".to_string()));
    }

    if sender_id != user_id {
        return Err(AppError::Forbidden(
            "You can only delete your own messages".to_string(),
        ));
    }

    sqlx::query("UPDATE messages SET deleted_at = unixepoch() WHERE id = ?")
        .bind(message_id)
        .execute(&state.pool)
        .await?;

    state.ws_pool.notify_channel(
        &channel_id,
        ServerEvent::MsgDeleted {
            channel_id: channel_id.clone(),
            cursor: message_id,
        },
    );

    Ok(axum::http::StatusCode::NO_CONTENT)
}

// ---------------------------------------------------------------------------
// Bot mention bridge
// ---------------------------------------------------------------------------

/// Detect `@botname` mentions in a freshly-inserted message and trigger
/// each matching channel-member bot. Async-spawned so the request handler
/// never blocks on the Hermes round-trip.
async fn spawn_bot_mentions(
    state: &Arc<AppState>,
    channel_id: &str,
    payload: &serde_json::Value,
) {
    let content_text = payload
        .get("text")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_lowercase();
    if content_text.is_empty() {
        return;
    }

    let bots: Vec<BotRow> =
        sqlx::query_as(
            "SELECT b.id, b.user_id, b.name, b.display_name, b.api_url, b.api_key, b.system_prompt, b.model
             FROM bots b
             JOIN channel_members cm ON cm.user_id = b.user_id AND cm.channel_id = ?
             WHERE b.is_active = 1",
        )
        .bind(channel_id)
        .fetch_all(&state.pool)
        .await
        .unwrap_or_default();

    let pool = state.pool.clone();
    let ws_pool = state.ws_pool.clone();

    for (bot_id, bot_user_id, bot_name, bot_display_name, api_url, api_key, system_prompt, model) in bots {
        let mention_name = format!("@{}", bot_name.to_lowercase());
        let matches_name = content_text.contains(&mention_name);
        let matches_display = !bot_display_name.is_empty()
            && content_text.contains(&format!("@{}", bot_display_name.to_lowercase()));
        if !matches_name && !matches_display {
            continue;
        }

        // Atomic cooldown check-and-set BEFORE spawn to prevent
        // two simultaneous mentions of the same bot both passing.
        let mut cd = cooldowns().lock().await;
        let key = (bot_id.clone(), channel_id.to_string());
        if let Some(last) = cd.get(&key)
            && last.elapsed().as_secs() < BOT_COOLDOWN_SECS
        {
            continue;
        }
        cd.insert(key, std::time::Instant::now());
        drop(cd);

        let bot = BotConfig {
            id: bot_id,
            user_id: bot_user_id,
            name: bot_name,
            display_name: bot_display_name,
            api_url,
            api_key,
            system_prompt,
            model,
        };
        let pool_clone = pool.clone();
        let ws_pool_clone = ws_pool.clone();
        let channel_id_clone = channel_id.to_string();

        tokio::spawn(async move {
            trigger_bot_response(pool_clone, ws_pool_clone, bot, channel_id_clone, 0).await;
        });
    }
}

/// Gather channel history, call Hermes, post the bot's reply (or error).
/// Recursively triggers bots mentioned in the response, capped at
/// `BOT_MAX_CHAIN_DEPTH`.
///
/// Returns early at the depth cap; swallows all internal errors to keep
/// the spawned task from panicking the runtime.
async fn trigger_bot_response(
    pool: sqlx::SqlitePool,
    ws_pool: Arc<crate::ws::ConnectionPool>,
    bot: BotConfig,
    channel_id: String,
    depth: u32,
) {
    if depth >= BOT_MAX_CHAIN_DEPTH {
        return;
    }

    let rows: Vec<(String, String, String, bool)> = sqlx::query_as(
        "SELECT m.sender_id, m.payload, u.username, u.is_bot
         FROM messages m JOIN users u ON m.sender_id = u.id
         WHERE m.channel_id = ? AND m.deleted_at IS NULL
         ORDER BY m.created_at ASC",
    )
    .bind(&channel_id)
    .fetch_all(&pool)
    .await
    .unwrap_or_default();

    let messages: Vec<ChatMessage> = rows
        .into_iter()
        .map(|(_sender_id, payload, username, is_bot)| {
            let role = if is_bot { "assistant" } else { "user" };
            let text = serde_json::from_str::<serde_json::Value>(&payload)
                .ok()
                .and_then(|v| v.get("text").and_then(|t| t.as_str()).map(String::from))
                .unwrap_or_default();
            ChatMessage {
                role: role.to_string(),
                content: format!("{}: {}", username, text),
            }
        })
        .collect();

    let result = HermesClient::new()
        .chat(
            &bot.api_url,
            &bot.api_key,
            &bot.model,
            &bot.system_prompt,
            &channel_id,
            messages,
        )
        .await;

    match result {
        Ok(segments) => {
            for segment in segments {
                let Some(inserted_id) =
                    insert_bot_message(&pool, &channel_id, &bot.user_id, &segment).await
                else {
                    continue;
                };
                ws_pool.notify_channel(
                    &channel_id,
                    ServerEvent::NewMsg {
                        channel_id: channel_id.clone(),
                        cursor: inserted_id,
                        sender_id: bot.user_id.clone(),
                        msg_type: "text".to_string(),
                        preview: segment.chars().take(100).collect(),
                    },
                );
                trigger_chain_mentions(&pool, &ws_pool, &channel_id, &bot.id, &segment, depth)
                    .await;
            }
        }
        Err(_) => {
            let error_text = format!("⚠️ {} 暂时不可用", bot.name);
            if let Some(inserted_id) =
                insert_bot_message(&pool, &channel_id, &bot.user_id, &error_text).await
            {
                ws_pool.notify_channel(
                    &channel_id,
                    ServerEvent::NewMsg {
                        channel_id: channel_id.clone(),
                        cursor: inserted_id,
                        sender_id: bot.user_id.clone(),
                        msg_type: "text".to_string(),
                        preview: error_text,
                    },
                );
            }
        }
    }
}

/// Inspect a bot's reply segment for `@other_bot` mentions and trigger
/// any that pass the cooldown check, with `depth + 1`.
async fn trigger_chain_mentions(
    pool: &sqlx::SqlitePool,
    ws_pool: &Arc<crate::ws::ConnectionPool>,
    channel_id: &str,
    src_bot_id: &str,
    segment: &str,
    depth: u32,
) {
    if depth + 1 >= BOT_MAX_CHAIN_DEPTH {
        return;
    }
    let segment_lower = segment.to_lowercase();

    let other_bots: Vec<BotRow> =
        sqlx::query_as(
            "SELECT b.id, b.user_id, b.name, b.display_name, b.api_url, b.api_key, b.system_prompt, b.model
             FROM bots b
             JOIN channel_members cm ON cm.user_id = b.user_id AND cm.channel_id = ?
             WHERE b.is_active = 1 AND b.id != ?",
        )
        .bind(channel_id)
        .bind(src_bot_id)
        .fetch_all(pool)
        .await
        .unwrap_or_default();

    for (id, user_id, name, display_name, api_url, api_key, system_prompt, model) in other_bots {
        let mention_name = format!("@{}", name.to_lowercase());
        let matches_name = segment_lower.contains(&mention_name);
        let matches_display = !display_name.is_empty()
            && segment_lower.contains(&format!("@{}", display_name.to_lowercase()));
        if !matches_name && !matches_display {
            continue;
        }

        let mut cd = cooldowns().lock().await;
        let key = (id.clone(), channel_id.to_string());
        if let Some(last) = cd.get(&key)
            && last.elapsed().as_secs() < BOT_COOLDOWN_SECS
        {
            continue;
        }
        cd.insert(key, std::time::Instant::now());
        drop(cd);

        let other = BotConfig { id, user_id, name, display_name, api_url, api_key, system_prompt, model };
        Box::pin(trigger_bot_response(
            pool.clone(),
            ws_pool.clone(),
            other,
            channel_id.to_string(),
            depth + 1,
        ))
        .await;
    }
}

/// Insert a bot-sent message and return its autoincrement id (the cursor
/// used in `ServerEvent::NewMsg`). Returns `None` on DB error.
async fn insert_bot_message(
    pool: &sqlx::SqlitePool,
    channel_id: &str,
    bot_user_id: &str,
    text: &str,
) -> Option<i64> {
    let msg_id = Uuid::new_v4().to_string();
    let payload = serde_json::json!({"text": text}).to_string();
    sqlx::query_scalar::<_, i64>(
        "INSERT INTO messages (msg_id, channel_id, sender_id, msg_type, payload)
         VALUES (?, ?, ?, 'text', ?) RETURNING id",
    )
    .bind(&msg_id)
    .bind(channel_id)
    .bind(bot_user_id)
    .bind(&payload)
    .fetch_one(pool)
    .await
    .ok()
}

/// Hosts the raw-proxy is allowed to fetch from. SSRF defense: any host not
/// in this exact-match list is rejected with 403 before any network call.
/// Decision (plan Q1:A): strict hardcoded — NOT env-configurable.
const ALLOWED_RAW_HOSTS: &[&str] = &[
    "raw.githubusercontent.com",
    "gist.githubusercontent.com",
    "gitlab.com",
];

/// Parse and validate a raw-proxy URL.
///
/// - Parses with `reqwest::Url::parse` so userinfo tricks (`evil@host`) and
///   subdomain spoofs (`host.evil.com`) are normalized before the host check.
/// - Rejects non-http/https schemes.
/// - Rejects any host not in `ALLOWED_RAW_HOSTS` (exact equality only).
fn parse_raw_url(raw: &str) -> Result<reqwest::Url, AppError> {
    let url = reqwest::Url::parse(raw).map_err(|_| AppError::BadRequest("Invalid url".into()))?;
    let scheme = url.scheme();
    if scheme != "http" && scheme != "https" {
        return Err(AppError::BadRequest("Only http/https URLs allowed".into()));
    }
    let host = url.host_str().unwrap_or("");
    if !ALLOWED_RAW_HOSTS.contains(&host) {
        return Err(AppError::Forbidden("host not allowed".into()));
    }
    Ok(url)
}

/// GET /api/raw?url=<encoded_url>
/// Proxy for fetching raw file content to bypass CORS restrictions
pub async fn raw_proxy(
    Query(params): Query<std::collections::HashMap<String, String>>,
) -> Result<axum::response::Response, AppError> {
    let url = params.get("url").ok_or_else(|| AppError::BadRequest("Missing url parameter".into()))?;
    let url = urlencoding::decode(url).map_err(|_| AppError::BadRequest("Invalid url encoding".into()))?;
    let parsed = parse_raw_url(url.as_ref())?;

    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|e| AppError::Internal(format!("Failed to create HTTP client: {e}")))?;

    let response = client.get(parsed)
        .header("User-Agent", "VAST-IM-RawProxy/1.0")
        .send()
        .await
        .map_err(|e| AppError::BadRequest(format!("Failed to fetch URL: {e}")))?;

    if !response.status().is_success() {
        return Err(AppError::NotFound(format!("Remote server returned {}", response.status())));
    }

    let content_type = response.headers().get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("text/plain")
        .to_string();

    let body = response.text().await
        .map_err(|e| AppError::Internal(format!("Failed to read response: {e}")))?;

    Ok(axum::response::Response::builder()
        .header("content-type", content_type)
        .header("access-control-allow-origin", "*")
        .body(axum::body::Body::from(body))
        .unwrap())
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
        routing::get,
        Router,
    };
    use serde_json::{json, Value};
    use tower::ServiceExt;

    use crate::ws;
    use crate::ws::protocol::ServerEvent;

    /// Helper to build a test app with an in-memory database.
    /// Returns (Router, pool, user_id, token, ws_pool).
    async fn setup() -> (Router, sqlx::SqlitePool, String, String, Arc<ws::ConnectionPool>) {
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

    /// Helper to create a channel via API and return its id.
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

    /// Helper to send a message via API and return its id.
    async fn send_message(app: &mut Router, channel_id: &str, token: &str) -> Value {
        let resp = request(
            app,
            Method::POST,
            &format!("/channels/{}/messages", channel_id),
            Some(json!({"msg_type": "text", "payload": {"text": "hello"}})),
            token,
        )
        .await;
        assert_eq!(resp.status(), StatusCode::CREATED);
        serde_json::from_slice(
            &axum::body::to_bytes(resp.into_body(), usize::MAX)
                .await
                .unwrap(),
        )
        .unwrap()
    }

    // ── DELETE /messages/{message_id} ─────────────────────────────────

    #[tokio::test]
    async fn test_delete_message_success() {
        let (mut app, _, _, token, _ws_pool) = setup().await;
        let channel_id = create_channel(&mut app, &token).await;
        let msg = send_message(&mut app, &channel_id, &token).await;
        let msg_id = msg["id"].as_i64().unwrap();

        let resp = request(
            &mut app,
            Method::DELETE,
            &format!("/messages/{}", msg_id),
            None,
            &token,
        )
        .await;
        assert_eq!(resp.status(), StatusCode::NO_CONTENT);

        let list_resp = request(
            &mut app,
            Method::GET,
            &format!("/channels/{}/messages", channel_id),
            None,
            &token,
        )
        .await;
        assert_eq!(list_resp.status(), StatusCode::OK);
        let list_body: Value = serde_json::from_slice(
            &axum::body::to_bytes(list_resp.into_body(), usize::MAX)
                .await
                .unwrap(),
        )
        .unwrap();
        let messages = list_body["messages"].as_array().unwrap();
        assert!(
            messages.iter().all(|m| m["id"] != msg_id),
            "deleted message should not appear in list"
        );
    }

    #[tokio::test]
    async fn test_delete_message_not_sender() {
        let (mut app, pool, _user_id, token1, _ws_pool) = setup().await;
        let channel_id = create_channel(&mut app, &token1).await;
        let msg = send_message(&mut app, &channel_id, &token1).await;
        let msg_id = msg["id"].as_i64().unwrap();

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

        let resp = request(
            &mut app,
            Method::DELETE,
            &format!("/messages/{}", msg_id),
            None,
            &token2,
        )
        .await;
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn test_delete_message_not_found() {
        let (mut app, _, _, token, _) = setup().await;
        let resp = request(
            &mut app,
            Method::DELETE,
            "/messages/99999",
            None,
            &token,
        )
        .await;
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_delete_message_already_deleted() {
        let (mut app, _, _, token, _ws_pool) = setup().await;
        let channel_id = create_channel(&mut app, &token).await;
        let msg = send_message(&mut app, &channel_id, &token).await;
        let msg_id = msg["id"].as_i64().unwrap();

        let resp1 = request(
            &mut app,
            Method::DELETE,
            &format!("/messages/{}", msg_id),
            None,
            &token,
        )
        .await;
        assert_eq!(resp1.status(), StatusCode::NO_CONTENT);

        let resp2 = request(
            &mut app,
            Method::DELETE,
            &format!("/messages/{}", msg_id),
            None,
            &token,
        )
        .await;
        assert_eq!(resp2.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_delete_message_requires_auth() {
        let (mut app, _, _, token, _) = setup().await;
        let channel_id = create_channel(&mut app, &token).await;
        let msg = send_message(&mut app, &channel_id, &token).await;
        let msg_id = msg["id"].as_i64().unwrap();

        let resp = request(
            &mut app,
            Method::DELETE,
            &format!("/messages/{}", msg_id),
            None,
            "",
        )
        .await;
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn test_delete_message_ws_event() {
        let (mut app, _, _, token, ws_pool) = setup().await;
        let channel_id = create_channel(&mut app, &token).await;

        let mut rx = ws_pool.register("testuser", "test-conn");
        while rx.try_recv().is_ok() {}

        let msg = send_message(&mut app, &channel_id, &token).await;
        let msg_id = msg["id"].as_i64().unwrap();

        while rx.try_recv().is_ok() {}

        let resp = request(
            &mut app,
            Method::DELETE,
            &format!("/messages/{}", msg_id),
            None,
            &token,
        )
        .await;
        assert_eq!(resp.status(), StatusCode::NO_CONTENT);

        let event = rx
            .try_recv()
            .expect("expected MsgDeleted WS event");
        match event {
            ServerEvent::MsgDeleted {
                channel_id: ch,
                cursor,
            } => {
                assert_eq!(ch, channel_id);
                assert_eq!(cursor, msg_id);
            }
            other => panic!("expected MsgDeleted, got {other:?}"),
        }
    }

    // ── Thread replies ──────────────────────────────────────────────

    #[tokio::test]
    async fn test_send_thread_reply_ws_event() {
        let (mut app, _, _, token, ws_pool) = setup().await;
        let channel_id = create_channel(&mut app, &token).await;

        let parent = send_message(&mut app, &channel_id, &token).await;
        let parent_id = parent["id"].as_i64().unwrap();

        let mut rx = ws_pool.register("testuser", "test-conn");
        while rx.try_recv().is_ok() {}

        let resp = request(
            &mut app,
            Method::POST,
            &format!("/channels/{}/messages", channel_id),
            Some(json!({
                "msg_type": "text",
                "payload": {"text": "thread reply"},
                "thread_parent_id": parent_id,
            })),
            &token,
        )
        .await;
        assert_eq!(resp.status(), StatusCode::CREATED);
        let reply_body: Value = serde_json::from_slice(
            &axum::body::to_bytes(resp.into_body(), usize::MAX)
                .await
                .unwrap(),
        )
        .unwrap();
        let reply_id = reply_body["id"].as_i64().unwrap();

        let event = rx
            .try_recv()
            .expect("expected ThreadReply WS event");
        match event {
            ServerEvent::ThreadReply {
                channel_id: ch,
                thread_parent_cursor,
                cursor,
                preview,
                ..
            } => {
                assert_eq!(ch, channel_id);
                assert_eq!(thread_parent_cursor, parent_id);
                assert_eq!(cursor, reply_id);
                assert_eq!(preview, "thread reply");
            }
            other => panic!("expected ThreadReply, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn test_get_thread() {
        let (mut app, _, _, token, _ws_pool) = setup().await;
        let channel_id = create_channel(&mut app, &token).await;

        let parent = send_message(&mut app, &channel_id, &token).await;
        let parent_id = parent["id"].as_i64().unwrap();

        for text in &["first reply", "second reply"] {
            let resp = request(
                &mut app,
                Method::POST,
                &format!("/channels/{}/messages", channel_id),
                Some(json!({
                    "msg_type": "text",
                    "payload": {"text": text},
                    "thread_parent_id": parent_id,
                })),
                &token,
            )
            .await;
            assert_eq!(resp.status(), StatusCode::CREATED);
        }

        let resp = request(
            &mut app,
            Method::GET,
            &format!("/channels/{}/messages/{}/thread", channel_id, parent_id),
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
        let messages = body["messages"].as_array().unwrap();
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0]["payload"]["text"], "first reply");
        assert_eq!(messages[1]["payload"]["text"], "second reply");
        for msg in messages {
            assert_eq!(msg["thread_parent_id"], parent_id);
        }
    }

    #[tokio::test]
    async fn test_thread_replies_excluded_from_channel_listing() {
        let (mut app, _, _, token, _ws_pool) = setup().await;
        let channel_id = create_channel(&mut app, &token).await;

        let parent = send_message(&mut app, &channel_id, &token).await;
        let parent_id = parent["id"].as_i64().unwrap();

        let _ = request(
            &mut app,
            Method::POST,
            &format!("/channels/{}/messages", channel_id),
            Some(json!({
                "msg_type": "text",
                "payload": {"text": "thread reply"},
                "thread_parent_id": parent_id,
            })),
            &token,
        )
        .await;

        let resp = request(
            &mut app,
            Method::POST,
            &format!("/channels/{}/messages", channel_id),
            Some(json!({"msg_type": "text", "payload": {"text": "another msg"}})),
            &token,
        )
        .await;
        assert_eq!(resp.status(), StatusCode::CREATED);

        let list_resp = request(
            &mut app,
            Method::GET,
            &format!("/channels/{}/messages", channel_id),
            None,
            &token,
        )
        .await;
        assert_eq!(list_resp.status(), StatusCode::OK);
        let list_body: Value = serde_json::from_slice(
            &axum::body::to_bytes(list_resp.into_body(), usize::MAX)
                .await
                .unwrap(),
        )
        .unwrap();
        let messages = list_body["messages"].as_array().unwrap();
        for msg in messages {
            assert!(
                msg["thread_parent_id"].is_null(),
                "expected thread_parent_id to be null, got {:?}",
                msg["thread_parent_id"]
            );
        }
        assert_eq!(messages.len(), 2);
    }

    #[tokio::test]
    async fn test_get_thread_not_found() {
        let (mut app, _, _, token, _) = setup().await;
        let channel_id = create_channel(&mut app, &token).await;

        let resp = request(
            &mut app,
            Method::GET,
            &format!("/channels/{}/messages/99999/thread", channel_id),
            None,
            &token,
        )
        .await;
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    // ── raw_proxy SSRF allowlist ────────────────────────────────────

    fn raw_router() -> Router {
        Router::new().route("/raw", get(raw_proxy))
    }

    async fn raw_status(url: &str) -> StatusCode {
        let app = raw_router();
        let req = Request::builder()
            .method(Method::GET)
            .uri(format!("/raw?url={}", urlencoding::encode(url)))
            .body(Body::empty())
            .unwrap();
        app.oneshot(req).await.unwrap().status()
    }

    #[tokio::test]
    async fn test_raw_proxy_rejects_internal_metadata_ip() {
        // 169.254.169.254 is the AWS/Azure/GCP metadata endpoint — must never be proxied.
        let status = raw_status("http://169.254.169.254/latest/meta-data/iam/").await;
        assert_eq!(status, StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn test_raw_proxy_rejects_localhost() {
        let status = raw_status("http://localhost:6379/").await;
        assert_eq!(status, StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn test_raw_proxy_rejects_subdomain_spoof() {
        // Host is "raw.githubusercontent.com.evil.com" — exact match catches the spoof.
        let status = raw_status("https://raw.githubusercontent.com.evil.com/x.rs").await;
        assert_eq!(status, StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn test_parse_raw_url_allows_gist() {
        // Regression guard: gist.githubusercontent.com is in the allowlist.
        let url = parse_raw_url("https://gist.githubusercontent.com/u/g/raw/f.py").unwrap();
        assert_eq!(url.host_str(), Some("gist.githubusercontent.com"));
    }

    #[tokio::test]
    async fn test_parse_raw_url_uses_host_str_not_userinfo() {
        // Userinfo "evil@" must NOT influence the host check. reqwest::Url::parse
        // puts "evil" in userinfo and host_str() returns the real host, so this
        // URL is allowed by the host allowlist (the actual request goes to the
        // github host, ignoring the userinfo).
        let url = parse_raw_url("https://evil@raw.githubusercontent.com/x.rs").unwrap();
        assert_eq!(url.host_str(), Some("raw.githubusercontent.com"));
    }

    // ── get_thread sender hydration ─────────────────────────────────

    async fn insert_channel_member(pool: &sqlx::SqlitePool, channel_id: &str, user_id: &str, role: &str) {
        sqlx::query("INSERT INTO channel_members (channel_id, user_id, role) VALUES (?, ?, ?)")
            .bind(channel_id)
            .bind(user_id)
            .bind(role)
            .execute(pool)
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn test_get_thread_hydrates_sender_info() {
        let (mut app, pool, owner_id, owner_token, _) = setup().await;
        let channel_id = create_channel(&mut app, &owner_token).await;

        let user2_id = Uuid::new_v4().to_string();
        let pw = crate::auth::hash_password("pass").unwrap();
        sqlx::query("INSERT INTO users (id, username, password_hash) VALUES (?, ?, ?)")
            .bind(&user2_id)
            .bind("alice")
            .bind(&pw)
            .execute(&pool)
            .await
            .unwrap();
        // Populate the fields create_second_user skips — assertions depend on these.
        sqlx::query("UPDATE users SET display_name = ?, avatar_url = ? WHERE id = ?")
            .bind("Alice")
            .bind("http://example.com/a.png")
            .bind(&user2_id)
            .execute(&pool)
            .await
            .unwrap();

        insert_channel_member(&pool, &channel_id, &user2_id, "member").await;
        let user2_token = crate::auth::create_token_pair(&user2_id, "test-secret", 0)
            .unwrap()
            .access_token;

        let parent = send_message(&mut app, &channel_id, &owner_token).await;
        let parent_id = parent["id"].as_i64().unwrap();

        let resp = request(
            &mut app,
            Method::POST,
            &format!("/channels/{}/messages", channel_id),
            Some(json!({
                "msg_type": "text",
                "payload": {"text": "hi from alice"},
                "thread_parent_id": parent_id,
            })),
            &user2_token,
        )
        .await;
        assert_eq!(resp.status(), StatusCode::CREATED);

        let thread_resp = request(
            &mut app,
            Method::GET,
            &format!("/channels/{}/messages/{}/thread", channel_id, parent_id),
            None,
            &owner_token,
        )
        .await;
        assert_eq!(thread_resp.status(), StatusCode::OK);
        let body: Value = serde_json::from_slice(
            &axum::body::to_bytes(thread_resp.into_body(), usize::MAX)
                .await
                .unwrap(),
        )
        .unwrap();
        let messages = body["messages"].as_array().unwrap();
        assert_eq!(messages.len(), 1);
        let reply = &messages[0];
        assert_eq!(reply["sender_id"], user2_id);
        assert_eq!(reply["sender_name"], "alice");
        assert_eq!(reply["sender_display_name"], "Alice");
        assert_eq!(reply["sender_avatar_url"], "http://example.com/a.png");
        let _ = owner_id;
    }

    #[tokio::test]
    async fn test_get_thread_graceful_when_sender_missing() {
        // Simulate a thread reply whose sender row no longer exists. With FK
        // enforcement off on a single connection we can insert an orphan row,
        // and the hydration must fall back gracefully (no panic).
        let (mut app, pool, owner_id, owner_token, _) = setup().await;
        let channel_id = create_channel(&mut app, &owner_token).await;

        let parent = send_message(&mut app, &channel_id, &owner_token).await;
        let parent_id = parent["id"].as_i64().unwrap();

        let orphan_sender = "ghost-user-not-in-users-table";
        let mut conn = pool.acquire().await.unwrap();
        sqlx::query("PRAGMA foreign_keys=OFF")
            .execute(&mut *conn)
            .await
            .unwrap();
        sqlx::query(
            "INSERT INTO messages (msg_id, channel_id, sender_id, msg_type, payload, thread_parent_id) \
             VALUES (?, ?, ?, 'text', '{}', ?)",
        )
        .bind(Uuid::new_v4().to_string())
        .bind(&channel_id)
        .bind(orphan_sender)
        .bind(parent_id)
        .execute(&mut *conn)
        .await
        .unwrap();
        drop(conn);

        let resp = request(
            &mut app,
            Method::GET,
            &format!("/channels/{}/messages/{}/thread", channel_id, parent_id),
            None,
            &owner_token,
        )
        .await;
        assert_eq!(resp.status(), StatusCode::OK);
        let body: Value = serde_json::from_slice(
            &axum::body::to_bytes(resp.into_body(), usize::MAX)
                .await
                .unwrap(),
        )
        .unwrap();
        let messages = body["messages"].as_array().unwrap();
        assert_eq!(messages.len(), 1);
        // No user row → sender_name falls back to the raw sender_id, empty display/avatar.
        assert_eq!(messages[0]["sender_name"], orphan_sender);
        assert_eq!(messages[0]["sender_display_name"], "");
        let _ = owner_id;
    }

    // ── Bot mention bridge ──────────────────────────────────────────

    /// Insert a bot user + bots row + channel membership for tests.
    /// `api_url` defaults to an unreachable port so the Hermes call fails
    /// fast (exercising the error path).
    async fn create_test_bot(
        pool: &sqlx::SqlitePool,
        channel_id: &str,
        name: &str,
        api_url: &str,
    ) -> String {
        let user_id = Uuid::new_v4().to_string();
        let bot_id = Uuid::new_v4().to_string();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;

        sqlx::query(
            "INSERT INTO users (id, username, password_hash, is_bot) VALUES (?, ?, '', 1)",
        )
        .bind(&user_id)
        .bind(name)
        .execute(pool)
        .await
        .unwrap();

        sqlx::query(
            "INSERT INTO bots (id, user_id, name, display_name, api_url, api_key, \
             system_prompt, model, is_active, created_at) \
             VALUES (?, ?, ?, '', ?, '', '', 'hermes', 1, ?)",
        )
        .bind(&bot_id)
        .bind(&user_id)
        .bind(name)
        .bind(api_url)
        .bind(now)
        .execute(pool)
        .await
        .unwrap();

        sqlx::query("INSERT INTO channel_members (channel_id, user_id, role) VALUES (?, ?, 'member')")
            .bind(channel_id)
            .bind(&user_id)
            .execute(pool)
            .await
            .unwrap();

        bot_id
    }

    /// Resolve the bot user id from a bot_id.
    async fn bot_user_id(pool: &sqlx::SqlitePool, bot_id: &str) -> String {
        sqlx::query_scalar::<_, String>("SELECT user_id FROM bots WHERE id = ?")
            .bind(bot_id)
            .fetch_one(pool)
            .await
            .unwrap()
    }

    /// Poll the DB until the bot has posted a reply (or timeout).
    /// Returns the bot's message text, or `None` on timeout.
    async fn wait_for_bot_reply(
        pool: &sqlx::SqlitePool,
        channel_id: &str,
        bot_user_id: &str,
        timeout_ms: u64,
    ) -> Option<String> {
        let deadline = std::time::Instant::now() + std::time::Duration::from_millis(timeout_ms);
        while std::time::Instant::now() < deadline {
            let row: Option<(String,)> = sqlx::query_as(
                "SELECT m.payload FROM messages m \
                 WHERE m.channel_id = ? AND m.sender_id = ? AND m.deleted_at IS NULL \
                 ORDER BY m.id DESC LIMIT 1",
            )
            .bind(channel_id)
            .bind(bot_user_id)
            .fetch_optional(pool)
            .await
            .ok()?;
            if let Some((payload,)) = row {
                let v: serde_json::Value = serde_json::from_str(&payload).unwrap_or_default();
                if let Some(text) = v.get("text").and_then(|t| t.as_str()) {
                    return Some(text.to_string());
                }
            }
            tokio::time::sleep(std::time::Duration::from_millis(25)).await;
        }
        None
    }

    /// Given: a channel with an active bot whose `api_url` points at an
    ///        unreachable port (Hermes call fails fast).
    /// When:  a member posts `@botname hello`.
    /// Then:  the bot posts a "⚠️ ... 暂时不可用" reply.
    #[tokio::test]
    async fn test_mention_triggers_bot_error_reply() {
        let (mut app, pool, _, token, _) = setup().await;
        let channel_id = create_channel(&mut app, &token).await;
        let bot_id = create_test_bot(&pool, &channel_id, "hermes", "http://127.0.0.1:1").await;
        let bot_uid = bot_user_id(&pool, &bot_id).await;

        let resp = request(
            &mut app,
            Method::POST,
            &format!("/channels/{}/messages", channel_id),
            Some(json!({"msg_type": "text", "payload": {"text": "@hermes please reply"}})),
            &token,
        )
        .await;
        assert_eq!(resp.status(), StatusCode::CREATED);

        let reply = wait_for_bot_reply(&pool, &channel_id, &bot_uid, 5000)
            .await
            .expect("bot should have posted an error reply");
        assert!(reply.contains("暂时不可用"), "got: {reply}");
        assert!(reply.contains("hermes"), "got: {reply}");
    }

    /// Given: a channel with an active bot.
    /// When:  a member posts a message WITHOUT mentioning the bot.
    /// Then:  no bot reply appears within a short window.
    #[tokio::test]
    async fn test_no_mention_does_not_trigger_bot() {
        let (mut app, pool, _, token, _) = setup().await;
        let channel_id = create_channel(&mut app, &token).await;
        let bot_id = create_test_bot(&pool, &channel_id, "hermes", "http://127.0.0.1:1").await;
        let bot_uid = bot_user_id(&pool, &bot_id).await;

        let resp = request(
            &mut app,
            Method::POST,
            &format!("/channels/{}/messages", channel_id),
            Some(json!({"msg_type": "text", "payload": {"text": "just chatting, no mention"}})),
            &token,
        )
        .await;
        assert_eq!(resp.status(), StatusCode::CREATED);

        let reply = wait_for_bot_reply(&pool, &channel_id, &bot_uid, 500).await;
        assert!(reply.is_none(), "no bot reply expected, got: {reply:?}");
    }

    /// Given: a channel with an active bot.
    /// When:  the same user mentions the bot twice in rapid succession.
    /// Then:  only the first mention triggers a reply (10s cooldown).
    #[tokio::test]
    async fn test_cooldown_blocks_rapid_re_trigger() {
        let (mut app, pool, _, token, _) = setup().await;
        let channel_id = create_channel(&mut app, &token).await;
        let bot_id = create_test_bot(&pool, &channel_id, "hermes", "http://127.0.0.1:1").await;
        let bot_uid = bot_user_id(&pool, &bot_id).await;

        for i in 0..2 {
            let resp = request(
                &mut app,
                Method::POST,
                &format!("/channels/{}/messages", channel_id),
                Some(json!({"msg_type": "text", "payload": {"text": format!("@hermes attempt {}", i)}})),
                &token,
            )
            .await;
            assert_eq!(resp.status(), StatusCode::CREATED);
        }

        // Wait for the first reply to land.
        let first = wait_for_bot_reply(&pool, &channel_id, &bot_uid, 5000)
            .await
            .expect("first mention should trigger");
        assert!(first.contains("暂时不可用"));

        // Drain; then give the second mention a window to (not) produce a reply.
        // Count total bot messages — should stay at 1.
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM messages WHERE channel_id = ? AND sender_id = ? AND deleted_at IS NULL",
        )
        .bind(&channel_id)
        .bind(&bot_uid)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(count, 1, "cooldown should have suppressed the second trigger");
    }

    /// Given: trigger_bot_response is called with `depth >= BOT_MAX_CHAIN_DEPTH`.
    /// When:  invoked directly with a maxed-out depth.
    /// Then:  it returns immediately without inserting any bot message.
    #[tokio::test]
    async fn test_chain_depth_limit_short_circuits() {
        let pool = sqlx::SqlitePool::connect("sqlite::memory:").await.unwrap();
        sqlx::migrate!("db/migrations").run(&pool).await.unwrap();

        let bot_user_id = Uuid::new_v4().to_string();
        sqlx::query("INSERT INTO users (id, username, password_hash, is_bot) VALUES (?, 'b', '', 1)")
            .bind(&bot_user_id)
            .execute(&pool)
            .await
            .unwrap();
        let channel_id = Uuid::new_v4().to_string();
        sqlx::query("INSERT INTO channels (id, name, owner_id) VALUES (?, 'c', ?)")
            .bind(&channel_id)
            .bind(&bot_user_id)
            .execute(&pool)
            .await
            .unwrap();

        let ws_pool = Arc::new(crate::ws::ConnectionPool::new());
        let before: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM messages")
            .fetch_one(&pool)
            .await
            .unwrap();

        let bot = BotConfig {
            id: "bot-id".to_string(),
            user_id: bot_user_id.clone(),
            name: "name".to_string(),
            display_name: String::new(),
            api_url: "http://127.0.0.1:1".to_string(),
            api_key: String::new(),
            system_prompt: String::new(),
            model: "hermes".to_string(),
        };
        trigger_bot_response(pool.clone(), ws_pool, bot, channel_id, BOT_MAX_CHAIN_DEPTH).await;

        let after: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM messages")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(before, after, "depth-capped call must not insert anything");
    }

    /// Given: an inactive bot is a channel member.
    /// When:  a user @mentions it.
    /// Then:  no bot reply appears (the WHERE is_active = 1 filter excludes it).
    #[tokio::test]
    async fn test_inactive_bot_is_not_triggered() {
        let (mut app, pool, _, token, _) = setup().await;
        let channel_id = create_channel(&mut app, &token).await;
        let bot_id = create_test_bot(&pool, &channel_id, "hermes", "http://127.0.0.1:1").await;
        let bot_uid = bot_user_id(&pool, &bot_id).await;

        sqlx::query("UPDATE bots SET is_active = 0 WHERE id = ?")
            .bind(&bot_id)
            .execute(&pool)
            .await
            .unwrap();

        let resp = request(
            &mut app,
            Method::POST,
            &format!("/channels/{}/messages", channel_id),
            Some(json!({"msg_type": "text", "payload": {"text": "@hermes ping"}})),
            &token,
        )
        .await;
        assert_eq!(resp.status(), StatusCode::CREATED);

        let reply = wait_for_bot_reply(&pool, &channel_id, &bot_uid, 500).await;
        assert!(reply.is_none(), "inactive bot must not be triggered");
    }

    /// Given: a bot is NOT a channel member.
    /// When:  a user @mentions it.
    /// Then:  no bot reply appears (the channel_members JOIN excludes it).
    #[tokio::test]
    async fn test_non_member_bot_is_not_triggered() {
        let (mut app, pool, _, token, _) = setup().await;
        let channel_id = create_channel(&mut app, &token).await;
        // Create the bot but skip the channel_members insert.
        let user_id = Uuid::new_v4().to_string();
        sqlx::query("INSERT INTO users (id, username, password_hash, is_bot) VALUES (?, 'hermes', '', 1)")
            .bind(&user_id)
            .execute(&pool)
            .await
            .unwrap();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;
        sqlx::query(
            "INSERT INTO bots (id, user_id, name, api_url, model, is_active, created_at) \
             VALUES ('b1', ?, 'hermes', 'http://127.0.0.1:1', 'hermes', 1, ?)",
        )
        .bind(&user_id)
        .bind(now)
        .execute(&pool)
        .await
        .unwrap();

        let resp = request(
            &mut app,
            Method::POST,
            &format!("/channels/{}/messages", channel_id),
            Some(json!({"msg_type": "text", "payload": {"text": "@hermes outsider"}})),
            &token,
        )
        .await;
        assert_eq!(resp.status(), StatusCode::CREATED);

        let reply = wait_for_bot_reply(&pool, &channel_id, &user_id, 500).await;
        assert!(reply.is_none(), "non-member bot must not be triggered");
    }
}
