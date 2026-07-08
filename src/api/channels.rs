use axum::{
    extract::{Path, State},
    http::{header, HeaderName, HeaderValue, StatusCode},
    Json,
};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use std::collections::HashMap;
use std::io::{Cursor, Write};
use std::sync::Arc;
use uuid::Uuid;
use zip::write::{SimpleFileOptions, ZipWriter};
use zip::CompressionMethod;

use crate::api::files::sanitize_filename_for_header;
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

#[derive(Debug, Deserialize)]
pub struct AddBotRequest {
    pub bot_id: String,
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

#[derive(Debug, Serialize, FromRow)]
pub struct PublicBot {
    pub id: String,
    pub name: String,
    pub display_name: String,
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

/// Maximum number of messages to include in a channel archive ZIP.
const MAX_ARCHIVE_MESSAGES: i64 = 100_000;

/// GET /api/channels/{id}/archive/download
///
/// Download a ZIP archive containing all channel messages and files.
/// Only the channel owner can download. DM channels are rejected.
/// The ZIP is generated on-the-fly and not persisted.
pub async fn download_channel_archive(
    State(state): State<Arc<AppState>>,
    user: AuthenticatedUser,
    Path(id): Path<String>,
) -> Result<([(HeaderName, HeaderValue); 2], Vec<u8>), AppError> {
    // ---- access control ----
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
            "Only the channel owner can download the archive".to_string(),
        ));
    }

    // ---- fetch all messages with sender info ----
    let message_rows = sqlx::query_as::<_, ArchiveMessageRow>(
        "SELECT m.id, m.msg_id, m.channel_id, m.sender_id, m.msg_type, m.payload, \
         m.thread_parent_id, m.deleted_at, m.edited_at, m.created_at, \
         COALESCE(u.username, m.sender_id) AS sender_name, \
         COALESCE(u.display_name, '') AS sender_display_name, \
         COALESCE(u.avatar_url, '') AS sender_avatar_url, \
         COALESCE(u.is_bot, 0) AS is_bot \
         FROM messages m \
         LEFT JOIN users u ON m.sender_id = u.id \
         WHERE m.channel_id = ? \
         ORDER BY m.id ASC LIMIT ?",
    )
    .bind(&id)
    .bind(MAX_ARCHIVE_MESSAGES)
    .fetch_all(&state.pool)
    .await?;

    let message_count = message_rows.len();
    if message_count as i64 == MAX_ARCHIVE_MESSAGES {
        tracing::warn!(
            channel_id = %id,
            limit = MAX_ARCHIVE_MESSAGES,
            "Archive message count hit limit — archive may be truncated"
        );
    }

    // ---- fetch non-deleted files ----
    let file_rows = sqlx::query_as::<_, ArchiveFileRow>(
        "SELECT id, uploader_id, channel_id, original_name, size, mime_type, extension, created_at \
         FROM files WHERE channel_id = ? AND is_deleted = 0 ORDER BY created_at ASC",
    )
    .bind(&id)
    .fetch_all(&state.pool)
    .await?;

    // ---- build serializable structures ----
    let messages: Vec<ArchiveMessage> = message_rows
        .into_iter()
        .map(|r| {
            let payload: serde_json::Value =
                serde_json::from_str(&r.payload).unwrap_or_else(|e| {
                    tracing::warn!(msg_id = %r.msg_id, error = %e, "Invalid message payload JSON in archive");
                    serde_json::json!({"__corrupt": true, "__raw": &r.payload})
                });
            ArchiveMessage {
                msg_id: r.msg_id,
                sender_id: r.sender_id,
                sender_name: r.sender_name,
                sender_display_name: r.sender_display_name,
                sender_avatar_url: r.sender_avatar_url,
                is_bot: r.is_bot,
                msg_type: r.msg_type,
                payload,
                thread_parent_id: r.thread_parent_id,
                deleted_at: r.deleted_at,
                edited_at: r.edited_at,
                created_at: r.created_at,
            }
        })
        .collect();

    let mut info = ArchiveChannelInfo {
        channel_id: channel.id.clone(),
        name: channel.name.clone(),
        description: channel.description.clone(),
        owner_id: channel.owner_id.clone(),
        created_at: channel.created_at,
        message_count,
        file_count: file_rows.len(), // may be updated below
        exported_at: chrono::Utc::now().timestamp(),
    };

    // ---- pre-read files in async context (for spawn_blocking later) ----
    let upload_dir = state.config.data_dir.join("uploads");
    let mut name_counts: HashMap<String, usize> = HashMap::new();
    let mut file_entries: Vec<(String, Vec<u8>)> = Vec::new();
    let mut actual_file_count: usize = 0;
    let mut skipped_count: usize = 0;

    for f in &file_rows {
        // H4: validate file record fields before constructing disk path
        if f.id.contains('/') || f.id.contains('\\') || f.id.contains("..")
            || f.extension.contains('/') || f.extension.contains('\\') || f.extension.contains("..")
        {
            tracing::warn!(file_id = %f.id, extension = %f.extension, "Suspicious file record — skipping from archive");
            skipped_count += 1;
            continue;
        }
        let file_path = upload_dir.join(format!("{}.{}", f.id, f.extension));
        match tokio::fs::read(&file_path).await {
            Ok(data) => {
                // C1: strip directory components from original_name to prevent Zip Slip
                let base_name = std::path::Path::new(&f.original_name)
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("unnamed");
                let safe_name = dedup_filename(base_name, &mut name_counts);
                let zip_path = format!("files/{safe_name}");
                file_entries.push((zip_path, data));
                actual_file_count += 1;
            }
            Err(e) => {
                tracing::warn!(file_id = %f.id, error = %e, "File missing on disk, skipping from archive");
                skipped_count += 1;
            }
        }
    }

    // M1: update file_count to reflect actual files included
    info.file_count = actual_file_count;

    if skipped_count > 0 {
        tracing::warn!(
            actual = actual_file_count,
            skipped = skipped_count,
            "Some files skipped from archive"
        );
    }

    // ---- serialize JSON in async context ----
    let messages_json = serde_json::to_vec_pretty(&messages)
        .map_err(|e| AppError::Internal(format!("Failed to serialize messages: {e}")))?;
    let info_json = serde_json::to_vec_pretty(&info)
        .map_err(|e| AppError::Internal(format!("Failed to serialize channel info: {e}")))?;

    // ---- build ZIP archive in spawn_blocking (M6) ----
    let zip_bytes = tokio::task::spawn_blocking(move || -> Result<Vec<u8>, AppError> {
        let mut cursor = Cursor::new(Vec::new());
        let mut archive = ZipWriter::new(&mut cursor);
        let options = SimpleFileOptions::default()
            .compression_method(CompressionMethod::Deflated);

        // messages.json
        archive
            .start_file("messages.json", options)
            .map_err(|e| AppError::Internal(format!("ZIP error: {e}")))?;
        archive
            .write_all(&messages_json)
            .map_err(|e| AppError::Internal(format!("ZIP write error: {e}")))?;

        // channel_info.json
        archive
            .start_file("channel_info.json", options)
            .map_err(|e| AppError::Internal(format!("ZIP error: {e}")))?;
        archive
            .write_all(&info_json)
            .map_err(|e| AppError::Internal(format!("ZIP write error: {e}")))?;

        // files/
        for (zip_path, data) in &file_entries {
            if let Err(e) = archive.start_file(zip_path.as_str(), options) {
                tracing::warn!(zip_path = %zip_path, error = %e, "Failed to add file entry to ZIP");
                continue;
            }
            if let Err(e) = archive.write_all(data) {
                tracing::warn!(zip_path = %zip_path, error = %e, "Failed to write file data to ZIP");
                continue;
            }
        }

        let _ = archive
            .finish()
            .map_err(|e| AppError::Internal(format!("ZIP error: {e}")))?;
        Ok(cursor.into_inner())
    })
    .await
    .map_err(|e| AppError::Internal(format!("ZIP construction panicked: {e}")))??;

    // ---- response headers (H1: RFC 5987 Unicode-safe) ----
    let sanitized_name = sanitize_channel_name(&channel.name);
    let archive_filename = format!("{sanitized_name}-archive.zip");
    let (ascii_name, utf8_encoded) = sanitize_filename_for_header(&archive_filename);
    let content_disposition = if ascii_name == utf8_encoded {
        format!("attachment; filename=\"{}\"", ascii_name)
    } else {
        format!(
            "attachment; filename=\"{}\"; filename*=UTF-8''{}",
            ascii_name, utf8_encoded
        )
    };
    let disposition_value = HeaderValue::try_from(&content_disposition)
        .map_err(|e| AppError::Internal(format!("Invalid header value: {e}")))?;

    Ok((
        [
            (header::CONTENT_TYPE, HeaderValue::from_static("application/zip")),
            (header::CONTENT_DISPOSITION, disposition_value),
        ],
        zip_bytes,
    ))
}

