use axum::{
    extract::{Multipart, Path, Query, State},
    http::{header, HeaderMap, StatusCode},
    Json,
};
use percent_encoding::{utf8_percent_encode, AsciiSet, CONTROLS};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use std::collections::HashMap;
use std::sync::Arc;
use uuid::Uuid;

use crate::auth::middleware::{AuthenticatedUser, require_membership};
use crate::db;
use crate::error::{created_response, AppError};
use crate::ws::protocol::ServerEvent;
use crate::AppState;

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// MIME type whitelist — image/* is wildcard, others are exact.
const ALLOWED_MIME_TYPES: &[&str] = &[
    "image/",      // prefix — any image subtype allowed
    "video/",      // prefix — any video subtype
    "audio/",      // prefix — any audio subtype
    "text/",       // prefix — text/html, text/css, text/javascript, text/markdown, etc.
    "application/pdf",
    "application/zip",
    "application/gzip",
    "application/x-tar",
    "application/x-7z-compressed",
    "application/x-rar-compressed",
    "application/json",
    "application/javascript",
    "application/msword",                                                    // .doc
    "application/vnd.openxmlformats-officedocument.wordprocessingml.document", // .docx
    "application/vnd.ms-excel",                                              // .xls
    "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",     // .xlsx
    "application/vnd.ms-powerpoint",                                         // .ppt
    "application/vnd.openxmlformats-officedocument.presentationml.presentation", // .pptx
    "application/octet-stream",   // fallback for unrecognized file types (.md, .py, .rs, etc.)
];

/// Max upload size (50 MB).
pub const MAX_UPLOAD_SIZE: usize = 50 * 1024 * 1024;

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// Metadata persisted as a JSON sidecar alongside the file.
#[derive(Debug, Serialize, Deserialize)]
pub struct FileMetadata {
    pub file_id: String,
    pub original_name: String,
    pub size: u64,
    pub mime_type: String,
    pub extension: String,
    pub created_at: i64,
}

