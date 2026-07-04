use axum::{
    extract::{Multipart, Path, State},
    http::{header, HeaderMap, StatusCode},
    Json,
};
use percent_encoding::{utf8_percent_encode, AsciiSet, CONTROLS};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use uuid::Uuid;

use crate::auth::middleware::AuthenticatedUser;
use crate::db;
use crate::error::{created_response, AppError};
use crate::AppState;

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// MIME type whitelist — image/* is wildcard, others are exact.
const ALLOWED_MIME_TYPES: &[&str] = &[
    "image/",      // prefix — any image subtype allowed
    "application/pdf",
    "text/plain",
    "application/zip",
    "application/gzip",
    "application/json",
    "text/csv",
    "video/mp4",
    "audio/mpeg",
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
fn sanitize_filename_for_header(filename: &str) -> (String, String) {
    let safe_ascii = filename
        .chars()
        .filter(|c| !matches!(c, '"' | '\\' | '\r' | '\n'))
        .collect::<String>();

    let percent_encoded = utf8_percent_encode(filename, RFC_5987_ENCODE_SET).to_string();

    (safe_ascii, percent_encoded)
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

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
    let _user_id = auth.0;

    // --- extract the first field -------------------------------------------------
    let field = multipart
        .next_field()
        .await
        .map_err(|e| AppError::BadRequest(format!("Failed to read multipart data: {e}")))?
        .ok_or_else(|| AppError::BadRequest("No multipart field found".into()))?;

    // --- optional: verify field is named "file" -----------------------------------
    if field.name() != Some("file") {
        return Err(AppError::BadRequest(
            "Expected multipart field named 'file'".into(),
        ));
    }

    let original_name = field.file_name().unwrap_or("unnamed").to_string();

    // Grab the MIME type the client declared for the file part.
    let mime_type = field
        .content_type()
        .map(|m| m.to_string())
        .unwrap_or_else(|| "application/octet-stream".to_string());

    // --- MIME whitelist check ----------------------------------------------------
    if !is_mime_allowed(&mime_type) {
        return Err(AppError::UnsupportedMediaType(format!(
            "File type '{mime_type}' is not allowed"
        )));
    }

    // --- read file bytes ---------------------------------------------------------
    let data = field
        .bytes()
        .await
        .map_err(|e| AppError::BadRequest(format!("Failed to read file data: {e}")))?;

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
            mime_guess::get_mime_extensions(&mime_type.parse().unwrap_or(mime::APPLICATION_OCTET_STREAM))
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
    let metadata = FileMetadata {
        file_id: file_id.clone(),
        original_name: original_name.clone(),
        size: file_size as u64,
        mime_type: mime_type.clone(),
        extension: ext.clone(),
        created_at: chrono::Utc::now().timestamp(),
    };

    let meta_path = upload_dir.join(format!("{file_id}.meta.json"));
    let meta_json = serde_json::to_string_pretty(&metadata)
        .map_err(|e| AppError::Internal(format!("Failed to serialize metadata: {e}")))?;
    tokio::fs::write(&meta_path, meta_json)
        .await
        .map_err(|e| AppError::Internal(format!("Failed to save metadata: {e}")))?;

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
    let _user_id = auth.0;

    let upload_dir = state.config.data_dir.join("uploads");

    // --- read metadata sidecar ---------------------------------------------------
    let meta_path = upload_dir.join(format!("{file_id}.meta.json"));
    let meta_json = tokio::fs::read_to_string(&meta_path)
        .await
        .map_err(|_| AppError::NotFound("File not found".into()))?;

    let metadata: FileMetadata = serde_json::from_str(&meta_json)
        .map_err(|_| AppError::Internal("Corrupt file metadata".into()))?;

    // --- read file bytes ---------------------------------------------------------
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
        crate::auth::create_token_pair("test-user", "test")
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

        // In-memory SQLite pool so AppState is happy (runs migrations).
        let opts = sqlx::sqlite::SqliteConnectOptions::new()
            .filename(":memory:")
            .create_if_missing(true)
            .foreign_keys(true);
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(2)
            .connect_with(opts)
            .await
            .unwrap();
        sqlx::migrate!("db/migrations")
            .run(&pool)
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
