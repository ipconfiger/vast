use axum::{
    extract::{Path, State},
    Json,
};
use serde::Serialize;
use std::sync::Arc;

use crate::AppState;

#[derive(Serialize)]
pub struct PresenceResponse {
    online_users: Vec<String>,
}

pub async fn get_presence(
    State(state): State<Arc<AppState>>,
    Path(channel_id): Path<String>,
) -> Json<PresenceResponse> {
    let online_users = state.ws_pool.get_channel_members(&channel_id);
    Json(PresenceResponse { online_users })
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use axum::routing::get;
    use axum::Router;
    use serde_json::Value;
    use std::sync::Arc;
    use tower::ServiceExt;

    fn make_state() -> Arc<AppState> {
        Arc::new(AppState {
            pool: sqlx::SqlitePool::connect_lazy(":memory:").unwrap(),
            ws_pool: Arc::new(crate::ws::ConnectionPool::new()),
            config: crate::AppConfig {
                data_dir: std::path::PathBuf::from("/tmp"),
                jwt_secret: "test-secret".to_string(),
                invite_code: "TESTINVITE".to_string(),
                tls_mode: crate::TlsMode::None,
            },
        })
    }

    fn build_app(state: Arc<AppState>) -> Router {
        Router::new()
            .route("/{channel_id}", get(get_presence))
            .with_state(state)
    }

    async fn get_json(app: &mut Router, uri: &str) -> (StatusCode, Value) {
        let req = Request::builder()
            .method("GET")
            .uri(uri)
            .body(Body::empty())
            .unwrap();

        let resp = app.oneshot(req).await.unwrap();
        let status = resp.status();
        let body_bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let val: Value = serde_json::from_slice(&body_bytes).unwrap_or(serde_json::json!({}));
        (status, val)
    }

    #[tokio::test]
    async fn test_get_presence_returns_online_users() {
        let state = make_state();
        state.ws_pool.register("user-a", "conn-1");
        state.ws_pool.subscribe_channel("conn-1", "ch-1");
        state.ws_pool.register("user-b", "conn-2");
        state.ws_pool.subscribe_channel("conn-2", "ch-1");

        let mut app = build_app(state);
        let (status, body) = get_json(&mut app, "/ch-1").await;

        assert_eq!(status, StatusCode::OK);
        let users = body["online_users"].as_array().unwrap();
        assert_eq!(users.len(), 2);
        let ids: Vec<&str> = users.iter().map(|v| v.as_str().unwrap()).collect();
        assert!(ids.contains(&"user-a"));
        assert!(ids.contains(&"user-b"));
    }

    #[tokio::test]
    async fn test_get_presence_empty_channel() {
        let state = make_state();
        let mut app = build_app(state);
        let (status, body) = get_json(&mut app, "/empty-channel").await;

        assert_eq!(status, StatusCode::OK);
        let users = body["online_users"].as_array().unwrap();
        assert!(users.is_empty());
    }

    #[tokio::test]
    async fn test_get_presence_multi_channel_user() {
        let state = make_state();
        state.ws_pool.register("user-a", "conn-1");
        state.ws_pool.subscribe_channel("conn-1", "ch-1");
        state.ws_pool.subscribe_channel("conn-1", "ch-2");

        let mut app = build_app(state);

        // User appears in ch-1
        let (_, body) = get_json(&mut app, "/ch-1").await;
        let users: Vec<&str> = body["online_users"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap())
            .collect();
        assert_eq!(users, vec!["user-a"]);

        // User appears in ch-2
        let (_, body) = get_json(&mut app, "/ch-2").await;
        let users: Vec<&str> = body["online_users"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap())
            .collect();
        assert_eq!(users, vec!["user-a"]);
    }
}