/// Response returned after a successful upload.
#[derive(Debug, Serialize)]
pub struct UploadResponse {
    pub file_id: String,
    pub url: String,
    pub original_name: String,
    pub size: u64,
    pub mime_type: String,
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Check whether `mime_type` is in the whitelist.
fn is_mime_allowed(mime_type: &str) -> bool {
    ALLOWED_MIME_TYPES.iter().any(|allowed| {
        if *allowed == "image/" {
            mime_type.starts_with("image/")
        } else {
            *allowed == mime_type
        }
    })
}

/// Percent-encoding set for RFC 5987 filename* parameter.
/// Encodes everything except alphanumerics, hyphens, periods, underscores, and spaces.
const RFC_5987_ENCODE_SET: &AsciiSet = &CONTROLS
    .add(b' ')
    .add(b'!')
    .add(b'#')
    .add(b'$')
    .add(b'&')
    .add(b'+')
    .add(b',')
    .add(b'/')
    .add(b':')
    .add(b';')
    .add(b'=')
    .add(b'?')
    .add(b'@')
    .add(b'"')
    .add(b'(')
    .add(b')')
    .add(b'*')
    .add(b',')
    .add(b'<')
    .add(b'>')
    .add(b'\\');

/// Sanitize filename for Content-Disposition header.
/// Returns (safe_ascii_name, percent_encoded_name) for use with:
/// `attachment; filename="<safe_ascii_name>"; filename*=UTF-8''<percent_encoded_name>`
pub fn sanitize_filename_for_header(filename: &str) -> (String, String) {
    let safe_ascii = filename
        .chars()
        .filter(|c| !matches!(c, '"' | '\\' | '\r' | '\n'))
        .collect::<String>();

    let percent_encoded = utf8_percent_encode(filename, RFC_5987_ENCODE_SET).to_string();

    (safe_ascii, percent_encoded)
}

// ---------------------------------------------------------------------------
// List-Files query params
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct ListFilesParams {
    pub channel_id: Option<String>,
    pub uploader_id: Option<String>,
    pub mime_type: Option<String>,
    pub mime_prefix: Option<String>,
    pub size_min: Option<i64>,
    pub size_max: Option<i64>,
    pub created_after: Option<i64>,
    pub created_before: Option<i64>,
    pub search: Option<String>,
    #[serde(default = "default_sort_by")]
    pub sort_by: String,
    #[serde(default = "default_sort_order")]
    pub sort_order: String,
    pub cursor: Option<String>,
    #[serde(default = "default_limit")]
    pub limit: i64,
}

fn default_sort_by() -> String {
    "created_at".to_string()
}
fn default_sort_order() -> String {
    "desc".to_string()
}
fn default_limit() -> i64 {
    50
}

// ---------------------------------------------------------------------------
// Response types
// ---------------------------------------------------------------------------

#[derive(Debug, FromRow, Serialize)]
pub struct FileRow {
    pub id: String,
    pub uploader_id: Option<String>,
    pub channel_id: Option<String>,
    pub original_name: String,
    pub size: i64,
    pub mime_type: String,
    pub extension: String,
    pub is_deleted: bool,
    pub deleted_at: Option<i64>,
    pub deleted_by: Option<String>,
    pub created_at: i64,
}

#[derive(Debug, Serialize)]
pub struct FileResponse {
    pub id: String,
    pub uploader_id: String,
    pub uploader_name: String,
    pub uploader_display_name: String,
    pub uploader_avatar_url: String,
    pub is_bot: bool,
    pub channel_id: Option<String>,
    pub original_name: String,
    pub size: i64,
    pub mime_type: String,
    pub extension: String,
    pub is_deleted: bool,
    pub deleted_at: Option<i64>,
    pub deleted_by: Option<String>,
    pub created_at: i64,
}

#[derive(Debug, Serialize)]
pub struct ListFilesResponse {
    pub files: Vec<FileResponse>,
    pub next_cursor: String,
    pub has_more: bool,
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

/// GET /api/files
pub async fn list_files(
    auth: AuthenticatedUser,
    State(state): State<Arc<AppState>>,
    Query(params): Query<ListFilesParams>,
) -> Result<Json<ListFilesResponse>, AppError> {
    let user_id = auth.0;
    let limit = params.limit.clamp(1, 100);

    // --- validate sort column -------------------------------------------------
    let sort_col = match params.sort_by.as_str() {
        "created_at" => "f.created_at",
        "size" => "f.size",
        "name" => "f.original_name COLLATE NOCASE",
        "mime_type" => "f.mime_type",
        _ => return Err(AppError::BadRequest(format!("Invalid sort_by: {}", params.sort_by))),
    };

    let asc = match params.sort_order.as_str() {
        "asc" => true,
        "desc" => false,
        _ => return Err(AppError::BadRequest(format!("Invalid sort_order: {}", params.sort_order))),
    };

    // Build the non-visibility WHERE conditions dynamically
    let mut conditions: Vec<String> = Vec::new();
    let mut bind_idx = 2u32; // bind index starts at 2 (user_id is ?1 in two places)

    if params.channel_id.is_some() {
        conditions.push(format!("f.channel_id = ?{}", bind_idx));
        bind_idx += 1;
    }
    if params.uploader_id.is_some() {
        conditions.push(format!("f.uploader_id = ?{}", bind_idx));
        bind_idx += 1;
    }
    if params.mime_type.is_some() {
        conditions.push(format!("f.mime_type = ?{}", bind_idx));
        bind_idx += 1;
    }
    if params.mime_prefix.is_some() {
        conditions.push(format!("f.mime_type LIKE ?{}", bind_idx));
        bind_idx += 1;
    }
    if params.size_min.is_some() {
        conditions.push(format!("f.size >= ?{}", bind_idx));
        bind_idx += 1;
    }
    if params.size_max.is_some() {
        conditions.push(format!("f.size <= ?{}", bind_idx));
        bind_idx += 1;
    }
    if params.created_after.is_some() {
        conditions.push(format!("f.created_at >= ?{}", bind_idx));
        bind_idx += 1;
    }
    if params.created_before.is_some() {
        conditions.push(format!("f.created_at <= ?{}", bind_idx));
        bind_idx += 1;
    }
    if params.search.is_some() {
        conditions.push(format!("f.original_name LIKE ?{}", bind_idx));
        bind_idx += 1;
    }

    // --- cursor (keyset pagination) -------------------------------------------
    let (cursor_sort_val, cursor_id) = match &params.cursor {
        Some(c) => {
            let parts: Vec<&str> = c.splitn(2, '/').collect();
            if parts.len() != 2 {
                return Err(AppError::BadRequest("Invalid cursor format".into()));
            }
            (parts[0].to_string(), parts[1].to_string())
        }
        None => {
            let default_val = if asc { "".to_string() } else { "\u{10FFFF}".to_string() };
            let default_id = if asc { "".to_string() } else { "\u{10FFFF}".to_string() };
            (default_val, default_id)
        }
    };

    let cursor_op = if asc { ">" } else { "<" };

    // For name sort, the cursor value is a name string; for numeric sorts it's a number.
    // Use the same sort_col expression for the cursor comparison.
    let cursor_param_start = bind_idx;
    conditions.push(format!(
        "({sort_col}, f.id) {cursor_op} (?{csv}, ?{cidv})",
        csv = cursor_param_start,
        cidv = cursor_param_start + 1,
    ));
    bind_idx += 2;

    let where_clause = if conditions.is_empty() {
        String::new()
    } else {
        format!("AND {}", conditions.join(" AND "))
    };

    let order_dir = if asc { "ASC" } else { "DESC" };

    let sql = format!(
        "SELECT f.id, f.uploader_id, f.channel_id, f.original_name, \
                f.size, f.mime_type, f.extension, f.is_deleted, \
                f.deleted_at, f.deleted_by, f.created_at \
         FROM files f \
         WHERE (f.channel_id IN (SELECT channel_id FROM channel_members WHERE user_id = ?1) \
                OR f.uploader_id = ?1) \
         {where_clause} \
         ORDER BY {sort_col} {order_dir}, f.id {order_dir} \
         LIMIT ?{limit_param}",
        where_clause = where_clause,
        sort_col = sort_col,
        order_dir = order_dir,
        limit_param = bind_idx,
    );

    let mut q = sqlx::query_as::<_, FileRow>(&sql).bind(&user_id);

    // Bind filter params in same order as conditions were added
    if let Some(ref cid) = params.channel_id {
        q = q.bind(cid);
    }
    if let Some(ref uid) = params.uploader_id {
        q = q.bind(uid);
    }
    if let Some(ref mt) = params.mime_type {
        q = q.bind(mt);
    }
    if let Some(ref mp) = params.mime_prefix {
        q = q.bind(format!("%{mp}%"));
    }
    if let Some(smin) = params.size_min {
        q = q.bind(smin);
    }
    if let Some(smax) = params.size_max {
        q = q.bind(smax);
    }
    if let Some(ca) = params.created_after {
        q = q.bind(ca);
    }
    if let Some(cb) = params.created_before {
        q = q.bind(cb);
    }
    if let Some(ref s) = params.search {
        q = q.bind(format!("%{s}%"));
    }

    // Bind cursor values
    q = q.bind(&cursor_sort_val).bind(&cursor_id);
    q = q.bind(limit + 1);

    let rows = q.fetch_all(&state.pool).await?;
    let has_more = (rows.len() as i64) > limit;

    // --- batch-fetch user info -------------------------------------------------
    let uploader_ids: Vec<String> = rows.iter().filter_map(|r| r.uploader_id.clone()).collect();

    type UserInfo = (String, String, String, bool);
    let usernames: HashMap<String, UserInfo> = if uploader_ids.is_empty() {
        HashMap::new()
    } else {
        let placeholders: Vec<String> = (0..uploader_ids.len())
            .map(|i| format!("?{}", i + 1))
            .collect();
        let query = format!(
            "SELECT id, username, display_name, avatar_url, is_bot FROM users WHERE id IN ({})",
            placeholders.join(",")
        );
        let mut uq = sqlx::query_as::<_, (String, String, String, String, bool)>(&query);
        for id in &uploader_ids {
            uq = uq.bind(id);
        }
        uq.fetch_all(&state.pool)
            .await
            .unwrap_or_default()
            .into_iter()
            .map(|(id, u, d, a, b)| (id, (u, d, a, b)))
            .collect()
    };

    // --- build response --------------------------------------------------------
    let files: Vec<FileResponse> = rows
        .into_iter()
        .take(limit as usize)
        .map(|r| {
            let (uploader_name, uploader_display_name, uploader_avatar_url, is_bot) =
                r.uploader_id.as_deref()
                    .and_then(|id| usernames.get(id))
                    .cloned()
                    .unwrap_or_else(|| {
                        ("unknown".into(), "Unknown".into(), String::new(), false)
                    });
            FileResponse {
                id: r.id,
                uploader_id: r.uploader_id.clone().unwrap_or_else(|| "unknown".into()),
                uploader_name,
                uploader_display_name,
                uploader_avatar_url,
                is_bot,
                channel_id: r.channel_id,
                original_name: r.original_name,
                size: r.size,
                mime_type: r.mime_type,
                extension: r.extension,
                is_deleted: r.is_deleted,
                deleted_at: r.deleted_at,
                deleted_by: r.deleted_by,
                created_at: r.created_at,
            }
        })
        .collect();

    let default_cursor = "".to_string();
    let next_cursor = files.last().map_or_else(
        || default_cursor,
        |f| {
            let sort_val = match params.sort_by.as_str() {
                "created_at" => f.created_at.to_string(),
                "size" => f.size.to_string(),
                "name" => f.original_name.clone(),
                "mime_type" => f.mime_type.clone(),
                _ => String::new(),
            };
            format!("{sort_val}/{}", f.id)
        },
    );

    Ok(Json(ListFilesResponse {
        files,
        next_cursor,
        has_more,
    }))
}

/// POST /api/files/upload
///
/// Accept a multipart form with a `file` field.  Validates MIME type, saves
/// the file under `data/uploads/{uuid}.{ext}` and writes a JSON sidecar
/// `data/uploads/{uuid}.meta.json` with the original metadata.
pub async fn upload_file(
    auth: AuthenticatedUser,
    State(state): State<Arc<AppState>>,
    mut multipart: Multipart,
) -> Result<(StatusCode, Json<UploadResponse>), AppError> {
    let user_id = auth.0;

    // --- parse all multipart fields ----------------------------------------------
    let mut channel_id: Option<String> = None;
    let mut file_field: Option<(String, String, Vec<u8>)> = None;

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| AppError::BadRequest(format!("Failed to read multipart data: {e}")))?
    {
        match field.name() {
            Some("file") if file_field.is_none() => {
                let original_name = field.file_name().unwrap_or("unnamed").to_string();
                let mime_type = field
                    .content_type()
                    .map(|m| m.to_string())
                    .unwrap_or_else(|| "application/octet-stream".to_string());
                let data = field
                    .bytes()
                    .await
                    .map_err(|e| AppError::BadRequest(format!("Failed to read file data: {e}")))?;
                file_field = Some((original_name, mime_type, data.to_vec()));
            }
            Some("channel_id") if channel_id.is_none() => {
                let text = field
                    .text()
                    .await
                    .map_err(|e| AppError::BadRequest(format!("Failed to read channel_id: {e}")))?;
                let trimmed = text.trim().to_string();
                if !trimmed.is_empty() {
                    channel_id = Some(trimmed);
                }
            }
            _ => {} // ignore unknown fields
        }
    }

