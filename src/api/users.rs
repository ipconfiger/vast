use axum::{
    extract::{Query, State},
    Json,
};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use std::sync::Arc;

use crate::auth::middleware::AuthenticatedUser;
use crate::error::AppError;
use crate::AppState;

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct SearchUsersParams {
    pub q: String,
}

#[derive(Debug, Serialize, FromRow)]
pub struct SearchedUser {
    pub id: String,
    pub username: String,
    pub display_name: String,
    pub avatar_url: String,
    pub dm_policy: String,
}

#[derive(Debug, Serialize)]
pub struct SearchUsersResponse {
    pub users: Vec<SearchedUser>,
}

// ---------------------------------------------------------------------------
// Handler
// ---------------------------------------------------------------------------

/// GET /api/users?q=...
///
/// Search users by username. Only returns users with dm_policy = 'open',
/// excluding bots and the requesting user.
pub async fn search_users(
    auth: AuthenticatedUser,
    State(state): State<Arc<AppState>>,
    Query(params): Query<SearchUsersParams>,
) -> Result<Json<SearchUsersResponse>, AppError> {
    let q = params.q.trim();
    if q.is_empty() {
        return Err(AppError::BadRequest(
            "Search query 'q' is required".to_string(),
        ));
    }

    let users: Vec<SearchedUser> = sqlx::query_as::<_, SearchedUser>(
        "SELECT id, username, display_name, avatar_url, dm_policy \
         FROM users \
         WHERE dm_policy = 'open' \
           AND is_bot = 0 \
           AND id != ? \
           AND username LIKE '%' || ? || '%' \
         ORDER BY username \
         LIMIT 20",
    )
    .bind(&auth.0)
    .bind(q)
    .fetch_all(&state.pool)
    .await?;

    Ok(Json(SearchUsersResponse { users }))
}

