use axum::{
    extract::{Path, Query, State},
    Json,
};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use std::collections::HashMap;
use std::sync::Arc;
use uuid::Uuid;

use crate::auth::middleware::AuthenticatedUser;
use crate::db;
use crate::error::AppError;
use crate::ws::protocol::ServerEvent;
use crate::AppState;

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
                        Some((tid, ref trole)) if trole == "owner" => Err(AppError::Forbidden("Cannot kick the channel owner".into())),
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
                sender_id: user_id,
                preview,
            },
        );
    } else {
        state.ws_pool.notify_channel(
            &channel_id,
            ServerEvent::NewMsg {
                channel_id: channel_id.clone(),
                cursor: inserted.id,
                sender_id: user_id,
                msg_type: body.msg_type,
                preview,
            },
        );
    }

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
}