    let (original_name, mime_type, data) =
        file_field.ok_or_else(|| AppError::BadRequest("No file field found".into()))?;

    // --- check channel membership if channel_id provided -------------------------
    if let Some(ref cid) = channel_id {
        require_membership(&state.pool, &user_id, cid).await?;
    }

    // --- MIME whitelist check ----------------------------------------------------
    if !is_mime_allowed(&mime_type) {
        return Err(AppError::UnsupportedMediaType(format!(
            "File type '{mime_type}' is not allowed"
        )));
    }

    let file_size = data.len();

    // --- disk space check --------------------------------------------------------
    db::check_disk_space(&state.config.data_dir)
        .map_err(|e| AppError::Internal(format!("Disk space check failed: {e}")))?;

    // --- build deterministic path ------------------------------------------------
    let file_id = Uuid::new_v4().to_string();

    // Determine extension: prefer original-extension, fall back to mime_guess.
    let ext = std::path::Path::new(&original_name)
        .extension()
        .and_then(|e| e.to_str())
        .filter(|e| !e.is_empty())
        .map(|e| e.to_lowercase())
        .unwrap_or_else(|| {
            mime_guess::get_mime_extensions(
                &mime_type.parse().unwrap_or(mime::APPLICATION_OCTET_STREAM),
            )
            .and_then(|exts| exts.first().copied())
            .unwrap_or("bin")
            .to_string()
        });

