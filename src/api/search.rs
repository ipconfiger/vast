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
// Request / Response types
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct SearchParams {
    pub q: String,
    pub channel_id: Option<String>,
    #[serde(default = "default_limit")]
    pub limit: i64,
}

fn default_limit() -> i64 {
    20
}

#[derive(Debug, FromRow)]
#[allow(dead_code)]
struct SearchResultRow {
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
    snippet: String,
}

#[derive(Debug, Serialize)]
pub struct SearchResultItem {
    pub message: serde_json::Value,
    pub snippet: String,
    pub channel_id: String,
    pub created_at: i64,
}

#[derive(Debug, Serialize)]
pub struct SearchResponse {
    pub results: Vec<SearchResultItem>,
}

// ---------------------------------------------------------------------------
// Handler
// ---------------------------------------------------------------------------

/// GET /api/search?q=keyword&channel_id=&limit=20
///
/// Full-text search across messages using FTS5 with BM25 ranking.
/// Only searches channels the authenticated user is a member of.
/// Supports FTS5 query syntax: phrases, prefix (devel*), boolean (AND/OR/NOT).
pub async fn search_messages(
    auth: AuthenticatedUser,
    State(state): State<Arc<AppState>>,
    Query(params): Query<SearchParams>,
) -> Result<Json<SearchResponse>, AppError> {
    let user_id = auth.0;
    let limit = params.limit.clamp(1, 100);

    let q = params.q.trim();
    if q.is_empty() {
        return Err(AppError::BadRequest(
            "Search query 'q' is required".to_string(),
        ));
    }

    let rows: Vec<SearchResultRow> = query_search(
        &state.pool,
        q,
        &user_id,
        params.channel_id.as_deref(),
        limit,
    )
    .await?;

    let results: Vec<SearchResultItem> = rows
        .into_iter()
        .map(|row| {
            let message: serde_json::Value =
                serde_json::from_str(&row.payload).unwrap_or_default();
            SearchResultItem {
                message,
                snippet: row.snippet,
                channel_id: row.channel_id,
                created_at: row.created_at,
            }
        })
        .collect();

    Ok(Json(SearchResponse { results }))
}