// ---- private helpers ----

/// Sanitize a channel name for use in a filename by replacing
/// characters that are not allowed in filenames with underscores.
fn sanitize_channel_name(name: &str) -> String {
    name.chars()
        .map(|c| match c {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
            _ => c,
        })
        .collect()
}

/// Deduplicate filenames for the ZIP archive.
/// Returns the original name on first use, and `{stem} ({n}).{ext}` on collisions.
fn dedup_filename(original_name: &str, counts: &mut HashMap<String, usize>) -> String {
    let count = counts.entry(original_name.to_string()).or_insert(0);
    if *count == 0 {
        *count += 1;
        return original_name.to_string();
    }
    *count += 1;
    let n = *count - 1;
    if let Some(dot_pos) = original_name.rfind('.') {
        let stem = &original_name[..dot_pos];
        let ext = &original_name[dot_pos..];
        format!("{stem} ({n}){ext}")
    } else {
        format!("{original_name} ({n})")
    }
}

// ---- private FromRow structs ----

#[derive(Debug, FromRow)]
#[allow(dead_code)]
struct ArchiveMessageRow {
    id: i64,
    msg_id: String,
    channel_id: String,
    sender_id: String,
    msg_type: String,
    payload: String,
    thread_parent_id: Option<i64>,
    deleted_at: Option<i64>,
    edited_at: Option<i64>,
    created_at: i64,
    sender_name: String,
    sender_display_name: String,
    sender_avatar_url: String,
    is_bot: bool,
}

#[derive(Debug, FromRow)]
#[allow(dead_code)]
struct ArchiveFileRow {
    id: String,
    uploader_id: Option<String>,
    channel_id: Option<String>,
    original_name: String,
    size: i64,
    mime_type: String,
    extension: String,
    created_at: i64,
}

// ---- serialization structs ----

#[derive(Serialize)]
struct ArchiveMessage {
    msg_id: String,
    sender_id: String,
    sender_name: String,
    sender_display_name: String,
    sender_avatar_url: String,
    is_bot: bool,
    msg_type: String,
    payload: serde_json::Value,
    thread_parent_id: Option<i64>,
    deleted_at: Option<i64>,
    edited_at: Option<i64>,
    created_at: i64,
}

#[derive(Serialize)]
struct ArchiveChannelInfo {
    channel_id: String,
    name: String,
    description: String,
    owner_id: String,
    created_at: i64,
    message_count: usize,
    file_count: usize,
    exported_at: i64,
}

/// GET /api/bots
///
/// List active bots (id, name, display_name) for any logged-in user.
/// No secrets (api_key, api_url, system_prompt, model) are exposed.
pub async fn list_public_bots(
    State(state): State<Arc<AppState>>,
    _user: AuthenticatedUser,
) -> Result<Json<Vec<PublicBot>>, AppError> {
    let bots = sqlx::query_as::<_, PublicBot>(
        "SELECT id, name, display_name FROM bots WHERE is_active = 1 ORDER BY name ASC",
    )
    .fetch_all(&state.pool)
    .await?;

    Ok(Json(bots))
}