    let filename = format!("{file_id}.{ext}");
    let upload_dir = state.config.data_dir.join("uploads");

    // --- write file --------------------------------------------------------------
    let filepath = upload_dir.join(&filename);
    tokio::fs::write(&filepath, &data)
        .await
        .map_err(|e| AppError::Internal(format!("Failed to save file: {e}")))?;

    // --- write metadata sidecar -------------------------------------------------
    let created_at = chrono::Utc::now().timestamp();
    let metadata = FileMetadata {
        file_id: file_id.clone(),
        original_name: original_name.clone(),
        size: file_size as u64,
        mime_type: mime_type.clone(),
        extension: ext.clone(),
        created_at,
    };

    let meta_path = upload_dir.join(format!("{file_id}.meta.json"));
    let meta_json = serde_json::to_string_pretty(&metadata)
        .map_err(|e| AppError::Internal(format!("Failed to serialize metadata: {e}")))?;
    tokio::fs::write(&meta_path, meta_json)
        .await
        .map_err(|e| AppError::Internal(format!("Failed to save metadata: {e}")))?;

    // --- insert file record into database ----------------------------------------
    let storage_path = format!("uploads/{}", filename);
    sqlx::query(
        "INSERT INTO files (id, uploader_id, channel_id, original_name, storage_path, size, mime_type, extension, created_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&file_id)
    .bind(&user_id)
    .bind(&channel_id)
    .bind(&original_name)
    .bind(&storage_path)
    .bind(file_size as i64)
    .bind(&mime_type)
    .bind(&ext)
    .bind(created_at)
    .execute(&state.pool)
    .await
    .map_err(|e| AppError::Internal(format!("Failed to save file record: {e}")))?;

    let url = format!("/api/files/{file_id}");

    created_response(UploadResponse {
        file_id,
        url,
        original_name,
        size: file_size as u64,
        mime_type,
    })
}

/// GET /api/files/{file_id}
///
/// Stream the uploaded file with the correct `Content-Type` and
/// `Content-Disposition: attachment` headers.
pub async fn download_file(
    auth: AuthenticatedUser,
    State(state): State<Arc<AppState>>,
    Path(file_id): Path<String>,
) -> Result<(HeaderMap, Vec<u8>), AppError> {
    let user_id = auth.0;

    // Try database first — access control + soft-delete awareness
    let db_file: Option<FileRow> = sqlx::query_as(
        "SELECT id, uploader_id, channel_id, original_name, size, mime_type, extension, is_deleted, deleted_at, deleted_by, created_at FROM files WHERE id = ?",
    )
    .bind(&file_id)
    .fetch_optional(&state.pool)
    .await?;

    if let Some(f) = db_file {
        // --- access control ---
        // Allow if user is the uploader
        let is_uploader = f.uploader_id.as_deref() == Some(&user_id);
        if !is_uploader {
            // Allow if user is a member of the file's channel
            if let Some(ref ch) = f.channel_id {
                require_membership(&state.pool, &user_id, ch).await?;
            } else {
                return Err(AppError::Forbidden(
                    "You do not have permission to access this file".to_string(),
                ));
            }
        }

        // --- soft-delete check ---
        if f.is_deleted {
            return Err(AppError::Gone(
                "该文件已被发布者删除".to_string(),
            ));
        }

        // Serve from DB metadata
        let upload_dir = state.config.data_dir.join("uploads");
        let filepath = upload_dir.join(format!("{}.{}", file_id, f.extension));
        let data = tokio::fs::read(&filepath)
            .await
            .map_err(|_| AppError::NotFound("File data not found on disk".to_string()))?;

        let mime_type = f.mime_type.parse::<mime::Mime>().unwrap_or(mime::APPLICATION_OCTET_STREAM);
        let (safe_ascii_name, percent_encoded_name) = sanitize_filename_for_header(&f.original_name);
        let content_disposition = format!(
            "attachment; filename=\"{}\"; filename*=UTF-8''{}",
            safe_ascii_name, percent_encoded_name
        );

        let mut headers = HeaderMap::new();
        headers.insert(header::CONTENT_TYPE, mime_type.to_string().parse().unwrap());
        headers.insert(header::CONTENT_DISPOSITION, content_disposition.parse().unwrap());
        return Ok((headers, data));
    }

    // --- fallback: legacy disk-only file (no DB record) ---
    let upload_dir = state.config.data_dir.join("uploads");

    let meta_path = upload_dir.join(format!("{file_id}.meta.json"));
    let meta_json = tokio::fs::read_to_string(&meta_path)
        .await
        .map_err(|_| AppError::NotFound("File not found".into()))?;

    let metadata: FileMetadata = serde_json::from_str(&meta_json)
        .map_err(|_| AppError::Internal("Corrupt file metadata".into()))?;

    let filename = format!("{}.{}", file_id, metadata.extension);
    let filepath = upload_dir.join(&filename);

    let data = tokio::fs::read(&filepath)
        .await
        .map_err(|_| AppError::NotFound("File not found".into()))?;

    let mime_type = metadata.mime_type.parse::<mime::Mime>().unwrap_or(mime::APPLICATION_OCTET_STREAM);
    let (safe_ascii_name, percent_encoded_name) = sanitize_filename_for_header(&metadata.original_name);
    let content_disposition = format!(
        "attachment; filename=\"{}\"; filename*=UTF-8''{}",
        safe_ascii_name, percent_encoded_name
    );

    let mut headers = HeaderMap::new();
    headers.insert(
        header::CONTENT_TYPE,
        mime_type.to_string().parse().unwrap(),
    );
    headers.insert(
        header::CONTENT_DISPOSITION,
        content_disposition.parse().unwrap(),
    );

    Ok((headers, data))
}

/// DELETE /api/files/{file_id}
///
/// Soft-deletes a file. Only the original uploader may delete.
/// Broadcasts a FileDeleted WebSocket event to the channel.
pub async fn delete_file(
    auth: AuthenticatedUser,
    State(state): State<Arc<AppState>>,
    Path(file_id): Path<String>,
) -> Result<StatusCode, AppError> {
    let user_id = auth.0;

    let row: Option<(Option<String>, Option<String>, bool)> =
        sqlx::query_as("SELECT uploader_id, channel_id, is_deleted FROM files WHERE id = ?")
            .bind(&file_id)
            .fetch_optional(&state.pool)
            .await?;

    let (uploader_id, channel_id, is_deleted) = match row {
        Some(r) => r,
        None => return Err(AppError::NotFound("File not found".to_string())),
    };

    if is_deleted {
        return Err(AppError::NotFound("File not found".to_string()));
    }

    let Some(ref uid) = uploader_id else {
        return Err(AppError::Forbidden(
            "File has no uploader — cannot be deleted".to_string(),
        ));
    };
    if uid != &user_id {
        return Err(AppError::Forbidden(
            "You can only delete your own files".to_string(),
        ));
    }

    sqlx::query(
        "UPDATE files SET is_deleted = 1, deleted_at = unixepoch(), deleted_by = ? WHERE id = ?",
    )
    .bind(&user_id)
    .bind(&file_id)
    .execute(&state.pool)
    .await?;

    if let Some(ref ch_id) = channel_id {
        state.ws_pool.notify_channel(
            ch_id,
            ServerEvent::FileDeleted {
                file_id: file_id.clone(),
                channel_id: ch_id.clone(),
            },
        );
    }

    Ok(StatusCode::NO_CONTENT)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        body::Body,
        http::{Request, StatusCode},
        routing::{get, post},
        Router,
    };
    use std::sync::Arc;
    use tower::ServiceExt;

