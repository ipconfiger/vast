//! Integration test: File upload/download lifecycle
//!   upload PNG → download with correct MIME → disallowed type rejected → 404
//!   unauthenticated upload/download rejected with 401
use axum::{
    body::Body,
    http::{header, Method, Request, StatusCode},
    Router,
};
use im_server::{AppConfig, AppState, TlsMode};
use serde_json::{json, Value};
use std::sync::Arc;
use tower::ServiceExt;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn ensure_env() {
    static ENV: std::sync::OnceLock<()> = std::sync::OnceLock::new();
    ENV.get_or_init(|| unsafe {
        std::env::set_var("JWT_SECRET", "integration-test-secret");
    });
}

async fn setup_pool() -> sqlx::SqlitePool {
    let pool = sqlx::SqlitePool::connect("sqlite::memory:")
        .await
        .expect("Failed to create in-memory DB");
    sqlx::migrate!("db/migrations")
        .run(&pool)
        .await
        .expect("Failed to run migrations");
    sqlx::query(
        "INSERT OR IGNORE INTO invite_codes (code, max_uses, is_active) VALUES ('E2ETEST', 1000, 1)",
    )
    .execute(&pool)
    .await
    .expect("Failed to seed invite code");
    pool
}

/// Build a test app that stores files in a unique temp directory.
fn build_app_with_tmpdir(pool: sqlx::SqlitePool, tmp_dir: std::path::PathBuf) -> Router {
    let state = Arc::new(AppState {
        pool,
        ws_pool: Arc::new(im_server::ws::ConnectionPool::new()),
        config: AppConfig {
            data_dir: tmp_dir,
            jwt_secret: "integration-test-secret".to_string(),
            invite_code: "E2ETEST".to_string(),
            tls_mode: TlsMode::None,
        },
    });
    im_server::api_routes().with_state(state)
}

/// Minimal valid 1x1 pixel PNG bytes.
fn png_bytes() -> Vec<u8> {
    vec![
        0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, // PNG signature
        0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44, 0x52, // IHDR chunk
        0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01,
        0x08, 0x02, 0x00, 0x00, 0x00, 0x90, 0x77, 0x53,
        0xDE, 0x00, 0x00, 0x00, 0x0C, 0x49, 0x44, 0x41, // IDAT chunk
        0x54, 0x08, 0xD7, 0x63, 0x60, 0x60, 0x60, 0x00,
        0x00, 0x00, 0x04, 0x00, 0x01, 0x27, 0x34, 0x27,
        0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4E, 0x44, // IEND chunk
        0xAE, 0x42, 0x60, 0x82,
    ]
}

/// Build a multipart/form-data request body and content-type header.
fn multipart_body(
    field_name: &str,
    file_name: &str,
    content_type: &str,
    data: &[u8],
) -> (Body, String) {
    let boundary = "testboundary42";
    let mut body = Vec::new();

    body.extend_from_slice(format!("--{boundary}\r\n").as_bytes());
    body.extend_from_slice(
        format!(
            "Content-Disposition: form-data; name=\"{field_name}\"; filename=\"{file_name}\"\r\n"
        )
        .as_bytes(),
    );
    body.extend_from_slice(format!("Content-Type: {content_type}\r\n").as_bytes());
    body.extend_from_slice(b"\r\n");
    body.extend_from_slice(data);
    body.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());

    let content_type = format!("multipart/form-data; boundary={boundary}");
    (Body::from(body), content_type)
}

/// Create a unique temp directory for file storage.
fn tmp_upload_dir() -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("vast-fl-test-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(dir.join("uploads")).unwrap();
    dir
}

async fn register_token(app: &mut Router, username: &str) -> String {
    let req = Request::builder()
        .method(Method::POST)
        .uri("/auth/register")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(
            json!({
                "username": username,
                "password": "FileTest12345",
                "invite_code": "E2ETEST"
            })
            .to_string(),
        ))
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    let status = resp.status();
    let body_bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    let val: Value = serde_json::from_slice(&body_bytes).unwrap_or_default();
    assert_eq!(status, StatusCode::CREATED, "register failed: {val}");
    val["access_token"].as_str().unwrap().to_string()
}

