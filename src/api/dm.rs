use axum::{extract::State, http::StatusCode, Json, Router};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use std::sync::Arc;
use uuid::Uuid;

use crate::auth::middleware::AuthenticatedUser;
use crate::error::AppError;
use crate::AppState;

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct CreateDmRequest {
    pub user_ids: Vec<String>,
    #[serde(default)]
    pub name: Option<String>,
}

#[derive(Debug, Serialize, FromRow)]
pub struct DmChannel {
    pub id: String,
    pub name: String,
    pub description: String,
    pub owner_id: Option<String>,
    pub is_direct: bool,
    pub is_group_dm: bool,
    pub is_archived: bool,
    pub created_at: i64,
}

#[derive(Debug, Serialize)]
pub struct DmListResponse {
    pub channels: Vec<DmChannel>,
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

/// POST /api/dm
pub async fn create_dm(
    State(state): State<Arc<AppState>>,
    user: AuthenticatedUser,
    Json(req): Json<CreateDmRequest>,
) -> Result<(StatusCode, Json<DmChannel>), AppError> {
    if req.user_ids.len() < 2 {
        return Err(AppError::BadRequest(
            "At least 2 user_ids are required".to_string(),
        ));
    }

    let mut user_ids = req.user_ids;
    user_ids.sort();
    user_ids.dedup();

    if user_ids.len() < 2 {
        return Err(AppError::BadRequest(
            "At least 2 unique user_ids are required".to_string(),
        ));
    }

    if !user_ids.contains(&user.0) {
        return Err(AppError::BadRequest(
            "You must include yourself in the DM".to_string(),
        ));
    }

    for uid in &user_ids {
        let exists = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM users WHERE id = ?",
        )
        .bind(uid)
        .fetch_one(&state.pool)
        .await?;

        if exists == 0 {
            return Err(AppError::NotFound(format!("User not found: {uid}")));
        }
    }

    if user_ids.len() == 2 {
        let user_a = &user_ids[0];
        let user_b = &user_ids[1];

        let existing = sqlx::query_as::<_, DmChannel>(
            "SELECT c.id, c.name, c.description, c.owner_id, c.is_direct, \
                    c.is_group_dm, c.is_archived, c.created_at \
             FROM channels c \
             WHERE c.is_direct = 1 \
               AND c.is_group_dm = 0 \
               AND (SELECT COUNT(*) FROM channel_members cm WHERE cm.channel_id = c.id) = 2 \
               AND EXISTS (SELECT 1 FROM channel_members cm WHERE cm.channel_id = c.id AND cm.user_id = ?) \
               AND EXISTS (SELECT 1 FROM channel_members cm WHERE cm.channel_id = c.id AND cm.user_id = ?)",
        )
        .bind(user_a)
        .bind(user_b)
        .fetch_optional(&state.pool)
        .await?;

        if let Some(channel) = existing {
            return Ok((StatusCode::OK, Json(channel)));
        }
    }

    let is_group_dm = user_ids.len() > 2;
    let channel_id = Uuid::new_v4().to_string();
    let now = chrono::Utc::now().timestamp();

    let name = match req.name {
        Some(ref n) if !n.trim().is_empty() => n.trim().to_string(),
        _ => generate_dm_name(&state, &user_ids).await?,
    };

    sqlx::query(
        "INSERT INTO channels (id, name, description, owner_id, is_direct, is_group_dm, created_at) \
         VALUES (?, ?, '', NULL, 1, ?, ?)",
    )
    .bind(&channel_id)
    .bind(&name)
    .bind(is_group_dm)
    .bind(now)
    .execute(&state.pool)
    .await?;

    for uid in &user_ids {
        sqlx::query(
            "INSERT INTO channel_members (channel_id, user_id, role) VALUES (?, ?, 'member')",
        )
        .bind(&channel_id)
        .bind(uid)
        .execute(&state.pool)
        .await?;
    }

    let channel = sqlx::query_as::<_, DmChannel>(
        "SELECT id, name, description, owner_id, is_direct, is_group_dm, is_archived, created_at \
         FROM channels WHERE id = ?",
    )
    .bind(&channel_id)
    .fetch_one(&state.pool)
    .await?;

    Ok((StatusCode::CREATED, Json(channel)))
}