    fn ensure_env() {
        static ENV: std::sync::OnceLock<()> = std::sync::OnceLock::new();
        ENV.get_or_init(|| unsafe {
            std::env::set_var("JWT_SECRET", "test");
        });
    }

    fn test_token() -> String {
        crate::auth::create_token_pair("test-user", "test", 0)
            .unwrap()
            .access_token
    }

    /// Build a minimal test router backed by a temporary directory.
    async fn test_app() -> Router {
        let tmp = std::env::temp_dir().join(format!("vast-test-files-{}", Uuid::new_v4()));
        std::fs::create_dir_all(tmp.join("uploads")).unwrap();

        let config = crate::AppConfig {
            data_dir: tmp,
            jwt_secret: "test".into(),
            invite_code: "test".into(),
            ..crate::AppConfig::test_default()
        };

        // In-memory SQLite pool. max_connections(1) is required because
        // plain `:memory:` gives each connection its own private database;
        // with >1 connection the INSERT below would land on a fresh DB.
        let opts = sqlx::sqlite::SqliteConnectOptions::new()
            .filename(":memory:")
            .create_if_missing(true)
            .foreign_keys(true);
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(opts)
            .await
            .unwrap();
        sqlx::migrate!("db/migrations")
            .run(&pool)
            .await
            .unwrap();
        // Seed the test-user so the AuthenticatedUser middleware's epoch
        // check (SELECT token_epoch FROM users WHERE id = ?) finds a row.
        sqlx::query(
            "INSERT INTO users (id, username, password_hash) VALUES ('test-user', 'test-user', 'x')",
        )
        .execute(&pool)
        .await
        .unwrap();

        let ws_pool = Arc::new(crate::ws::ConnectionPool::new());

        let state = Arc::new(crate::AppState {
            pool,
            ws_pool,
            config,
        });

        Router::new()
            .route("/api/files/upload", post(upload_file))
            .route("/api/files/{file_id}", get(download_file))
            .with_state(state)
    }