// ===========================================================================
// Tests
// ===========================================================================

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
    use uuid::Uuid;

    async fn setup_app() -> (Router, sqlx::SqlitePool) {
        let pool = sqlx::SqlitePool::connect("sqlite::memory:")
            .await
            .expect("Failed to create in-memory DB");
        sqlx::migrate!("db/migrations")
            .run(&pool)
            .await
            .expect("Failed to run migrations");

        let secret = "test-secret";
        // SAFETY: test-only; single-threaded
        unsafe { std::env::set_var("JWT_SECRET", secret) };

        let state = Arc::new(AppState {
            pool: pool.clone(),
            ws_pool: Arc::new(crate::ws::ConnectionPool::new()),
            config: crate::AppConfig {
                jwt_secret: secret.to_string(),
                invite_code: "TEST".to_string(),
                ..crate::AppConfig::test_default()
            },
        });

        let app = crate::api::routes().with_state(state);
        (app, pool)
    }

    async fn setup_user(
        pool: &sqlx::SqlitePool,
        username: &str,
        dm_policy: &str,
        is_bot: bool,
    ) -> (String, String) {
        let user_id = Uuid::new_v4().to_string();
        let pw = crate::auth::hash_password("testpass").expect("Failed to hash password");
        sqlx::query(
            "INSERT INTO users (id, username, password_hash, dm_policy, is_bot) VALUES (?, ?, ?, ?, ?)",
        )
        .bind(&user_id)
        .bind(username)
        .bind(&pw)
        .bind(dm_policy)
        .bind(is_bot as i64)
        .execute(pool)
        .await
        .expect("Failed to insert test user");

        let secret = "test-secret";
        let token = crate::auth::create_token_pair(&user_id, secret, 0)
            .expect("Failed to create token")
            .access_token;

        (user_id, token)
    }

    async fn search(
        app: &mut Router,
        token: &str,
        q: &str,
    ) -> (StatusCode, Value) {
        let uri = format!("/users?q={}", urlencoding(q));
        let req = Request::builder()
            .method(Method::GET)
            .uri(&uri)
            .header(header::AUTHORIZATION, format!("Bearer {}", token))
            .body(Body::empty())
            .unwrap();

        let resp = app.oneshot(req).await.unwrap();
        let status = resp.status();
        let body_bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let val: Value = serde_json::from_slice(&body_bytes).unwrap_or(json!({}));
        (status, val)
    }

    fn urlencoding(s: &str) -> String {
        s.replace(' ', "%20")
    }

    // ── Tests ─────────────────────────────────────────────────────────

    #[tokio::test]
    async fn test_search_users_finds_open_policy() {
        let (mut app, pool) = setup_app().await;

        // Create a user with dm_policy = 'open'
        let (_uid1, _token1) = setup_user(&pool, "alice", "open", false).await;
        // Create another user with dm_policy = 'members' (should not appear)
        let (_uid2, _token2) = setup_user(&pool, "bob", "members", false).await;
        // Create the requesting user
        let (_uid3, token3) = setup_user(&pool, "carol", "open", false).await;

        let (status, body) = search(&mut app, &token3, "ali").await;
        assert_eq!(status, StatusCode::OK);

        let users = body["users"].as_array().unwrap();
        assert_eq!(users.len(), 1, "Should find alice (dm_policy=open)");
        assert_eq!(users[0]["username"], "alice");
    }

    #[tokio::test]
    async fn test_search_users_excludes_members_policy() {
        let (mut app, pool) = setup_app().await;

        let (_uid1, _token1) = setup_user(&pool, "bob", "members", false).await;
        let (_uid2, token2) = setup_user(&pool, "carol", "open", false).await;

        let (status, body) = search(&mut app, &token2, "bob").await;
        assert_eq!(status, StatusCode::OK);

        let users = body["users"].as_array().unwrap();
        assert_eq!(users.len(), 0, "bob has dm_policy=members, should be excluded");
    }

    #[tokio::test]
    async fn test_search_users_excludes_bots() {
        let (mut app, pool) = setup_app().await;

        // Create a bot user with dm_policy = 'open'
        let (_uid1, _token1) = setup_user(&pool, "bot_user", "open", true).await;
        let (_uid2, token2) = setup_user(&pool, "carol", "open", false).await;

        let (status, body) = search(&mut app, &token2, "bot").await;
        assert_eq!(status, StatusCode::OK);

        let users = body["users"].as_array().unwrap();
        assert_eq!(users.len(), 0, "Bots should be excluded from search");
    }

    #[tokio::test]
    async fn test_search_users_excludes_self() {
        let (mut app, pool) = setup_app().await;

        let (_uid1, token1) = setup_user(&pool, "dave", "open", false).await;

        let (status, body) = search(&mut app, &token1, "dave").await;
        assert_eq!(status, StatusCode::OK);

        let users = body["users"].as_array().unwrap();
        assert_eq!(users.len(), 0, "User should not find themselves");
    }

    #[tokio::test]
    async fn test_search_users_requires_auth() {
        let (app, pool) = setup_app().await;

        let (_uid1, _token1) = setup_user(&pool, "alice", "open", false).await;

        let req = Request::builder()
            .method(Method::GET)
            .uri("/users?q=ali")
            .body(Body::empty())
            .unwrap();

        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn test_search_users_empty_query() {
        let (mut app, pool) = setup_app().await;

        let (_uid1, token1) = setup_user(&pool, "alice", "open", false).await;

        let (status, body) = search(&mut app, &token1, "").await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(body["error"]["code"], "BAD_REQUEST");
    }

    #[tokio::test]
    async fn test_search_users_limit_20() {
        let (mut app, pool) = setup_app().await;

        // Create 25 open-policy users
        for i in 0..25 {
            let uid = Uuid::new_v4().to_string();
            let username = format!("searchable_{:02}", i);
            let pw = crate::auth::hash_password("testpass").unwrap();
            sqlx::query(
                "INSERT INTO users (id, username, password_hash, dm_policy, is_bot) VALUES (?, ?, ?, 'open', 0)",
            )
            .bind(&uid)
            .bind(&username)
            .bind(&pw)
            .execute(&pool)
            .await
            .unwrap();
        }

        // Requesting user
        let (_uid_req, token_req) = setup_user(&pool, "requester", "open", false).await;

        let (status, body) = search(&mut app, &token_req, "searchable").await;
        assert_eq!(status, StatusCode::OK);

        let users = body["users"].as_array().unwrap();
        assert!(
            users.len() <= 20,
            "Results should be limited to 20, got {}",
            users.len()
        );
    }
}
