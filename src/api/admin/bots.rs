//! Admin bot CRUD endpoints — `/api/admin/bots`.
//!
//! A bot is a pair of rows: a `users` record with `is_bot = 1` (so it can
//! post messages, appear in member lists, etc.) and a `bots` record that
//! carries the Hermes API configuration. The user record's `username` is
//! the bot's `name` (the token used for `@mentions`); both unique
//! constraints apply, so duplicate creation surfaces as 409 Conflict via
//! `From<sqlx::Error>`.
//!
//! `api_key` is stored but never returned by any read/list endpoint.

use axum::{
    extract::{Path, State},
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use uuid::Uuid;

use crate::auth::admin::AdminAuthenticatedUser;
use crate::error::{created_response, ok_response, AppError};
use crate::AppState;

// ---------------------------------------------------------------------------
// Request / response types
// ---------------------------------------------------------------------------

/// Bot view returned by every read/list/create/update endpoint.
///
/// Deliberately omits `api_key` — that field is write-only.
#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct BotView {
    pub id: String,
    pub user_id: String,
    pub name: String,
    pub display_name: String,
    pub api_url: String,
    pub system_prompt: String,
    pub model: String,
    pub is_active: bool,
    pub created_at: i64,
    /// `users.username` for the bot's user row (== `name`). Joined so the
    /// admin UI can render the owning principal without a second query.
    pub username: String,
}

#[derive(Debug, Deserialize)]
pub struct CreateBotRequest {
    pub name: String,
    pub display_name: Option<String>,
    pub api_url: String,
    pub api_key: Option<String>,
    pub system_prompt: Option<String>,
    pub model: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateBotRequest {
    pub display_name: Option<String>,
    pub api_url: Option<String>,
    pub api_key: Option<String>,
    pub system_prompt: Option<String>,
    pub model: Option<String>,
    pub is_active: Option<bool>,
}

const SQL_COLUMNS: &str = "b.id, b.user_id, b.name, b.display_name, b.api_url, \
     b.system_prompt, b.model, b.is_active, b.created_at, u.username";

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

/// POST /api/admin/bots
///
/// Creates the bot user (is_bot=1, password_hash='') and the bots row.
/// Either UNIQUE constraint (users.username or bots.name) yields 409.
pub async fn create_bot(
    _admin: AdminAuthenticatedUser,
    State(state): State<Arc<AppState>>,
    Json(body): Json<CreateBotRequest>,
) -> Result<(axum::http::StatusCode, Json<BotView>), AppError> {
    let name = body.name.trim().to_string();
    if name.is_empty() || name.len() > 64 {
        return Err(AppError::BadRequest(
            "name must be 1-64 characters".to_string(),
        ));
    }
    let api_url = body.api_url.trim().to_string();
    if api_url.is_empty() {
        return Err(AppError::BadRequest(
            "api_url must not be empty".to_string(),
        ));
    }

    let display_name = body.display_name.as_deref().unwrap_or("").trim().to_string();
    let api_key = body.api_key.as_deref().unwrap_or("").to_string();
    let system_prompt = body.system_prompt.as_deref().unwrap_or("").to_string();
    let model = body.model.as_deref().unwrap_or("hermes").trim().to_string();

    let user_id = Uuid::now_v7().to_string();
    let bot_id = Uuid::now_v7().to_string();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|e| AppError::Internal(format!("SystemTime before UNIX_EPOCH: {e}")))?
        .as_secs() as i64;

    // User row first — bots.user_id references users.id.
    sqlx::query(
        "INSERT INTO users (id, username, display_name, password_hash, is_bot) \
         VALUES (?, ?, ?, '', 1)",
    )
    .bind(&user_id)
    .bind(&name)
    .bind(&display_name)
    .execute(&state.pool)
    .await?;

    sqlx::query(
        "INSERT INTO bots \
         (id, user_id, name, display_name, api_url, api_key, system_prompt, model, is_active, created_at) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, 1, ?)",
    )
    .bind(&bot_id)
    .bind(&user_id)
    .bind(&name)
    .bind(&display_name)
    .bind(&api_url)
    .bind(&api_key)
    .bind(&system_prompt)
    .bind(&model)
    .bind(now)
    .execute(&state.pool)
    .await?;

    let details = serde_json::to_string(&serde_json::json!({
        "name": name,
        "user_id": user_id,
    }))
    .ok();
    let _ = super::audit(
        &state.pool,
        "bot.create",
        Some("bot"),
        Some(&bot_id),
        details.as_deref(),
    )
    .await;