    fn png_bytes() -> Vec<u8> {
        // Minimal valid 1×1 pixel PNG (CRC included)
        let png: &[u8] = &[
            0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, // PNG signature
            0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44, 0x52, // IHDR chunk
            0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01,
            0x08, 0x02, 0x00, 0x00, 0x00, 0x90, 0x77, 0x53,
            0xDE, 0x00, 0x00, 0x00, 0x0C, 0x49, 0x44, 0x41, // IDAT chunk
            0x54, 0x08, 0xD7, 0x63, 0x60, 0x60, 0x60, 0x00,
            0x00, 0x00, 0x04, 0x00, 0x01, 0x27, 0x34, 0x27,
            0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4E, 0x44, // IEND chunk
            0xAE, 0x42, 0x60, 0x82,
        ];
        png.to_vec()
    }

    fn multipart_body(
        field_name: &str,
        file_name: &str,
        content_type: &str,
        data: Vec<u8>,
    ) -> (Body, String) {
        let boundary = "----testboundary42";
        let mut body = Vec::new();

        // Write the multipart preamble
        body.extend_from_slice(format!("--{boundary}\r\n").as_bytes());
        body.extend_from_slice(
            format!(
                "Content-Disposition: form-data; name=\"{field_name}\"; filename=\"{file_name}\"\r\n"
            )
            .as_bytes(),
        );
        body.extend_from_slice(format!("Content-Type: {content_type}\r\n").as_bytes());
        body.extend_from_slice(b"\r\n");
        body.extend_from_slice(&data);
        body.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());