/// Execute the FTS5 search query, handling query syntax errors gracefully.
async fn query_search(
    pool: &sqlx::SqlitePool,
    q: &str,
    user_id: &str,
    channel_id: Option<&str>,
    limit: i64,
) -> Result<Vec<SearchResultRow>, AppError> {
    let result = if let Some(cid) = channel_id {
        sqlx::query_as::<_, SearchResultRow>(
            r#"
            SELECT m.id, m.msg_id, m.channel_id, m.sender_id, m.msg_type, m.payload,
                   m.thread_parent_id, m.deleted_at, m.edited_at, m.created_at,
                   snippet(messages_fts, 0, '<mark>', '</mark>', '...', 32) AS snippet
            FROM messages m
            JOIN messages_fts fts ON m.rowid = fts.rowid
            WHERE messages_fts MATCH ?
              AND m.deleted_at IS NULL
              AND m.channel_id IN (SELECT channel_id FROM channel_members WHERE user_id = ?)
              AND m.channel_id = ?
            ORDER BY bm25(messages_fts)
            LIMIT ?
            "#,
        )
        .bind(q)
        .bind(user_id)
        .bind(cid)
        .bind(limit)
        .fetch_all(pool)
        .await
    } else {
        sqlx::query_as::<_, SearchResultRow>(
            r#"
            SELECT m.id, m.msg_id, m.channel_id, m.sender_id, m.msg_type, m.payload,
                   m.thread_parent_id, m.deleted_at, m.edited_at, m.created_at,
                   snippet(messages_fts, 0, '<mark>', '</mark>', '...', 32) AS snippet
            FROM messages m
            JOIN messages_fts fts ON m.rowid = fts.rowid
            WHERE messages_fts MATCH ?
              AND m.deleted_at IS NULL
              AND m.channel_id IN (SELECT channel_id FROM channel_members WHERE user_id = ?)
            ORDER BY bm25(messages_fts)
            LIMIT ?
            "#,
        )
        .bind(q)
        .bind(user_id)
        .bind(limit)
        .fetch_all(pool)
        .await
    };

    result.map_err(|e| {
        let msg = e.to_string();
        // FTS5 syntax errors are user errors, not server errors
        if msg.contains("fts5: syntax error") || msg.contains("unterminated") {
            AppError::BadRequest(format!("Invalid search query: {msg}"))
        } else {
            AppError::from(e)
        }
    })
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

    /// Helper to build a test app with an in-memory database.
    /// Returns (Router, pool, user1_id, user1_token, user2_id, user2_token).
    async fn setup() -> (
        Router,
        sqlx::SqlitePool,
        String,
        String,
        String,
        String,
    ) {
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

        // Create user1
        let user1_id = Uuid::new_v4().to_string();
        let pw1 = crate::auth::hash_password("testpass").expect("Failed to hash password");
        sqlx::query("INSERT INTO users (id, username, password_hash) VALUES (?, ?, ?)")
            .bind(&user1_id)
            .bind("user1")
            .bind(&pw1)
            .execute(&pool)
            .await
            .expect("Failed to insert user1");

        let token1 = crate::auth::create_token_pair(&user1_id, secret)
            .expect("Failed to create token")
            .access_token;

        // Create user2
        let user2_id = Uuid::new_v4().to_string();
        let pw2 = crate::auth::hash_password("testpass2").expect("Failed to hash password");
        sqlx::query("INSERT INTO users (id, username, password_hash) VALUES (?, ?, ?)")
            .bind(&user2_id)
            .bind("user2")
            .bind(&pw2)
            .execute(&pool)
            .await
            .expect("Failed to insert user2");

        let token2 = crate::auth::create_token_pair(&user2_id, secret)
            .expect("Failed to create token")
            .access_token;

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

        (app, pool, user1_id, token1, user2_id, token2)
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

    /// Helper to parse response body as JSON.
    async fn response_json(resp: axum::response::Response) -> Value {
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        serde_json::from_slice(&body).unwrap()
    }

    /// Helper to seed a channel and return its id.
    async fn create_channel(
        app: &mut Router,
        token: &str,
        name: &str,
    ) -> String {
        let resp = request(
            app,
            Method::POST,
            "/channels",
            Some(json!({"name": name})),
            token,
        )
        .await;
        assert_eq!(resp.status(), StatusCode::CREATED);
        let body = response_json(resp).await;
        body["id"].as_str().unwrap().to_string()
    }

    /// Helper to send a text message in a channel.
    async fn send_message(
        app: &mut Router,
        token: &str,
        channel_id: &str,
        text: &str,
    ) {
        let resp = request(
            app,
            Method::POST,
            &format!("/channels/{}/messages", channel_id),
            Some(json!({"msg_type": "text", "payload": {"text": text}})),
            token,
        )
        .await;
        // Accept 201 Created or 200 OK
        assert!(
            resp.status() == StatusCode::CREATED || resp.status() == StatusCode::OK,
            "Expected 201 or 200, got {}",
            resp.status()
        );
    }

    /// Helper to make a search request.
    async fn search(
        app: &mut Router,
        token: &str,
        q: &str,
        channel_id: Option<&str>,
        limit: Option<i64>,
    ) -> (StatusCode, Value) {
        let mut uri = format!("/search?q={}", urlencoding(q));
        if let Some(cid) = channel_id {
            uri.push_str(&format!("&channel_id={}", cid));
        }
        if let Some(l) = limit {
            uri.push_str(&format!("&limit={}", l));
        }
        let resp = request(app, Method::GET, &uri, None, token).await;
        let status = resp.status();
        let body = response_json(resp).await;
        (status, body)
    }

    fn urlencoding(s: &str) -> String {
        s.replace(' ', "%20")
            .replace('*', "%2A")
    }

    // -----------------------------------------------------------------------
    // Tests
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_search_exact_match() {
        let (mut app, _pool, _u1, token1, _u2, _t2) = setup().await;

        let channel_id = create_channel(&mut app, &token1, "Test Channel").await;
        send_message(&mut app, &token1, &channel_id, "deploy to production at 3pm").await;
        send_message(&mut app, &token1, &channel_id, "development environment setup").await;

        // Search for "production" — should find exactly 1 result
        let (status, body) = search(&mut app, &token1, "production", None, None).await;
        assert_eq!(status, StatusCode::OK);
        let results = body["results"].as_array().unwrap();
        assert_eq!(results.len(), 1, "Should find exactly 1 result for 'production'");
        assert!(
            results[0]["snippet"].as_str().unwrap().contains("production"),
            "Snippet should contain 'production'"
        );
    }

    #[tokio::test]
    async fn test_search_prefix_match() {
        let (mut app, _pool, _u1, token1, _u2, _t2) = setup().await;

        let channel_id = create_channel(&mut app, &token1, "Test Channel").await;
        send_message(&mut app, &token1, &channel_id, "deploy to production at 3pm").await;
        send_message(&mut app, &token1, &channel_id, "development environment setup").await;

        // Search for "devel*" prefix — should match "development"
        let (status, body) = search(&mut app, &token1, "devel*", None, None).await;
        assert_eq!(status, StatusCode::OK);
        let results = body["results"].as_array().unwrap();
        assert_eq!(results.len(), 1, "Prefix 'devel*' should match 'development'");
        assert!(
            results[0]["snippet"].as_str().unwrap().contains("development"),
            "Snippet should highlight 'development'"
        );
    }

    #[tokio::test]
    async fn test_search_across_channels() {
        let (mut app, _pool, _u1, token1, _u2, _t2) = setup().await;

        let ch1 = create_channel(&mut app, &token1, "Channel One").await;
        let ch2 = create_channel(&mut app, &token1, "Channel Two").await;

        send_message(&mut app, &token1, &ch1, "production deployment").await;
        send_message(&mut app, &token1, &ch2, "production issue fixed").await;

        // Search across all channels — should find both
        let (status, body) = search(&mut app, &token1, "production", None, None).await;
        assert_eq!(status, StatusCode::OK);
        let results = body["results"].as_array().unwrap();
        assert_eq!(results.len(), 2, "Should find 'production' in both channels");
    }

    #[tokio::test]
    async fn test_search_channel_filter() {
        let (mut app, _pool, _u1, token1, _u2, _t2) = setup().await;

        let ch1 = create_channel(&mut app, &token1, "Channel One").await;
        let ch2 = create_channel(&mut app, &token1, "Channel Two").await;

        send_message(&mut app, &token1, &ch1, "production deployment").await;
        send_message(&mut app, &token1, &ch2, "production issue fixed").await;

        // Filter by channel one
        let (status, body) = search(&mut app, &token1, "production", Some(&ch1), None).await;
        assert_eq!(status, StatusCode::OK);
        let results = body["results"].as_array().unwrap();
        assert_eq!(results.len(), 1, "Should find only 1 result in channel one");
        assert_eq!(results[0]["channel_id"], ch1);
    }

    #[tokio::test]
    async fn test_search_no_results() {
        let (mut app, _pool, _u1, token1, _u2, _t2) = setup().await;

        let channel_id = create_channel(&mut app, &token1, "Test Channel").await;
        send_message(&mut app, &token1, &channel_id, "hello world").await;

        // Search for something that doesn't exist
        let (status, body) = search(&mut app, &token1, "nonexistent_term_xyz", None, None).await;
        assert_eq!(status, StatusCode::OK);
        let results = body["results"].as_array().unwrap();
        assert_eq!(results.len(), 0, "Should return 0 results for unmatched query");
    }

    #[tokio::test]
    async fn test_search_deleted_messages_excluded() {
        let (mut app, pool, user1_id, token1, _u2, _t2) = setup().await;

        let channel_id = create_channel(&mut app, &token1, "Test Channel").await;
        send_message(&mut app, &token1, &channel_id, "production deployment").await;

        // Insert a deleted message directly via SQL
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;
        sqlx::query(
            r#"INSERT INTO messages (msg_id, channel_id, sender_id, msg_type, payload, deleted_at)
               VALUES (?, ?, ?, 'text', '{"text":"deleted production message"}', ?)"#,
        )
        .bind(Uuid::new_v4().to_string())
        .bind(&channel_id)
        .bind(&user1_id)
        .bind(now)
        .execute(&pool)
        .await
        .unwrap();

        let (status, body) = search(&mut app, &token1, "production", None, None).await;
        assert_eq!(status, StatusCode::OK);
        let results = body["results"].as_array().unwrap();
        assert_eq!(results.len(), 1, "Deleted messages should be excluded");
    }

    #[tokio::test]
    async fn test_search_non_member_excluded() {
        let (mut app, _pool, _u1, token1, _u2, token2) = setup().await;

        let channel_id = create_channel(&mut app, &token1, "User1 Channel").await;
        send_message(&mut app, &token1, &channel_id, "production deployment").await;

        // User2 searches — should not find user1's channel messages
        let (status, body) = search(&mut app, &token2, "production", None, None).await;
        assert_eq!(status, StatusCode::OK);
        let results = body["results"].as_array().unwrap();
        assert_eq!(results.len(), 0, "Non-member should not find messages");
    }

    #[tokio::test]
    async fn test_search_requires_auth() {
        let (mut app, _pool, _u1, _token1, _u2, _t2) = setup().await;

        let resp = request(
            &mut app,
            Method::GET,
            "/search?q=test",
            None,
            "", // no token
        )
        .await;
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn test_search_empty_query() {
        let (mut app, _pool, _u1, token1, _u2, _t2) = setup().await;

        let (status, body) = search(&mut app, &token1, "", None, None).await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(body["error"]["code"], "BAD_REQUEST");
    }

    #[tokio::test]
    async fn test_search_limit() {
        let (mut app, _pool, _u1, token1, _u2, _t2) = setup().await;

        let channel_id = create_channel(&mut app, &token1, "Test Channel").await;
        // Send multiple messages
        for i in 0..5 {
            send_message(
                &mut app,
                &token1,
                &channel_id,
                &format!("message number {}", i),
            )
            .await;
        }

        // Search with limit 2
        let (status, body) = search(&mut app, &token1, "message", None, Some(2)).await;
        assert_eq!(status, StatusCode::OK);
        let results = body["results"].as_array().unwrap();
        assert_eq!(results.len(), 2, "Should respect limit parameter");
    }
}