    let view = sqlx::query_as::<_, BotView>(&format!(
        "SELECT {SQL_COLUMNS} FROM bots b JOIN users u ON b.user_id = u.id WHERE b.id = ?"
    ))
    .bind(&bot_id)
    .fetch_one(&state.pool)
    .await?;

    created_response(view)
}

/// GET /api/admin/bots
pub async fn list_bots(
    _admin: AdminAuthenticatedUser,
    State(state): State<Arc<AppState>>,
) -> Result<Json<Vec<BotView>>, AppError> {
    let bots = sqlx::query_as::<_, BotView>(&format!(
        "SELECT {SQL_COLUMNS} FROM bots b \
         JOIN users u ON b.user_id = u.id \
         ORDER BY b.created_at DESC, b.id"
    ))
    .fetch_all(&state.pool)
    .await?;
    Ok(Json(bots))
}

/// GET /api/admin/bots/{id}
pub async fn get_bot(
    _admin: AdminAuthenticatedUser,
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<BotView>, AppError> {
    let view = sqlx::query_as::<_, BotView>(&format!(
        "SELECT {SQL_COLUMNS} FROM bots b JOIN users u ON b.user_id = u.id WHERE b.id = ?"
    ))
    .bind(&id)
    .fetch_optional(&state.pool)
    .await?;
    view.map(Json).ok_or_else(|| AppError::NotFound("Bot not found".to_string()))
}

/// PATCH /api/admin/bots/{id}
///
/// Partial update — each provided field issues its own UPDATE. The first
/// UPDATE serves as the existence check (0 rows affected → 404).
pub async fn update_bot(
    _admin: AdminAuthenticatedUser,
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(body): Json<UpdateBotRequest>,
) -> Result<Json<BotView>, AppError> {
    let exists: Option<i64> = sqlx::query_scalar("SELECT 1 FROM bots WHERE id = ?")
        .bind(&id)
        .fetch_optional(&state.pool)
        .await?;
    if exists.is_none() {
        return Err(AppError::NotFound("Bot not found".to_string()));
    }

    if let Some(ref name) = body.display_name {
        let trimmed = name.trim();
        if trimmed.len() > 64 {
            return Err(AppError::BadRequest(
                "display_name must be 64 characters or fewer".into(),
            ));
        }
        sqlx::query("UPDATE bots SET display_name = ? WHERE id = ?")
            .bind(trimmed)
            .bind(&id)
            .execute(&state.pool)
            .await?;
    }
    if let Some(ref url) = body.api_url {
        let trimmed = url.trim();
        if trimmed.is_empty() {
            return Err(AppError::BadRequest("api_url must not be empty".into()));
        }
        sqlx::query("UPDATE bots SET api_url = ? WHERE id = ?")
            .bind(trimmed)
            .bind(&id)
            .execute(&state.pool)
            .await?;
    }
    if let Some(ref key) = body.api_key {
        sqlx::query("UPDATE bots SET api_key = ? WHERE id = ?")
            .bind(key)
            .bind(&id)
            .execute(&state.pool)
            .await?;
    }
    if let Some(ref prompt) = body.system_prompt {
        sqlx::query("UPDATE bots SET system_prompt = ? WHERE id = ?")
            .bind(prompt)
            .bind(&id)
            .execute(&state.pool)
            .await?;
    }
    if let Some(ref model) = body.model {
        let trimmed = model.trim();
        if trimmed.is_empty() {
            return Err(AppError::BadRequest("model must not be empty".into()));
        }
        sqlx::query("UPDATE bots SET model = ? WHERE id = ?")
            .bind(trimmed)
            .bind(&id)
            .execute(&state.pool)
            .await?;
    }
    if let Some(is_active) = body.is_active {
        sqlx::query("UPDATE bots SET is_active = ? WHERE id = ?")
            .bind(is_active)
            .bind(&id)
            .execute(&state.pool)
            .await?;
    }

    let _ = super::audit(
        &state.pool,
        "bot.update",
        Some("bot"),
        Some(&id),
        None,
    )
    .await;

    let view = sqlx::query_as::<_, BotView>(&format!(
        "SELECT {SQL_COLUMNS} FROM bots b JOIN users u ON b.user_id = u.id WHERE b.id = ?"
    ))
    .bind(&id)
    .fetch_one(&state.pool)
    .await?;

    Ok(Json(view))
}

/// DELETE /api/admin/bots/{id}
///
/// Removes the bots row and the bot's user row. Bots first (FK), then
/// the user. Returns 200 with an empty object per the T2 contract.
pub async fn delete_bot(
    _admin: AdminAuthenticatedUser,
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<(axum::http::StatusCode, Json<serde_json::Value>), AppError> {
    // Fetch user_id before deleting so we can clean up the users row.
    let user_id: Option<String> =
        sqlx::query_scalar("SELECT user_id FROM bots WHERE id = ?")
            .bind(&id)
            .fetch_optional(&state.pool)
            .await?;
    let user_id = user_id
        .ok_or_else(|| AppError::NotFound("Bot not found".to_string()))?;

    sqlx::query("DELETE FROM bots WHERE id = ?")
        .bind(&id)
        .execute(&state.pool)
        .await?;
    sqlx::query("DELETE FROM users WHERE id = ?")
        .bind(&user_id)
        .execute(&state.pool)
        .await?;

    let _ = super::audit(
        &state.pool,
        "bot.delete",
        Some("bot"),
        Some(&id),
        None,
    )
    .await;

    ok_response(serde_json::json!({}))
}

/// POST /api/admin/bots/{id}/test
///
/// Sends a `ping` chat-completion request to the bot's Hermes API and
/// reports whether the round-trip succeeded. Always returns 200 with an
/// `ok` field — connection failures, timeouts, non-2xx responses, and
/// parse errors all surface as `{ ok: false, error }` so the caller can
/// treat this as a pure boolean check without parsing HTTP status codes.
///
/// The 10-second timeout is shorter than `HermesClient`'s 60s default so
/// the admin UI gets quick feedback.
pub async fn test_bot(
    _admin: AdminAuthenticatedUser,
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, AppError> {
    let row: Option<(String, String, String)> =
        sqlx::query_as("SELECT api_url, api_key, model FROM bots WHERE id = ?")
            .bind(&id)
            .fetch_optional(&state.pool)
            .await?;
    let (api_url, api_key, model) = row
        .ok_or_else(|| AppError::NotFound("Bot not found".to_string()))?;

    let base = api_url.trim_end_matches('/').trim_end_matches("/v1");
    let url = format!("{}/v1/chat/completions", base);
    let body = serde_json::json!({
        "model": model,
        "messages": [{"role": "user", "content": "ping"}],
        "stream": false
    });

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|e| AppError::Internal(format!("HTTP client build failed: {e}")))?;

    let mut req = client
        .post(&url)
        .json(&body)
        .header("X-Hermes-Session-Id", "test");
    if !api_key.is_empty() {
        req = req.header("Authorization", format!("Bearer {api_key}"));
    }

    match req.send().await {
        Ok(resp) => {
            let status = resp.status();
            if !status.is_success() {
                let text = resp.text().await.unwrap_or_default();
                return Ok(Json(serde_json::json!({
                    "ok": false,
                    "error": format!("Hermes returned {status}: {}", truncate_preview(&text, 200))
                })));
            }
            let text = resp.text().await.unwrap_or_default();
            // Prefer the parsed assistant content; fall back to a raw-body
            // preview so the admin still sees something useful if Hermes
            // returns a non-OpenAI shape.
            let preview = serde_json::from_str::<serde_json::Value>(&text)
                .ok()
                .and_then(|v| {
                    v["choices"][0]["message"]["content"]
                        .as_str()
                        .map(|s| s.to_string())
                })
                .unwrap_or_else(|| truncate_preview(&text, 200));
            Ok(Json(serde_json::json!({
                "ok": true,
                "response": preview
            })))
        }
        Err(e) => {
            let msg = if e.is_timeout() {
                "Request timed out".to_string()
            } else if e.is_connect() {
                format!("Connection failed: {e}")
            } else {
                e.to_string()
            };
            Ok(Json(serde_json::json!({
                "ok": false,
                "error": msg
            })))
        }
    }
}

/// Truncate `s` to at most `max` chars (counting Unicode scalar values),
/// appending an ellipsis when truncation occurs. Used to keep test
/// responses short enough for the admin UI toast.
fn truncate_preview(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let mut truncated: String = s.chars().take(max).collect();
        truncated.push('…');
        truncated
    }
}

// ---------------------------------------------------------------------------
// Router
// ---------------------------------------------------------------------------

pub fn bot_routes() -> Router<Arc<AppState>> {
    Router::new()
        .route("/", post(create_bot).get(list_bots))
        .route("/{id}", get(get_bot).patch(update_bot).delete(delete_bot))
        .route("/{id}/test", post(test_bot))
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::admin::create_admin_token_pair;
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use serde_json::{json, Value};
    use tower::ServiceExt;

    const SECRET: &str = "test-secret";

    async fn setup_pool() -> sqlx::SqlitePool {
        let pool = sqlx::SqlitePool::connect(":memory:").await.unwrap();
        sqlx::migrate!("db/migrations").run(&pool).await.unwrap();
        pool
    }

    fn admin_enabled_config() -> crate::AppConfig {
        let mut config = crate::AppConfig::test_default();
        config.admin_password_hash =
            crate::auth::hash_password("test-admin-pass").unwrap();
        config
    }

    fn make_state(pool: sqlx::SqlitePool, config: crate::AppConfig) -> Arc<AppState> {
        Arc::new(AppState {
            pool,
            ws_pool: Arc::new(crate::ws::ConnectionPool::new()),
            config,
        })
    }

    fn build_app(state: Arc<AppState>) -> Router {
        Router::new()
            .nest("/admin", crate::api::admin::admin_routes())
            .with_state(state)
    }

    async fn post_json_with_token(
        app: &mut Router,
        uri: &str,
        body: Value,
        token: &str,
    ) -> (StatusCode, Value) {
        let req = Request::builder()
            .method("POST")
            .uri(uri)
            .header("content-type", "application/json")
            .header("authorization", format!("Bearer {token}"))
            .body(Body::from(body.to_string()))
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        let status = resp.status();
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let val: Value = serde_json::from_slice(&bytes).unwrap_or(json!({}));
        (status, val)
    }

    async fn get_with_token(app: &mut Router, uri: &str, token: &str) -> (StatusCode, Value) {
        let req = Request::builder()
            .method("GET")
            .uri(uri)
            .header("authorization", format!("Bearer {token}"))
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        let status = resp.status();
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let val: Value = serde_json::from_slice(&bytes).unwrap_or(json!({}));
        (status, val)
    }

    async fn patch_json_with_token(
        app: &mut Router,
        uri: &str,
        body: Value,
        token: &str,
    ) -> (StatusCode, Value) {
        let req = Request::builder()
            .method("PATCH")
            .uri(uri)
            .header("content-type", "application/json")
            .header("authorization", format!("Bearer {token}"))
            .body(Body::from(body.to_string()))
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        let status = resp.status();
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let val: Value = serde_json::from_slice(&bytes).unwrap_or(json!({}));
        (status, val)
    }

    async fn delete_with_token(
        app: &mut Router,
        uri: &str,
        token: &str,
    ) -> (StatusCode, Value) {
        let req = Request::builder()
            .method("DELETE")
            .uri(uri)
            .header("authorization", format!("Bearer {token}"))
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        let status = resp.status();
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let val: Value = serde_json::from_slice(&bytes).unwrap_or(json!({}));
        (status, val)
    }

    fn valid_create_body(name: &str) -> Value {
        json!({
            "name": name,
            "display_name": "Hermes",
            "api_url": "http://localhost:9090",
            "api_key": "sekret",
            "system_prompt": "You are helpful.",
            "model": "hermes"
        })
    }

    // -----------------------------------------------------------------------
    // 1. Create
    // -----------------------------------------------------------------------

    /// Given: no bot exists.
    /// When:  POST /admin/bots with a valid body.
    /// Then:  201 Created; response has all fields, no api_key, and the
    ///        underlying user row carries is_bot=1.
    #[tokio::test]
    async fn test_create_bot_success() {
        let pool = setup_pool().await;
        let mut app = build_app(make_state(pool.clone(), admin_enabled_config()));
        let pair = create_admin_token_pair(SECRET).unwrap();

        let (status, body) =
            post_json_with_token(&mut app, "/admin/bots", valid_create_body("hermes"), &pair.access_token)
                .await;

        assert_eq!(status, StatusCode::CREATED);
        assert_eq!(body["name"], "hermes");
        assert_eq!(body["display_name"], "Hermes");
        assert_eq!(body["api_url"], "http://localhost:9090");
        assert_eq!(body["model"], "hermes");
        assert_eq!(body["is_active"], true);
        assert_eq!(body["username"], "hermes");
        assert!(body.get("api_key").is_none(), "api_key must not be returned");

        // User row is_bot=1.
        let is_bot: i64 = sqlx::query_scalar("SELECT is_bot FROM users WHERE username = 'hermes'")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(is_bot, 1);
    }

    // -----------------------------------------------------------------------
    // 2. List
    // -----------------------------------------------------------------------

    /// Given: two bots exist.
    /// When:  GET /admin/bots.
    /// Then:  200 OK returning both; ordered by created_at DESC; no api_key.
    #[tokio::test]
    async fn test_list_bots() {
        let pool = setup_pool().await;
        let mut app = build_app(make_state(pool.clone(), admin_enabled_config()));
        let pair = create_admin_token_pair(SECRET).unwrap();

        // Seed two bots sequentially so created_at differs (1s resolution).
        let _ = post_json_with_token(
            &mut app,
            "/admin/bots",
            valid_create_body("alpha"),
            &pair.access_token,
        )
        .await;
        // created_at is unix-seconds; sleep 1s to guarantee ordering.
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
        let _ = post_json_with_token(
            &mut app,
            "/admin/bots",
            valid_create_body("beta"),
            &pair.access_token,
        )
        .await;

        let (status, body) = get_with_token(&mut app, "/admin/bots", &pair.access_token).await;
        assert_eq!(status, StatusCode::OK);
        let arr = body.as_array().expect("array");
        assert_eq!(arr.len(), 2, "expected 2 bots, got {arr:?}");
        assert_eq!(arr[0]["name"], "beta", "newest first");
        assert_eq!(arr[1]["name"], "alpha");
        for b in arr {
            assert!(b.get("api_key").is_none());
        }
    }

    // -----------------------------------------------------------------------
    // 3. Get by id
    // -----------------------------------------------------------------------

    /// Given: a bot exists.
    /// When:  GET /admin/bots/{id}.
    /// Then:  200 OK; body matches the seeded config; no api_key.
    #[tokio::test]
    async fn test_get_bot_by_id() {
        let pool = setup_pool().await;
        let mut app = build_app(make_state(pool.clone(), admin_enabled_config()));
        let pair = create_admin_token_pair(SECRET).unwrap();

        let (_, created) =
            post_json_with_token(&mut app, "/admin/bots", valid_create_body("hermes"), &pair.access_token)
                .await;
        let id = created["id"].as_str().unwrap().to_string();

        let (status, body) =
            get_with_token(&mut app, &format!("/admin/bots/{id}"), &pair.access_token).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["id"], id);
        assert_eq!(body["name"], "hermes");
        assert!(body.get("api_key").is_none());
    }

    /// Given: no bot with the requested id.
    /// When:  GET /admin/bots/missing.
    /// Then:  404 Not Found.
    #[tokio::test]
    async fn test_get_bot_not_found() {
        let pool = setup_pool().await;
        let mut app = build_app(make_state(pool, admin_enabled_config()));
        let pair = create_admin_token_pair(SECRET).unwrap();

        let (status, body) =
            get_with_token(&mut app, "/admin/bots/does-not-exist", &pair.access_token).await;
        assert_eq!(status, StatusCode::NOT_FOUND);
        assert_eq!(body["error"]["code"], "NOT_FOUND");
    }

    // -----------------------------------------------------------------------
    // 4. Update
    // -----------------------------------------------------------------------

    /// Given: a bot exists.
    /// When:  PATCH with new display_name, model, is_active.
    /// Then:  200 OK; returned body reflects the new values; DB persists them.
    #[tokio::test]
    async fn test_update_bot_fields() {
        let pool = setup_pool().await;
        let mut app = build_app(make_state(pool.clone(), admin_enabled_config()));
        let pair = create_admin_token_pair(SECRET).unwrap();

        let (_, created) =
            post_json_with_token(&mut app, "/admin/bots", valid_create_body("hermes"), &pair.access_token)
                .await;
        let id = created["id"].as_str().unwrap().to_string();

        let (status, body) = patch_json_with_token(
            &mut app,
            &format!("/admin/bots/{id}"),
            json!({
                "display_name": "Hermes v2",
                "model": "hermes-pro",
                "is_active": false,
                "api_url": "http://new-host:9090"
            }),
            &pair.access_token,
        )
        .await;

        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["display_name"], "Hermes v2");
        assert_eq!(body["model"], "hermes-pro");
        assert_eq!(body["is_active"], false);
        assert_eq!(body["api_url"], "http://new-host:9090");
        assert!(body.get("api_key").is_none());

        // DB persisted.
        let (db_active,): (bool,) =
            sqlx::query_as("SELECT is_active FROM bots WHERE id = ?")
                .bind(&id)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert!(!db_active);
    }

    /// Given: updating a non-existent bot.
    /// When:  PATCH /admin/bots/missing.
    /// Then:  404 Not Found.
    #[tokio::test]
    async fn test_update_bot_not_found() {
        let pool = setup_pool().await;
        let mut app = build_app(make_state(pool, admin_enabled_config()));
        let pair = create_admin_token_pair(SECRET).unwrap();

        let (status, body) = patch_json_with_token(
            &mut app,
            "/admin/bots/missing",
            json!({"display_name": "x"}),
            &pair.access_token,
        )
        .await;
        assert_eq!(status, StatusCode::NOT_FOUND);
        assert_eq!(body["error"]["code"], "NOT_FOUND");
    }

    // -----------------------------------------------------------------------
    // 5. Delete
    // -----------------------------------------------------------------------

    /// Given: a bot exists.
    /// When:  DELETE /admin/bots/{id}.
    /// Then:  200 OK; both bots and users rows are gone; subsequent GET 404s.
    #[tokio::test]
    async fn test_delete_bot_removes_user_and_bot_row() {
        let pool = setup_pool().await;
        let mut app = build_app(make_state(pool.clone(), admin_enabled_config()));
        let pair = create_admin_token_pair(SECRET).unwrap();

        let (_, created) =
            post_json_with_token(&mut app, "/admin/bots", valid_create_body("hermes"), &pair.access_token)
                .await;
        let id = created["id"].as_str().unwrap().to_string();
        let user_id = created["user_id"].as_str().unwrap().to_string();

        let (status, _body) =
            delete_with_token(&mut app, &format!("/admin/bots/{id}"), &pair.access_token).await;
        assert_eq!(status, StatusCode::OK);

        let bot_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM bots WHERE id = ?")
            .bind(&id)
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(bot_count, 0, "bots row must be deleted");

        let user_count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM users WHERE id = ?")
                .bind(&user_id)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(user_count, 0, "bot user row must be deleted");

        // Subsequent GET → 404.
        let (status, _) =
            get_with_token(&mut app, &format!("/admin/bots/{id}"), &pair.access_token).await;
        assert_eq!(status, StatusCode::NOT_FOUND);
    }

    /// Given: deleting a non-existent bot.
    /// When:  DELETE /admin/bots/missing.
    /// Then:  404 Not Found.
    #[tokio::test]
    async fn test_delete_bot_not_found() {
        let pool = setup_pool().await;
        let mut app = build_app(make_state(pool, admin_enabled_config()));
        let pair = create_admin_token_pair(SECRET).unwrap();

        let (status, body) =
            delete_with_token(&mut app, "/admin/bots/missing", &pair.access_token).await;
        assert_eq!(status, StatusCode::NOT_FOUND);
        assert_eq!(body["error"]["code"], "NOT_FOUND");
    }

    // -----------------------------------------------------------------------
    // 6. Duplicate name → 409 Conflict
    // -----------------------------------------------------------------------

    /// Given: a bot named "hermes" exists.
    /// When:  POST /admin/bots with the same name.
    /// Then:  409 Conflict (either users.username or bots.name UNIQUE).
    #[tokio::test]
    async fn test_create_bot_duplicate_name_conflict() {
        let pool = setup_pool().await;
        let mut app = build_app(make_state(pool, admin_enabled_config()));
        let pair = create_admin_token_pair(SECRET).unwrap();

        let (first_status, _) =
            post_json_with_token(&mut app, "/admin/bots", valid_create_body("hermes"), &pair.access_token)
                .await;
        assert_eq!(first_status, StatusCode::CREATED);

        let (status, body) =
            post_json_with_token(&mut app, "/admin/bots", valid_create_body("hermes"), &pair.access_token)
                .await;
        assert_eq!(status, StatusCode::CONFLICT);
        assert_eq!(body["error"]["code"], "CONFLICT");
    }

    // -----------------------------------------------------------------------
    // 7. Non-admin access → 401
    //    (AdminAuthenticatedUser rejects both missing-token and user-token
    //    requests with 401; the codebase pattern, established by the
    //    invite-codes endpoints, is "non-admin ⇒ UNAUTHORIZED", not 403.)
    // -----------------------------------------------------------------------

    /// Given: bot endpoints require admin authentication.
    /// When:  GET /admin/bots with no token, then GET with a user token.
    /// Then:  Both 401 Unauthorized.
    #[tokio::test]
    async fn test_bot_endpoints_require_admin() {
        let pool = setup_pool().await;
        let app = build_app(make_state(pool.clone(), admin_enabled_config()));

        let req = Request::builder()
            .method("GET")
            .uri("/admin/bots")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);

        let mut app = build_app(make_state(pool, admin_enabled_config()));
        let user_pair =
            crate::auth::create_token_pair("user-1", SECRET, 0).unwrap();
        let (status, body) =
            get_with_token(&mut app, "/admin/bots", &user_pair.access_token).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);
        assert_eq!(body["error"]["code"], "UNAUTHORIZED");
    }

    // -----------------------------------------------------------------------
    // Misc: validation + audit
    // -----------------------------------------------------------------------

    /// Given: name is required and must be 1-64 chars.
    /// When:  POST with an empty name.
    /// Then:  400 Bad Request.
    #[tokio::test]
    async fn test_create_bot_empty_name_rejected() {
        let pool = setup_pool().await;
        let mut app = build_app(make_state(pool, admin_enabled_config()));
        let pair = create_admin_token_pair(SECRET).unwrap();

        let body = json!({
            "name": "",
            "api_url": "http://localhost:9090"
        });
        let (status, body) =
            post_json_with_token(&mut app, "/admin/bots", body, &pair.access_token).await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(body["error"]["code"], "BAD_REQUEST");
    }

    /// Given: api_url is required.
    /// When:  POST with empty api_url.
    /// Then:  400 Bad Request.
    #[tokio::test]
    async fn test_create_bot_empty_api_url_rejected() {
        let pool = setup_pool().await;
        let mut app = build_app(make_state(pool, admin_enabled_config()));
        let pair = create_admin_token_pair(SECRET).unwrap();

        let body = json!({
            "name": "hermes",
            "api_url": ""
        });
        let (status, body) =
            post_json_with_token(&mut app, "/admin/bots", body, &pair.access_token).await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(body["error"]["code"], "BAD_REQUEST");
    }

    // -----------------------------------------------------------------------
    // 8. Test endpoint
    // -----------------------------------------------------------------------

    /// Given: a bot whose `api_url` points at an in-process mock that
    ///        returns an OpenAI-compatible chat completion.
    /// When:  POST /admin/bots/{id}/test.
    /// Then:  200 OK with `{ ok: true, response: "pong" }`.
    #[tokio::test]
    async fn test_test_bot_success() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let mock_addr = listener.local_addr().unwrap();
        let mock_url = format!("http://{mock_addr}");
        let server = tokio::spawn(async move {
            use tokio::io::{AsyncReadExt, AsyncWriteExt};
            loop {
                let (mut sock, _) = match listener.accept().await {
                    Ok(p) => p,
                    Err(_) => break,
                };
                tokio::spawn(async move {
                    let mut buf = vec![0u8; 4096];
                    let _ = sock.read(&mut buf).await;
                    let body = serde_json::json!({
                        "choices": [{"message": {"content": "pong"}}]
                    });
                    let resp = format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                        body.to_string().len(),
                        body
                    );
                    let _ = sock.write_all(resp.as_bytes()).await;
                    let _ = sock.shutdown().await;
                });
            }
        });

        let pool = setup_pool().await;
        let mut app = build_app(make_state(pool.clone(), admin_enabled_config()));
        let pair = create_admin_token_pair(SECRET).unwrap();

        let create_body = json!({
            "name": "hermes",
            "display_name": "Hermes",
            "api_url": mock_url,
            "api_key": "sekret",
            "model": "hermes"
        });
        let (_, created) =
            post_json_with_token(&mut app, "/admin/bots", create_body, &pair.access_token).await;
        let id = created["id"].as_str().unwrap().to_string();

        let (status, body) = post_json_with_token(
            &mut app,
            &format!("/admin/bots/{id}/test"),
            json!({}),
            &pair.access_token,
        )
        .await;

        server.abort();
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["ok"], true);
        assert_eq!(body["response"], "pong");
    }

    /// Given: a bot whose `api_url` points at a port with no listener.
    /// When:  POST /admin/bots/{id}/test.
    /// Then:  200 OK with `{ ok: false, error }` (connection refused).
    #[tokio::test]
    async fn test_test_bot_connection_failed() {
        // Reserve a port then drop the listener — connecting typically
        // yields ECONNREFUSED on the same host.
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        drop(listener);
        let dead_url = format!("http://127.0.0.1:{port}");

        let pool = setup_pool().await;
        let mut app = build_app(make_state(pool, admin_enabled_config()));
        let pair = create_admin_token_pair(SECRET).unwrap();

        let (_, created) = post_json_with_token(
            &mut app,
            "/admin/bots",
            json!({ "name": "dead", "api_url": dead_url }),
            &pair.access_token,
        )
        .await;
        let id = created["id"].as_str().unwrap().to_string();

        let (status, body) = post_json_with_token(
            &mut app,
            &format!("/admin/bots/{id}/test"),
            json!({}),
            &pair.access_token,
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["ok"], false);
        assert!(
            body.get("error").and_then(|e| e.as_str()).is_some(),
            "expected error message, got {body}"
        );
    }

    /// Given: a bot whose `api_url` accepts TCP connections but never
    ///        writes a response.
    /// When:  POST /admin/bots/{id}/test (10s timeout).
    /// Then:  200 OK with `{ ok: false, error: "Request timed out" }`.
    #[tokio::test]
    async fn test_test_bot_timeout() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let mock_addr = listener.local_addr().unwrap();
        let mock_url = format!("http://{mock_addr}");
        let server = tokio::spawn(async move {
            loop {
                let (mut sock, _) = match listener.accept().await {
                    Ok(p) => p,
                    Err(_) => break,
                };
                // Drain the request but never respond — forces client timeout.
                tokio::spawn(async move {
                    use tokio::io::AsyncReadExt;
                    let mut buf = vec![0u8; 4096];
                    let _ = sock.read(&mut buf).await;
                    std::future::pending::<()>().await;
                });
            }
        });

        let pool = setup_pool().await;
        let mut app = build_app(make_state(pool, admin_enabled_config()));
        let pair = create_admin_token_pair(SECRET).unwrap();

        let (_, created) = post_json_with_token(
            &mut app,
            "/admin/bots",
            json!({ "name": "slow", "api_url": mock_url }),
            &pair.access_token,
        )
        .await;
        let id = created["id"].as_str().unwrap().to_string();

        // Bound the whole test to avoid hanging the suite if the timeout
        // ever regresses.
        let outcome = tokio::time::timeout(
            std::time::Duration::from_secs(15),
            post_json_with_token(
                &mut app,
                &format!("/admin/bots/{id}/test"),
                json!({}),
                &pair.access_token,
            ),
        )
        .await;
        server.abort();

        let (status, body) = outcome.expect("test did not complete within 15s");
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["ok"], false);
        assert!(
            body["error"].as_str().unwrap_or("").contains("timed out"),
            "expected timeout error, got {}",
            body["error"]
        );
    }

    /// Given: no bot with the requested id.
    /// When:  POST /admin/bots/missing/test.
    /// Then:  404 Not Found (test_bot surfaces a missing config row as 404
    ///        rather than ok:false, since the bot itself is the resource).
    #[tokio::test]
    async fn test_test_bot_not_found() {
        let pool = setup_pool().await;
        let mut app = build_app(make_state(pool, admin_enabled_config()));
        let pair = create_admin_token_pair(SECRET).unwrap();

        let (status, body) = post_json_with_token(
            &mut app,
            "/admin/bots/does-not-exist/test",
            json!({}),
            &pair.access_token,
        )
        .await;
        assert_eq!(status, StatusCode::NOT_FOUND);
        assert_eq!(body["error"]["code"], "NOT_FOUND");
    }

    /// Given: create, update, delete each write an audit row.
    /// When:  Performing all three on the same bot.
    /// Then:  admin_audit_logs has 3 rows with matching action/target_type.
    #[tokio::test]
    async fn test_bot_audit_trail() {
        let pool = setup_pool().await;
        let mut app = build_app(make_state(pool.clone(), admin_enabled_config()));
        let pair = create_admin_token_pair(SECRET).unwrap();

        let (_, created) =
            post_json_with_token(&mut app, "/admin/bots", valid_create_body("hermes"), &pair.access_token)
                .await;
        let id = created["id"].as_str().unwrap().to_string();

        let _ = patch_json_with_token(
            &mut app,
            &format!("/admin/bots/{id}"),
            json!({"display_name": "x"}),
            &pair.access_token,
        )
        .await;

        let _ = delete_with_token(&mut app, &format!("/admin/bots/{id}"), &pair.access_token).await;

        for action in ["bot.create", "bot.update", "bot.delete"] {
            let (count,): (i64,) = sqlx::query_as(
                "SELECT COUNT(*) FROM admin_audit_logs WHERE action = ? AND target_type = 'bot' AND target_id = ?",
            )
            .bind(action)
            .bind(&id)
            .fetch_one(&pool)
            .await
            .unwrap();
            assert_eq!(count, 1, "audit row for {action} must exist");
        }
    }
}