// ---------------------------------------------------------------------------
// File Flow
// ---------------------------------------------------------------------------

#[tokio::test]
async fn file_upload_download_integrity() {
    ensure_env();
    let pool = setup_pool().await;
    let tmp_dir = tmp_upload_dir();
    let mut app = build_app_with_tmpdir(pool, tmp_dir.clone());
    let token = register_token(&mut app, "fileuser1").await;

    let png_data = png_bytes();
    let (body, ct) = multipart_body("file", "test.png", "image/png", &png_data);

    // Upload
    let req = Request::builder()
        .method(Method::POST)
        .uri("/files/upload")
        .header(header::CONTENT_TYPE, &ct)
        .header(header::AUTHORIZATION, format!("Bearer {token}"))
        .body(body)
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED, "Upload should succeed");

    let body_bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
    let file_id = json["file_id"].as_str().unwrap().to_string();
    assert_eq!(json["original_name"], "test.png");
    assert_eq!(json["mime_type"], "image/png");
    assert!(json["size"].as_u64().unwrap() > 0);
    assert!(json["url"].is_string());

    // Download
    let req = Request::builder()
        .method(Method::GET)
        .uri(format!("/files/{file_id}"))
        .header(header::AUTHORIZATION, format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let content_type = resp
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    assert!(
        content_type.starts_with("image/"),
        "Content-Type should be image/*, got: {content_type}"
    );
}

#[tokio::test]
async fn file_disallowed_mime_rejected() {
    ensure_env();
    let pool = setup_pool().await;
    let tmp_dir = tmp_upload_dir();
    let mut app = build_app_with_tmpdir(pool, tmp_dir);
    let token = register_token(&mut app, "fileuser2").await;

    let (body, ct) =
        multipart_body("file", "evil.exe", "application/x-msdownload", &[0u8; 100]);

    let req = Request::builder()
        .method(Method::POST)
        .uri("/files/upload")
        .header(header::CONTENT_TYPE, &ct)
        .header(header::AUTHORIZATION, format!("Bearer {token}"))
        .body(body)
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(
        resp.status(),
        StatusCode::UNSUPPORTED_MEDIA_TYPE,
        "Disallowed MIME should be rejected"
    );
}

#[tokio::test]
async fn file_not_found_returns_404() {
    ensure_env();
    let pool = setup_pool().await;
    let tmp_dir = tmp_upload_dir();
    let mut app = build_app_with_tmpdir(pool, tmp_dir);
    let token = register_token(&mut app, "fileuser3").await;

    let req = Request::builder()
        .method(Method::GET)
        .uri("/files/00000000-0000-0000-0000-000000000000")
        .header(header::AUTHORIZATION, format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn file_upload_without_auth_returns_401() {
    ensure_env();
    let pool = setup_pool().await;
    let tmp_dir = tmp_upload_dir();
    let app = build_app_with_tmpdir(pool, tmp_dir);

    let (body, ct) = multipart_body("file", "test.png", "image/png", &png_bytes());

    let req = Request::builder()
        .method(Method::POST)
        .uri("/files/upload")
        .header(header::CONTENT_TYPE, &ct)
        .body(body)
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(
        resp.status(),
        StatusCode::UNAUTHORIZED,
        "Unauthenticated upload must be rejected"
    );
}

#[tokio::test]
async fn file_download_without_auth_returns_401() {
    ensure_env();
    let pool = setup_pool().await;
    let tmp_dir = tmp_upload_dir();
    let app = build_app_with_tmpdir(pool, tmp_dir);

    let req = Request::builder()
        .method(Method::GET)
        .uri("/files/00000000-0000-0000-0000-000000000000")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(
        resp.status(),
        StatusCode::UNAUTHORIZED,
        "Unauthenticated download must be rejected"
    );
}