        let content_type = format!("multipart/form-data; boundary={boundary}");
        (Body::from(body), content_type)
    }

    // -----------------------------------------------------------------------
    // Tests
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_upload_png_returns_201() {
        ensure_env();
        let app = test_app().await;
        let token = test_token();
        let (body, ct) = multipart_body("file", "test.png", "image/png", png_bytes());

        let resp = app
            .oneshot(
                Request::post("/api/files/upload")
                    .header("Content-Type", &ct)
                    .header("Authorization", format!("Bearer {token}"))
                    .body(body)
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::CREATED);

        let body_bytes = axum::body::to_bytes(resp.into_body(), MAX_UPLOAD_SIZE).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
        assert!(json["file_id"].is_string());
        assert_eq!(json["original_name"], "test.png");
        assert_eq!(json["mime_type"], "image/png");
        assert!(json["size"].as_u64().unwrap() > 0);
        assert!(json["url"].is_string());
    }

    #[tokio::test]
    async fn test_upload_without_auth_returns_401() {
        ensure_env();
        let app = test_app().await;
        let (body, ct) = multipart_body("file", "test.png", "image/png", png_bytes());

        let resp = app
            .oneshot(
                Request::post("/api/files/upload")
                    .header("Content-Type", &ct)
                    .body(body)
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn test_upload_disallowed_mime_returns_415() {
        ensure_env();
        let app = test_app().await;
        let token = test_token();
        let (body, ct) = multipart_body("file", "evil.exe", "application/x-msdownload", vec![0u8; 100]);

        let resp = app
            .oneshot(
                Request::post("/api/files/upload")
                    .header("Content-Type", &ct)
                    .header("Authorization", format!("Bearer {token}"))
                    .body(body)
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::UNSUPPORTED_MEDIA_TYPE);
    }

    #[tokio::test]
    async fn test_upload_wrong_field_name_returns_400() {
        ensure_env();
        let app = test_app().await;
        let token = test_token();
        let (body, ct) = multipart_body("not_file", "test.png", "image/png", png_bytes());

        let resp = app
            .oneshot(
                Request::post("/api/files/upload")
                    .header("Content-Type", &ct)
                    .header("Authorization", format!("Bearer {token}"))
                    .body(body)
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_download_returns_correct_mime() {
        ensure_env();
        let app = test_app().await;
        let token = test_token();
        let (body, ct) = multipart_body("file", "photo.png", "image/png", png_bytes());

        // Upload first
        let upload_resp = app
            .clone()
            .oneshot(
                Request::post("/api/files/upload")
                    .header("Content-Type", &ct)
                    .header("Authorization", format!("Bearer {token}"))
                    .body(body)
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(upload_resp.status(), StatusCode::CREATED);

        let body_bytes = axum::body::to_bytes(upload_resp.into_body(), MAX_UPLOAD_SIZE)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
        let file_id = json["file_id"].as_str().unwrap().to_string();

        // Download
        let download_resp = app
            .oneshot(
                Request::get(format!("/api/files/{file_id}"))
                    .header("Authorization", format!("Bearer {token}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(download_resp.status(), StatusCode::OK);
        assert_eq!(
            download_resp
                .headers()
                .get(header::CONTENT_TYPE)
                .and_then(|v| v.to_str().ok()),
            Some("image/png")
        );
        assert!(download_resp
            .headers()
            .get(header::CONTENT_DISPOSITION)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .contains("photo.png"));
    }

    #[tokio::test]
    async fn test_download_without_auth_returns_401() {
        ensure_env();
        let app = test_app().await;

        let resp = app
            .oneshot(
                Request::get("/api/files/00000000-0000-0000-0000-000000000000")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn test_download_missing_returns_404() {
        ensure_env();
        let app = test_app().await;
        let token = test_token();

        let resp = app
            .oneshot(
                Request::get("/api/files/00000000-0000-0000-0000-000000000000")
                    .header("Authorization", format!("Bearer {token}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_download_sanitizes_filename_with_special_characters() {
        ensure_env();
        let app = test_app().await;
        let token = test_token();

        let malicious_filename = "test\"file.png\\test";
        let (body, ct) = multipart_body("file", malicious_filename, "image/png", png_bytes());

        let upload_resp = app
            .clone()
            .oneshot(
                Request::post("/api/files/upload")
                    .header("Content-Type", &ct)
                    .header("Authorization", format!("Bearer {token}"))
                    .body(body)
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(upload_resp.status(), StatusCode::CREATED);

        let body_bytes = axum::body::to_bytes(upload_resp.into_body(), MAX_UPLOAD_SIZE)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
        let file_id = json["file_id"].as_str().unwrap().to_string();

        let download_resp = app
            .oneshot(
                Request::get(format!("/api/files/{file_id}"))
                    .header("Authorization", format!("Bearer {token}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(download_resp.status(), StatusCode::OK);

        let content_disposition = download_resp
            .headers()
            .get(header::CONTENT_DISPOSITION)
            .and_then(|v| v.to_str().ok())
            .unwrap();

        assert!(content_disposition.starts_with("attachment; filename=\""));
        assert!(content_disposition.contains("; filename*=UTF-8''"));

        let safe_name_part = content_disposition
            .split("; filename*=")
            .next()
            .unwrap()
            .strip_prefix("attachment; filename=\"")
            .unwrap()
            .strip_suffix("\"")
            .unwrap();

        assert!(!safe_name_part.contains('"'));
        assert!(!safe_name_part.contains('\\'));
    }
}