/// POST /api/channels/{id}/bots
///
/// Add a bot to a channel. Requires owner or admin role.
/// Verifies bot exists and is active, checks for duplicates, inserts into channel_members,
/// and broadcasts MemberAdded via WebSocket.
pub async fn add_bot_to_channel(
    State(state): State<Arc<AppState>>,
    user: AuthenticatedUser,
    Path(channel_id): Path<String>,
    Json(req): Json<AddBotRequest>,
) -> Result<Json<serde_json::Value>, AppError> {
    crate::auth::middleware::require_role(&state.pool, &user.0, &channel_id, &["owner", "admin"]).await?;

    let bot_info: Option<(String, String, bool)> = sqlx::query_as(
        "SELECT user_id, name, is_active FROM bots WHERE id = ?",
    )
    .bind(&req.bot_id)
    .fetch_optional(&state.pool)
    .await?;

    let (bot_user_id, bot_name, is_active) = bot_info.ok_or_else(|| {
        AppError::NotFound("Bot not found".to_string())
    })?;

    if !is_active {
        return Err(AppError::BadRequest(
            "Bot is not active".to_string(),
        ));
    }

    let is_member: Option<i64> = sqlx::query_scalar(
        "SELECT 1 FROM channel_members WHERE channel_id = ? AND user_id = ?",
    )
    .bind(&channel_id)
    .bind(&bot_user_id)
    .fetch_optional(&state.pool)
    .await?;

    if is_member.is_some() {
        return Err(AppError::Conflict("Bot is already a member of this channel".to_string()));
    }

    let now = chrono::Utc::now().timestamp();
    sqlx::query(
        "INSERT INTO channel_members (channel_id, user_id, role, joined_at) VALUES (?, ?, 'member', ?)",
    )
    .bind(&channel_id)
    .bind(&bot_user_id)
    .bind(now)
    .execute(&state.pool)
    .await?;

    state.ws_pool.notify_channel(
        &channel_id,
        crate::ws::protocol::ServerEvent::MemberAdded {
            channel_id: channel_id.clone(),
            user_id: bot_user_id.clone(),
            username: bot_name.clone(),
        },
    );

    Ok(Json(serde_json::json!({ "ok": true })))
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

    // ── Add Bot to Channel ───────────────────────────────────────────

    #[tokio::test]
    async fn test_add_bot_to_channel_success() {
        let (mut app, pool, _owner_id, token) = setup().await;

        let create_resp = request(
            &mut app,
            Method::POST,
            "/channels",
            Some(json!({"name": "Bot Test Channel"})),
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

        let bot_user_id = Uuid::new_v4().to_string();
        let password_hash = crate::auth::hash_password("botpass").expect("Failed to hash password");
        sqlx::query("INSERT INTO users (id, username, password_hash, is_bot) VALUES (?, ?, ?, 1)")
            .bind(&bot_user_id)
            .bind("testbot")
            .bind(&password_hash)
            .execute(&pool)
            .await
            .expect("Failed to insert bot user");

        let bot_id = Uuid::new_v4().to_string();
        let now = chrono::Utc::now().timestamp();
        sqlx::query("INSERT INTO bots (id, user_id, name, display_name, api_url, is_active, created_at) VALUES (?, ?, ?, '', ?, 1, ?)")
            .bind(&bot_id)
            .bind(&bot_user_id)
            .bind("TestBot")
            .bind("https://example.com/bot")
            .bind(now)
            .execute(&pool)
            .await
            .expect("Failed to insert bot");

        let resp = request(
            &mut app,
            Method::POST,
            &format!("/channels/{}/bots", channel_id),
            Some(json!({"bot_id": &bot_id})),
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
        assert_eq!(body["ok"], true);

        let member_role: Option<String> = sqlx::query_scalar(
            "SELECT role FROM channel_members WHERE channel_id = ? AND user_id = ?",
        )
        .bind(&channel_id)
        .bind(&bot_user_id)
        .fetch_optional(&pool)
        .await
        .expect("Failed to query channel_members");
        assert_eq!(member_role, Some("member".to_string()));
    }

    #[tokio::test]
    async fn test_add_bot_non_owner_forbidden() {
        let (mut app, pool, _owner_id, owner_token) = setup().await;

        let create_resp = request(
            &mut app,
            Method::POST,
            "/channels",
            Some(json!({"name": "Non-Owner Channel"})),
            &owner_token,
        )
        .await;
        let create_body: Value = serde_json::from_slice(
            &axum::body::to_bytes(create_resp.into_body(), usize::MAX)
                .await
                .unwrap(),
        )
        .unwrap();
        let channel_id = create_body["id"].as_str().unwrap().to_string();

        let user_id = Uuid::new_v4().to_string();
        let password_hash = crate::auth::hash_password("userpass").expect("Failed to hash password");
        sqlx::query("INSERT INTO users (id, username, password_hash) VALUES (?, ?, ?)")
            .bind(&user_id)
            .bind("regularuser")
            .bind(&password_hash)
            .execute(&pool)
            .await
            .expect("Failed to insert user");

        let bot_user_id = Uuid::new_v4().to_string();
        let bot_password_hash = crate::auth::hash_password("botpass").expect("Failed to hash password");
        sqlx::query("INSERT INTO users (id, username, password_hash, is_bot) VALUES (?, ?, ?, 1)")
            .bind(&bot_user_id)
            .bind("testbot2")
            .bind(&bot_password_hash)
            .execute(&pool)
            .await
            .expect("Failed to insert bot user");

        let bot_id = Uuid::new_v4().to_string();
        let now = chrono::Utc::now().timestamp();
        sqlx::query("INSERT INTO bots (id, user_id, name, display_name, api_url, is_active, created_at) VALUES (?, ?, ?, '', ?, 1, ?)")
            .bind(&bot_id)
            .bind(&bot_user_id)
            .bind("TestBot2")
            .bind("https://example.com/bot2")
            .bind(now)
            .execute(&pool)
            .await
            .expect("Failed to insert bot");

        sqlx::query("INSERT INTO channel_members (channel_id, user_id, role) VALUES (?, ?, 'member')")
            .bind(&channel_id)
            .bind(&user_id)
            .execute(&pool)
            .await
            .expect("Failed to add user to channel");

        let secret = "test-secret";
        let user_token = crate::auth::create_token_pair(&user_id, secret, 0)
            .expect("Failed to create token")
            .access_token;

        let resp = request(
            &mut app,
            Method::POST,
            &format!("/channels/{}/bots", channel_id),
            Some(json!({"bot_id": &bot_id})),
            &user_token,
        )
        .await;
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn test_add_bot_already_member_conflict() {
        let (mut app, pool, _owner_id, token) = setup().await;

        let create_resp = request(
            &mut app,
            Method::POST,
            "/channels",
            Some(json!({"name": "Duplicate Bot Channel"})),
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

        let bot_user_id = Uuid::new_v4().to_string();
        let password_hash = crate::auth::hash_password("botpass").expect("Failed to hash password");
        sqlx::query("INSERT INTO users (id, username, password_hash, is_bot) VALUES (?, ?, ?, 1)")
            .bind(&bot_user_id)
            .bind("testbot3")
            .bind(&password_hash)
            .execute(&pool)
            .await
            .expect("Failed to insert bot user");

        let bot_id = Uuid::new_v4().to_string();
        let now = chrono::Utc::now().timestamp();
        sqlx::query("INSERT INTO bots (id, user_id, name, display_name, api_url, is_active, created_at) VALUES (?, ?, ?, '', ?, 1, ?)")
            .bind(&bot_id)
            .bind(&bot_user_id)
            .bind("TestBot3")
            .bind("https://example.com/bot3")
            .bind(now)
            .execute(&pool)
            .await
            .expect("Failed to insert bot");

        sqlx::query("INSERT INTO channel_members (channel_id, user_id, role) VALUES (?, ?, 'member')")
            .bind(&channel_id)
            .bind(&bot_user_id)
            .execute(&pool)
            .await
            .expect("Failed to add bot to channel");

        let resp = request(
            &mut app,
            Method::POST,
            &format!("/channels/{}/bots", channel_id),
            Some(json!({"bot_id": &bot_id})),
            &token,
        )
        .await;
        assert_eq!(resp.status(), StatusCode::CONFLICT);
    }

    #[tokio::test]
    async fn test_add_bot_not_found() {
        let (mut app, _, _, token) = setup().await;

        let create_resp = request(
            &mut app,
            Method::POST,
            "/channels",
            Some(json!({"name": "Non-Existent Bot Channel"})),
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

        let fake_bot_id = Uuid::new_v4().to_string();
        let resp = request(
            &mut app,
            Method::POST,
            &format!("/channels/{}/bots", channel_id),
            Some(json!({"bot_id": fake_bot_id})),
            &token,
        )
        .await;
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_add_bot_inactive_bad_request() {
        let (mut app, pool, _owner_id, token) = setup().await;

        let create_resp = request(
            &mut app,
            Method::POST,
            "/channels",
            Some(json!({"name": "Inactive Bot Channel"})),
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

        let bot_user_id = Uuid::new_v4().to_string();
        let password_hash = crate::auth::hash_password("botpass").expect("Failed to hash password");
        sqlx::query("INSERT INTO users (id, username, password_hash, is_bot) VALUES (?, ?, ?, 1)")
            .bind(&bot_user_id)
            .bind("testbot4")
            .bind(&password_hash)
            .execute(&pool)
            .await
            .expect("Failed to insert bot user");

        let bot_id = Uuid::new_v4().to_string();
        let now = chrono::Utc::now().timestamp();
        sqlx::query("INSERT INTO bots (id, user_id, name, display_name, api_url, is_active, created_at) VALUES (?, ?, ?, '', ?, 0, ?)")
            .bind(&bot_id)
            .bind(&bot_user_id)
            .bind("TestBot4")
            .bind("https://example.com/bot4")
            .bind(now)
            .execute(&pool)
            .await
            .expect("Failed to insert bot");

        let resp = request(
            &mut app,
            Method::POST,
            &format!("/channels/{}/bots", channel_id),
            Some(json!({"bot_id": &bot_id})),
            &token,
        )
        .await;
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    // ── Public bot list ───────────────────────────────────────────────

    async fn insert_bot(
        pool: &sqlx::SqlitePool,
        name: &str,
        display_name: &str,
        is_active: bool,
    ) -> String {
        let bot_user_id = Uuid::new_v4().to_string();
        let password_hash = crate::auth::hash_password("botpass").expect("hash");
        sqlx::query("INSERT INTO users (id, username, password_hash, is_bot) VALUES (?, ?, ?, 1)")
            .bind(&bot_user_id)
            .bind(format!("botuser_{}", name))
            .bind(&password_hash)
            .execute(pool)
            .await
            .expect("insert bot user");

        let bot_id = Uuid::new_v4().to_string();
        let now = chrono::Utc::now().timestamp();
        sqlx::query("INSERT INTO bots (id, user_id, name, display_name, api_url, api_key, system_prompt, model, is_active, created_at) VALUES (?, ?, ?, ?, 'https://example.com', 'secret-key', 'private-prompt', 'hermes', ?, ?)")
            .bind(&bot_id)
            .bind(&bot_user_id)
            .bind(name)
            .bind(display_name)
            .bind(if is_active { 1 } else { 0 })
            .bind(now)
            .execute(pool)
            .await
            .expect("insert bot");
        bot_id
    }

    #[tokio::test]
    async fn test_list_public_bots_returns_only_active_and_no_secrets() {
        let (mut app, pool, _, token) = setup().await;

        insert_bot(&pool, "alpha", "Alpha Bot", true).await;
        insert_bot(&pool, "beta", "Beta Bot", true).await;
        insert_bot(&pool, "gamma", "Gamma Bot", false).await;

        let resp = request(&mut app, Method::GET, "/bots", None, &token).await;
        assert_eq!(resp.status(), StatusCode::OK);

        let body: Value = serde_json::from_slice(
            &axum::body::to_bytes(resp.into_body(), usize::MAX)
                .await
                .unwrap(),
        )
        .unwrap();
        let arr = body.as_array().unwrap();
        assert_eq!(arr.len(), 2, "inactive bot must be excluded");

        let names: Vec<&str> = arr.iter().map(|b| b["name"].as_str().unwrap()).collect();
        assert_eq!(names, vec!["alpha", "beta"], "ordered by name ascending");

        for b in arr {
            let obj = b.as_object().unwrap();
            assert!(obj.contains_key("id"));
            assert!(obj.contains_key("name"));
            assert!(obj.contains_key("display_name"));
            assert!(
                !obj.contains_key("api_key"),
                "api_key must not be exposed publicly",
            );
            assert!(
                !obj.contains_key("api_url"),
                "api_url must not be exposed publicly",
            );
            assert!(
                !obj.contains_key("system_prompt"),
                "system_prompt must not be exposed publicly",
            );
            assert!(!obj.contains_key("model"));
            assert!(!obj.contains_key("user_id"));
        }
    }

    #[tokio::test]
    async fn test_list_public_bots_requires_auth() {
        let (mut app, _, _, _) = setup().await;

        let resp = request(&mut app, Method::GET, "/bots", None, "").await;
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn test_list_public_bots_empty_when_none() {
        let (mut app, _, _, token) = setup().await;

        let resp = request(&mut app, Method::GET, "/bots", None, &token).await;
        assert_eq!(resp.status(), StatusCode::OK);

        let body: Value = serde_json::from_slice(
            &axum::body::to_bytes(resp.into_body(), usize::MAX)
                .await
                .unwrap(),
        )
        .unwrap();
        assert_eq!(body.as_array().unwrap().len(), 0);
    }

    // ── Archive Download ────────────────────────────────────────────────

    /// Helper: make a GET request and return status, headers, and raw body.
    async fn download(
        app: &mut Router,
        channel_id: &str,
        token: &str,
    ) -> (StatusCode, axum::http::HeaderMap, Vec<u8>) {
        let req = Request::builder()
            .method(Method::GET)
            .uri(format!("/channels/{channel_id}/archive/download"))
            .header(header::AUTHORIZATION, format!("Bearer {token}"))
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        let status = resp.status();
        let headers = resp.headers().clone();
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        (status, headers, body.to_vec())
    }

    /// Helper: create a channel via the API and return its ID.
    async fn create_test_channel(app: &mut Router, name: &str, token: &str) -> String {
        let resp = request(
            app,
            Method::POST,
            "/channels",
            Some(json!({"name": name})),
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

    /// Helper: insert a message directly into the DB.
    async fn insert_message(
        pool: &sqlx::SqlitePool,
        channel_id: &str,
        sender_id: &str,
        msg_type: &str,
        payload: &str,
        created_at: i64,
    ) -> String {
        let msg_id = Uuid::new_v4().to_string();
        sqlx::query(
            "INSERT INTO messages (msg_id, channel_id, sender_id, msg_type, payload, created_at) VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(&msg_id)
        .bind(channel_id)
        .bind(sender_id)
        .bind(msg_type)
        .bind(payload)
        .bind(created_at)
        .execute(pool)
        .await
        .expect("insert message");
        msg_id
    }

    /// Helper: create a setup with a custom data directory and return
    /// (Router, pool, user_id, token, data_dir).
    async fn setup_with_data_dir() -> (Router, sqlx::SqlitePool, String, String, std::path::PathBuf) {
        let data_dir =
            std::env::temp_dir().join(format!("vast-test-{}", Uuid::new_v4()));
        std::fs::create_dir_all(data_dir.join("uploads")).expect("create uploads dir");

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
        let token = crate::auth::create_token_pair(&user_id, secret, 0)
            .expect("Failed to create token")
            .access_token;

        let state = Arc::new(AppState {
            pool: pool.clone(),
            ws_pool: Arc::new(ws::ConnectionPool::new()),
            config: crate::AppConfig {
                jwt_secret: secret.to_string(),
                invite_code: "TEST".to_string(),
                data_dir: data_dir.clone(),
                ..crate::AppConfig::test_default()
            },
        });

        let app = crate::api::routes().with_state(state);
        (app, pool, user_id, token, data_dir)
    }

    // 1. Empty channel produces valid ZIP with empty messages and zero counts.
    #[tokio::test]
    async fn test_download_archive_empty_channel() {
        let (mut app, _pool, _user_id, token) = setup().await;
        let channel_id = create_test_channel(&mut app, "Empty Channel", &token).await;

        let (status, headers, zip_bytes) = download(&mut app, &channel_id, &token).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(
            headers
                .get(header::CONTENT_TYPE)
                .unwrap()
                .to_str()
                .unwrap(),
            "application/zip"
        );
        assert!(
            headers
                .get(header::CONTENT_DISPOSITION)
                .unwrap()
                .to_str()
                .unwrap()
                .contains("Empty Channel-archive.zip")
        );

        let cursor = Cursor::new(zip_bytes);
        let mut archive = zip::ZipArchive::new(cursor).expect("valid ZIP");
        assert_eq!(archive.len(), 2);

        // Verify messages.json
        let msgs: Value = {
            let entry = archive.by_name("messages.json").unwrap();
            serde_json::from_reader(entry).unwrap()
        };
        assert!(msgs.as_array().unwrap().is_empty());

        // Verify channel_info.json
        let info: Value = {
            let entry = archive.by_name("channel_info.json").unwrap();
            serde_json::from_reader(entry).unwrap()
        };
        assert_eq!(info["channel_id"], channel_id);
        assert_eq!(info["name"], "Empty Channel");
        assert_eq!(info["message_count"], 0);
        assert_eq!(info["file_count"], 0);
        assert!(info["exported_at"].as_i64().unwrap() > 0);
    }

    // 2. Messages appear in correct order with sender info.
    #[tokio::test]
    async fn test_download_archive_with_messages() {
        let (mut app, pool, user_id, token) = setup().await;
        let channel_id = create_test_channel(&mut app, "Msg Channel", &token).await;

        let now = chrono::Utc::now().timestamp();
        insert_message(&pool, &channel_id, &user_id, "text", r#"{"text":"first"}"#, now).await;
        insert_message(&pool, &channel_id, &user_id, "text", r#"{"text":"second"}"#, now + 1).await;
        insert_message(&pool, &channel_id, &user_id, "text", r#"{"text":"third"}"#, now + 2).await;

        let (status, _, zip_bytes) = download(&mut app, &channel_id, &token).await;
        assert_eq!(status, StatusCode::OK);

        let cursor = Cursor::new(zip_bytes);
        let mut archive = zip::ZipArchive::new(cursor).expect("valid ZIP");
        let msgs: Vec<Value> = {
            let entry = archive.by_name("messages.json").unwrap();
            serde_json::from_reader(entry).unwrap()
        };
        assert_eq!(msgs.len(), 3);
        assert_eq!(msgs[0]["payload"]["text"], "first");
        assert_eq!(msgs[1]["payload"]["text"], "second");
        assert_eq!(msgs[2]["payload"]["text"], "third");
        for msg in &msgs {
            assert_eq!(msg["sender_id"], user_id);
            assert_eq!(msg["sender_name"], "testuser");
            assert!(!msg["is_bot"].as_bool().unwrap());
        }
    }

    // 3. Thread messages are included flat with thread_parent_id.
    #[tokio::test]
    async fn test_download_archive_with_thread_messages() {
        let (mut app, pool, user_id, token) = setup().await;
        let channel_id = create_test_channel(&mut app, "Thread Channel", &token).await;

        let now = chrono::Utc::now().timestamp();
        let parent_id = insert_message(&pool, &channel_id, &user_id, "text", r#"{"text":"parent"}"#, now).await;
        // Insert replies with thread_parent_id pointing to the parent message's internal id
        let parent_internal_id: i64 = sqlx::query_scalar(
            "SELECT id FROM messages WHERE msg_id = ?",
        )
        .bind(&parent_id)
        .fetch_one(&pool)
        .await
        .unwrap();

        sqlx::query("INSERT INTO messages (msg_id, channel_id, sender_id, msg_type, payload, thread_parent_id, created_at) VALUES (?, ?, ?, ?, ?, ?, ?)")
            .bind(Uuid::new_v4().to_string())
            .bind(&channel_id)
            .bind(&user_id)
            .bind("text")
            .bind(r#"{"text":"reply1"}"#)
            .bind(parent_internal_id)
            .bind(now + 1)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO messages (msg_id, channel_id, sender_id, msg_type, payload, thread_parent_id, created_at) VALUES (?, ?, ?, ?, ?, ?, ?)")
            .bind(Uuid::new_v4().to_string())
            .bind(&channel_id)
            .bind(&user_id)
            .bind("text")
            .bind(r#"{"text":"reply2"}"#)
            .bind(parent_internal_id)
            .bind(now + 2)
            .execute(&pool)
            .await
            .unwrap();

        let (status, _, zip_bytes) = download(&mut app, &channel_id, &token).await;
        assert_eq!(status, StatusCode::OK);

        let cursor = Cursor::new(zip_bytes);
        let mut archive = zip::ZipArchive::new(cursor).expect("valid ZIP");
        let msgs: Vec<Value> = {
            let entry = archive.by_name("messages.json").unwrap();
            serde_json::from_reader(entry).unwrap()
        };
        assert_eq!(msgs.len(), 3);
        assert_eq!(msgs[0]["payload"]["text"], "parent");
        assert!(msgs[0]["thread_parent_id"].is_null());
        assert_eq!(msgs[1]["payload"]["text"], "reply1");
        assert_eq!(msgs[1]["thread_parent_id"].as_i64().unwrap(), parent_internal_id);
        assert_eq!(msgs[2]["payload"]["text"], "reply2");
    }

    // 4. Soft-deleted messages are included with deleted_at set.
    #[tokio::test]
    async fn test_download_archive_includes_deleted_messages() {
        let (mut app, pool, user_id, token) = setup().await;
        let channel_id = create_test_channel(&mut app, "Deleted Msg Channel", &token).await;

        let now = chrono::Utc::now().timestamp();
        let msg_id = insert_message(&pool, &channel_id, &user_id, "text", r#"{"text":"alive"}"#, now).await;
        let deleted_msg_id = Uuid::new_v4().to_string();
        sqlx::query("INSERT INTO messages (msg_id, channel_id, sender_id, msg_type, payload, deleted_at, created_at) VALUES (?, ?, ?, ?, ?, ?, ?)")
            .bind(&deleted_msg_id)
            .bind(&channel_id)
            .bind(&user_id)
            .bind("text")
            .bind(r#"{"text":"deleted"}"#)
            .bind(now + 10)
            .bind(now + 1)
            .execute(&pool)
            .await
            .unwrap();

        let (status, _, zip_bytes) = download(&mut app, &channel_id, &token).await;
        assert_eq!(status, StatusCode::OK);

        let cursor = Cursor::new(zip_bytes);
        let mut archive = zip::ZipArchive::new(cursor).expect("valid ZIP");
        let msgs: Vec<Value> = {
            let entry = archive.by_name("messages.json").unwrap();
            serde_json::from_reader(entry).unwrap()
        };
        assert_eq!(msgs.len(), 2);
        let alive = msgs.iter().find(|m| m["msg_id"] == msg_id).unwrap();
        assert!(alive["deleted_at"].is_null());
        let deleted = msgs.iter().find(|m| m["msg_id"] == deleted_msg_id).unwrap();
        assert!(deleted["deleted_at"].as_i64().is_some());
    }

    // 5. Files appear under files/ in the ZIP.
    #[tokio::test]
    async fn test_download_archive_with_files() {
        let (mut app, pool, user_id, token, data_dir) = setup_with_data_dir().await;
        let channel_id = create_test_channel(&mut app, "File Channel", &token).await;

        let now = chrono::Utc::now().timestamp();
        let file_id1 = Uuid::new_v4().to_string();
        let file_id2 = Uuid::new_v4().to_string();

        // Insert file rows
        sqlx::query("INSERT INTO files (id, uploader_id, channel_id, original_name, storage_path, size, mime_type, extension, created_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)")
            .bind(&file_id1)
            .bind(&user_id)
            .bind(&channel_id)
            .bind("hello.txt")
            .bind("uploads/hello.txt")
            .bind(13)
            .bind("text/plain")
            .bind("txt")
            .bind(now)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO files (id, uploader_id, channel_id, original_name, storage_path, size, mime_type, extension, created_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)")
            .bind(&file_id2)
            .bind(&user_id)
            .bind(&channel_id)
            .bind("data.json")
            .bind("uploads/data.json")
            .bind(18)
            .bind("application/json")
            .bind("json")
            .bind(now + 1)
            .execute(&pool)
            .await
            .unwrap();

        // Write physical files
        std::fs::write(
            data_dir.join("uploads").join(format!("{file_id1}.txt")),
            b"Hello, World!",
        )
        .unwrap();
        std::fs::write(
            data_dir.join("uploads").join(format!("{file_id2}.json")),
            b"{\"key\": \"value\"}",
        )
        .unwrap();

        let (status, _, zip_bytes) = download(&mut app, &channel_id, &token).await;
        assert_eq!(status, StatusCode::OK);

        let cursor = Cursor::new(zip_bytes);
        let mut archive = zip::ZipArchive::new(cursor).expect("valid ZIP");

        // Verify files exist
        let hello: Vec<u8> = {
            let mut entry = archive.by_name("files/hello.txt").unwrap();
            let mut buf = Vec::new();
            std::io::Read::read_to_end(&mut entry, &mut buf).unwrap();
            buf
        };
        assert_eq!(hello, b"Hello, World!");

        let data: Vec<u8> = {
            let mut entry = archive.by_name("files/data.json").unwrap();
            let mut buf = Vec::new();
            std::io::Read::read_to_end(&mut entry, &mut buf).unwrap();
            buf
        };
        assert_eq!(data, b"{\"key\": \"value\"}");

        // Verify channel_info has file_count = 2
        let info: Value = {
            let entry = archive.by_name("channel_info.json").unwrap();
            serde_json::from_reader(entry).unwrap()
        };
        assert_eq!(info["file_count"], 2);
    }

    // 6. Soft-deleted files are excluded.
    #[tokio::test]
    async fn test_download_archive_excludes_deleted_files() {
        let (mut app, pool, user_id, token, data_dir) = setup_with_data_dir().await;
        let channel_id = create_test_channel(&mut app, "DelFile Channel", &token).await;

        let now = chrono::Utc::now().timestamp();
        let active_id = Uuid::new_v4().to_string();
        let deleted_id = Uuid::new_v4().to_string();

        sqlx::query("INSERT INTO files (id, uploader_id, channel_id, original_name, storage_path, size, mime_type, extension, is_deleted, created_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, 0, ?)")
            .bind(&active_id)
            .bind(&user_id)
            .bind(&channel_id)
            .bind("active.txt")
            .bind("uploads/active.txt")
            .bind(7)
            .bind("text/plain")
            .bind("txt")
            .bind(now)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO files (id, uploader_id, channel_id, original_name, storage_path, size, mime_type, extension, is_deleted, created_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, 1, ?)")
            .bind(&deleted_id)
            .bind(&user_id)
            .bind(&channel_id)
            .bind("deleted.txt")
            .bind("uploads/deleted.txt")
            .bind(9)
            .bind("text/plain")
            .bind("txt")
            .bind(now + 1)
            .execute(&pool)
            .await
            .unwrap();

        std::fs::write(data_dir.join("uploads").join(format!("{active_id}.txt")), b"active").unwrap();
        std::fs::write(data_dir.join("uploads").join(format!("{deleted_id}.txt")), b"deleted").unwrap();

        let (status, _, zip_bytes) = download(&mut app, &channel_id, &token).await;
        assert_eq!(status, StatusCode::OK);

        let cursor = Cursor::new(zip_bytes);
        let mut archive = zip::ZipArchive::new(cursor).expect("valid ZIP");
        assert!(archive.by_name("files/active.txt").is_ok());
        assert!(archive.by_name("files/deleted.txt").is_err());

        let info: Value = {
            let entry = archive.by_name("channel_info.json").unwrap();
            serde_json::from_reader(entry).unwrap()
        };
        assert_eq!(info["file_count"], 1);
    }

    // 7. Non-owner member gets 403.
    #[tokio::test]
    async fn test_download_archive_non_owner_forbidden() {
        let (mut app, pool, _, owner_token) = setup().await;
        let channel_id = create_test_channel(&mut app, "Owner Channel", &owner_token).await;

        // Create second user and add as member
        let user2_id = Uuid::new_v4().to_string();
        let pw = crate::auth::hash_password("pass2").unwrap();
        sqlx::query("INSERT INTO users (id, username, password_hash) VALUES (?, ?, ?)")
            .bind(&user2_id)
            .bind("member")
            .bind(&pw)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO channel_members (channel_id, user_id, role) VALUES (?, ?, 'member')")
            .bind(&channel_id)
            .bind(&user2_id)
            .execute(&pool)
            .await
            .unwrap();
        let secret = "test-secret";
        let member_token = crate::auth::create_token_pair(&user2_id, secret, 0)
            .unwrap()
            .access_token;

        let (status, _, _) = download(&mut app, &channel_id, &member_token).await;
        assert_eq!(status, StatusCode::FORBIDDEN);
    }

    // 8. Non-member gets 403.
    #[tokio::test]
    async fn test_download_archive_non_member_forbidden() {
        let (mut app, pool, _, owner_token) = setup().await;
        let channel_id = create_test_channel(&mut app, "Private Channel", &owner_token).await;

        let user2_id = Uuid::new_v4().to_string();
        let pw = crate::auth::hash_password("pass2").unwrap();
        sqlx::query("INSERT INTO users (id, username, password_hash) VALUES (?, ?, ?)")
            .bind(&user2_id)
            .bind("outsider")
            .bind(&pw)
            .execute(&pool)
            .await
            .unwrap();
        let secret = "test-secret";
        let outsider_token = crate::auth::create_token_pair(&user2_id, secret, 0)
            .unwrap()
            .access_token;

        let (status, _, _) = download(&mut app, &channel_id, &outsider_token).await;
        assert_eq!(status, StatusCode::FORBIDDEN);
    }

    // 9. DM channel is rejected with 403.
    #[tokio::test]
    async fn test_download_archive_dm_rejected() {
        let (mut app, pool, user_id, token) = setup().await;

        // Create a DM channel (direct insert with is_direct = 1)
        let dm_id = Uuid::new_v4().to_string();
        let now = chrono::Utc::now().timestamp();
        sqlx::query("INSERT INTO channels (id, name, description, owner_id, is_direct, created_at) VALUES (?, ?, '', ?, 1, ?)")
            .bind(&dm_id)
            .bind("DM")
            .bind(&user_id)
            .bind(now)
            .execute(&pool)
            .await
            .unwrap();

        // Also add user as member so they count as a member
        sqlx::query("INSERT INTO channel_members (channel_id, user_id, role) VALUES (?, ?, 'owner')")
            .bind(&dm_id)
            .bind(&user_id)
            .execute(&pool)
            .await
            .unwrap();

        let (status, _, _) = download(&mut app, &dm_id, &token).await;
        assert_eq!(status, StatusCode::FORBIDDEN);
    }

    // 10. Non-existent channel returns 404.
    #[tokio::test]
    async fn test_download_archive_not_found() {
        let (mut app, _, _, token) = setup().await;
        let fake_id = "nonexistent-channel-id";
        let (status, _, _) = download(&mut app, fake_id, &token).await;
        assert_eq!(status, StatusCode::NOT_FOUND);
    }

    // 11. Duplicate filenames are deduplicated.
    #[tokio::test]
    async fn test_download_archive_duplicate_filenames() {
        let (mut app, pool, user_id, token, data_dir) = setup_with_data_dir().await;
        let channel_id = create_test_channel(&mut app, "Dup Channel", &token).await;

        let now = chrono::Utc::now().timestamp();
        let f1 = Uuid::new_v4().to_string();
        let f2 = Uuid::new_v4().to_string();

        sqlx::query("INSERT INTO files (id, uploader_id, channel_id, original_name, storage_path, size, mime_type, extension, created_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)")
            .bind(&f1)
            .bind(&user_id)
            .bind(&channel_id)
            .bind("report.pdf")
            .bind("uploads/report.pdf")
            .bind(100)
            .bind("application/pdf")
            .bind("pdf")
            .bind(now)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO files (id, uploader_id, channel_id, original_name, storage_path, size, mime_type, extension, created_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)")
            .bind(&f2)
            .bind(&user_id)
            .bind(&channel_id)
            .bind("report.pdf")
            .bind("uploads/report.pdf")
            .bind(200)
            .bind("application/pdf")
            .bind("pdf")
            .bind(now + 1)
            .execute(&pool)
            .await
            .unwrap();

        std::fs::write(data_dir.join("uploads").join(format!("{f1}.pdf")), b"first").unwrap();
        std::fs::write(data_dir.join("uploads").join(format!("{f2}.pdf")), b"second").unwrap();

        let (status, _, zip_bytes) = download(&mut app, &channel_id, &token).await;
        assert_eq!(status, StatusCode::OK);

        let cursor = Cursor::new(zip_bytes);
        let mut archive = zip::ZipArchive::new(cursor).expect("valid ZIP");

        let first: Vec<u8> = {
            let mut entry = archive.by_name("files/report.pdf").unwrap();
            let mut buf = Vec::new();
            std::io::Read::read_to_end(&mut entry, &mut buf).unwrap();
            buf
        };
        assert_eq!(first, b"first");

        let second: Vec<u8> = {
            let mut entry = archive.by_name("files/report (1).pdf").unwrap();
            let mut buf = Vec::new();
            std::io::Read::read_to_end(&mut entry, &mut buf).unwrap();
            buf
        };
        assert_eq!(second, b"second");
    }

    // 12. Missing physical file is gracefully skipped.
    #[tokio::test]
    async fn test_download_archive_missing_file_on_disk() {
        let (mut app, pool, user_id, token, data_dir) = setup_with_data_dir().await;
        let channel_id = create_test_channel(&mut app, "Missing Channel", &token).await;

        let now = chrono::Utc::now().timestamp();
        let present_id = Uuid::new_v4().to_string();
        let missing_id = Uuid::new_v4().to_string();

        sqlx::query("INSERT INTO files (id, uploader_id, channel_id, original_name, storage_path, size, mime_type, extension, created_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)")
            .bind(&present_id)
            .bind(&user_id)
            .bind(&channel_id)
            .bind("present.txt")
            .bind("uploads/present.txt")
            .bind(8)
            .bind("text/plain")
            .bind("txt")
            .bind(now)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO files (id, uploader_id, channel_id, original_name, storage_path, size, mime_type, extension, created_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)")
            .bind(&missing_id)
            .bind(&user_id)
            .bind(&channel_id)
            .bind("missing.txt")
            .bind("uploads/missing.txt")
            .bind(9)
            .bind("text/plain")
            .bind("txt")
            .bind(now + 1)
            .execute(&pool)
            .await
            .unwrap();

        // Only write the present file; missing file is never created
        std::fs::write(
            data_dir.join("uploads").join(format!("{present_id}.txt")),
            b"present",
        )
        .unwrap();

        let (status, _, zip_bytes) = download(&mut app, &channel_id, &token).await;
        assert_eq!(status, StatusCode::OK);

        let cursor = Cursor::new(zip_bytes);
        let mut archive = zip::ZipArchive::new(cursor).expect("valid ZIP");

        // Present file is included
        let present: Vec<u8> = {
            let mut entry = archive.by_name("files/present.txt").unwrap();
            let mut buf = Vec::new();
            std::io::Read::read_to_end(&mut entry, &mut buf).unwrap();
            buf
        };
        assert_eq!(present, b"present");

        // Missing file is not in the archive
        assert!(archive.by_name("files/missing.txt").is_err());

        // file_count reflects actual files on disk, not DB count
        let info: Value = {
            let entry = archive.by_name("channel_info.json").unwrap();
            serde_json::from_reader(entry).unwrap()
        };
        assert_eq!(info["file_count"], 1);
    }

    // 13. Channel name is sanitized in Content-Disposition filename.
    #[tokio::test]
    async fn test_download_archive_sanitized_filename() {
        let (mut app, pool, user_id, token) = setup().await;

        // Create channel with special characters in the name
        let channel_id = Uuid::new_v4().to_string();
        let now = chrono::Utc::now().timestamp();
        sqlx::query("INSERT INTO channels (id, name, description, owner_id, created_at) VALUES (?, ?, '', ?, ?)")
            .bind(&channel_id)
            .bind("report/final:2026")
            .bind(&user_id)
            .bind(now)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO channel_members (channel_id, user_id, role) VALUES (?, ?, 'owner')")
            .bind(&channel_id)
            .bind(&user_id)
            .execute(&pool)
            .await
            .unwrap();

        let (status, headers, _zip_bytes) = download(&mut app, &channel_id, &token).await;
        assert_eq!(status, StatusCode::OK);

        let disposition = headers
            .get(header::CONTENT_DISPOSITION)
            .unwrap()
            .to_str()
            .unwrap();
        assert!(disposition.contains("report_final_2026-archive.zip"));
        assert!(!disposition.contains("/"));
        assert!(!disposition.contains(":"));
    }
}