/// GET /api/dm
pub async fn list_dms(
    State(state): State<Arc<AppState>>,
    user: AuthenticatedUser,
) -> Result<Json<DmListResponse>, AppError> {
    let channels = sqlx::query_as::<_, DmChannel>(
        "SELECT c.id, c.name, c.description, c.owner_id, c.is_direct, \
                c.is_group_dm, c.is_archived, c.created_at \
         FROM channels c \
         JOIN channel_members cm ON c.id = cm.channel_id \
         WHERE cm.user_id = ? AND c.is_direct = 1 \
         ORDER BY c.created_at DESC",
    )
    .bind(&user.0)
    .fetch_all(&state.pool)
    .await?;

    Ok(Json(DmListResponse { channels }))
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

async fn generate_dm_name(
    state: &Arc<AppState>,
    user_ids: &[String],
) -> Result<String, AppError> {
    let mut names = Vec::new();
    for uid in user_ids {
        let username: Option<String> =
            sqlx::query_scalar("SELECT username FROM users WHERE id = ?")
                .bind(uid)
                .fetch_optional(&state.pool)
                .await?;

        match username {
            Some(name) => names.push(name),
            None => names.push(uid.clone()),
        }
    }
    Ok(names.join(", "))
}

// ---------------------------------------------------------------------------
// Router
// ---------------------------------------------------------------------------

pub fn dm_routes() -> Router<Arc<AppState>> {
    use axum::routing::post;
    Router::new().route("/", post(create_dm).get(list_dms))
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

    struct TestContext {
        app: Router,
        pool: sqlx::SqlitePool,
    }

    async fn setup_user(pool: &sqlx::SqlitePool) -> (String, String) {
        let user_id = Uuid::new_v4().to_string();
        let pw = crate::auth::hash_password("testpass").expect("Failed to hash password");
        let username = format!("user_{}", &user_id[..6]);
        sqlx::query("INSERT INTO users (id, username, password_hash) VALUES (?, ?, ?)")
            .bind(&user_id)
            .bind(&username)
            .bind(&pw)
            .execute(pool)
            .await
            .expect("Failed to insert test user");

        let secret = "test-secret";
        unsafe { std::env::set_var("JWT_SECRET", secret) };
        let token = crate::auth::create_token_pair(&user_id, secret)
            .expect("Failed to create token")
            .access_token;

        (user_id, token)
    }

    async fn setup_app() -> TestContext {
        let pool = sqlx::SqlitePool::connect("sqlite::memory:")
            .await
            .expect("Failed to create in-memory DB");
        sqlx::migrate!("db/migrations")
            .run(&pool)
            .await
            .expect("Failed to run migrations");

        let secret = "test-secret";
        unsafe { std::env::set_var("JWT_SECRET", secret) };

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
        TestContext { app, pool }
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

    // ── Tests ─────────────────────────────────────────────────────────

    #[tokio::test]
    async fn test_create_dm_1on1() {
        let mut ctx = setup_app().await;
        let (u1, token1) = setup_user(&ctx.pool).await;
        let (u2, _token2) = setup_user(&ctx.pool).await;

        let resp = request(
            &mut ctx.app,
            Method::POST,
            "/dm",
            Some(json!({"user_ids": [u1, u2]})),
            &token1,
        )
        .await;

        assert_eq!(resp.status(), StatusCode::CREATED);

        let body: Value = serde_json::from_slice(
            &axum::body::to_bytes(resp.into_body(), usize::MAX)
                .await
                .unwrap(),
        )
        .unwrap();

        assert_eq!(body["is_direct"], true);
        assert_eq!(body["is_group_dm"], false);
        assert_eq!(body["is_archived"], false);
        assert!(!body["id"].as_str().unwrap().is_empty());
        assert!(body["owner_id"].is_null());
    }

    #[tokio::test]
    async fn test_create_dm_group() {
        let mut ctx = setup_app().await;
        let (u1, token1) = setup_user(&ctx.pool).await;
        let (u2, _t2) = setup_user(&ctx.pool).await;
        let (u3, _t3) = setup_user(&ctx.pool).await;

        let resp = request(
            &mut ctx.app,
            Method::POST,
            "/dm",
            Some(json!({"user_ids": [u1, u2, u3], "name": "Team Chat"})),
            &token1,
        )
        .await;

        assert_eq!(resp.status(), StatusCode::CREATED);

        let body: Value = serde_json::from_slice(
            &axum::body::to_bytes(resp.into_body(), usize::MAX)
                .await
                .unwrap(),
        )
        .unwrap();

        assert_eq!(body["is_direct"], true);
        assert_eq!(body["is_group_dm"], true);
        assert_eq!(body["name"], "Team Chat");
        assert!(body["owner_id"].is_null());
        assert_eq!(body["is_archived"], false);
    }

    #[tokio::test]
    async fn test_create_dm_reuse_existing() {
        let mut ctx = setup_app().await;
        let (u1, token1) = setup_user(&ctx.pool).await;
        let (u2, token2) = setup_user(&ctx.pool).await;

        let resp1 = request(
            &mut ctx.app,
            Method::POST,
            "/dm",
            Some(json!({"user_ids": [u1, u2]})),
            &token1,
        )
        .await;

        assert_eq!(resp1.status(), StatusCode::CREATED);
        let body1: Value = serde_json::from_slice(
            &axum::body::to_bytes(resp1.into_body(), usize::MAX)
                .await
                .unwrap(),
        )
        .unwrap();
        let channel_id = body1["id"].as_str().unwrap().to_string();

        let resp2 = request(
            &mut ctx.app,
            Method::POST,
            "/dm",
            Some(json!({"user_ids": [u2, u1]})),
            &token2,
        )
        .await;

        assert_eq!(resp2.status(), StatusCode::OK);

        let body2: Value = serde_json::from_slice(
            &axum::body::to_bytes(resp2.into_body(), usize::MAX)
                .await
                .unwrap(),
        )
        .unwrap();

        assert_eq!(body2["id"], channel_id);
    }

    #[tokio::test]
    async fn test_dm_not_in_regular_listing() {
        let mut ctx = setup_app().await;
        let (u1, token1) = setup_user(&ctx.pool).await;
        let (u2, _t2) = setup_user(&ctx.pool).await;

        request(
            &mut ctx.app,
            Method::POST,
            "/dm",
            Some(json!({"user_ids": [u1, u2]})),
            &token1,
        )
        .await;

        request(
            &mut ctx.app,
            Method::POST,
            "/channels",
            Some(json!({"name": "Regular Channel"})),
            &token1,
        )
        .await;

        let resp = request(&mut ctx.app, Method::GET, "/channels", None, &token1).await;
        assert_eq!(resp.status(), StatusCode::OK);

        let body: Value = serde_json::from_slice(
            &axum::body::to_bytes(resp.into_body(), usize::MAX)
                .await
                .unwrap(),
        )
        .unwrap();

        let channels = body["channels"].as_array().unwrap();
        assert_eq!(channels.len(), 1);
        assert_eq!(channels[0]["name"], "Regular Channel");
        assert_eq!(channels[0]["is_direct"], false);
    }

    #[tokio::test]
    async fn test_dm_cannot_be_archived() {
        let mut ctx = setup_app().await;
        let (u1, token1) = setup_user(&ctx.pool).await;
        let (u2, _t2) = setup_user(&ctx.pool).await;

        let resp = request(
            &mut ctx.app,
            Method::POST,
            "/dm",
            Some(json!({"user_ids": [u1, u2]})),
            &token1,
        )
        .await;

        let body: Value = serde_json::from_slice(
            &axum::body::to_bytes(resp.into_body(), usize::MAX)
                .await
                .unwrap(),
        )
        .unwrap();
        let channel_id = body["id"].as_str().unwrap().to_string();

        let archive_resp = request(
            &mut ctx.app,
            Method::POST,
            &format!("/channels/{}/archive", channel_id),
            None,
            &token1,
        )
        .await;

        assert_eq!(archive_resp.status(), StatusCode::FORBIDDEN);

        let unarchive_resp = request(
            &mut ctx.app,
            Method::POST,
            &format!("/channels/{}/unarchive", channel_id),
            None,
            &token1,
        )
        .await;

        assert_eq!(unarchive_resp.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn test_dm_non_member_access() {
        let mut ctx = setup_app().await;
        let (u1, token1) = setup_user(&ctx.pool).await;
        let (u2, _t2) = setup_user(&ctx.pool).await;
        let (_u3, token3) = setup_user(&ctx.pool).await;

        let resp = request(
            &mut ctx.app,
            Method::POST,
            "/dm",
            Some(json!({"user_ids": [u1, u2]})),
            &token1,
        )
        .await;

        let body: Value = serde_json::from_slice(
            &axum::body::to_bytes(resp.into_body(), usize::MAX)
                .await
                .unwrap(),
        )
        .unwrap();
        let channel_id = body["id"].as_str().unwrap().to_string();

        let access_resp = request(
            &mut ctx.app,
            Method::GET,
            &format!("/channels/{}", channel_id),
            None,
            &token3,
        )
        .await;

        assert_eq!(access_resp.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn test_list_dms() {
        let mut ctx = setup_app().await;
        let (u1, token1) = setup_user(&ctx.pool).await;
        let (u2, _t2) = setup_user(&ctx.pool).await;

        request(
            &mut ctx.app,
            Method::POST,
            "/dm",
            Some(json!({"user_ids": [u1, u2]})),
            &token1,
        )
        .await;

        let resp = request(&mut ctx.app, Method::GET, "/dm", None, &token1).await;
        assert_eq!(resp.status(), StatusCode::OK);

        let body: Value = serde_json::from_slice(
            &axum::body::to_bytes(resp.into_body(), usize::MAX)
                .await
                .unwrap(),
        )
        .unwrap();

        let channels = body["channels"].as_array().unwrap();
        assert_eq!(channels.len(), 1);
        assert_eq!(channels[0]["is_direct"], true);
        assert_eq!(channels[0]["is_group_dm"], false);
    }

    #[tokio::test]
    async fn test_create_dm_requires_auth() {
        let mut ctx = setup_app().await;
        let (u1, _t1) = setup_user(&ctx.pool).await;
        let (u2, _t2) = setup_user(&ctx.pool).await;

        let resp = request(
            &mut ctx.app,
            Method::POST,
            "/dm",
            Some(json!({"user_ids": [u1, u2]})),
            "",
        )
        .await;

        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }
}
