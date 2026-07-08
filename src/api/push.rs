use axum::{
    extract::{Query, State},
    http::StatusCode,
    Json,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::auth::middleware::AuthenticatedUser;
use crate::error::AppError;
use crate::AppState;

// ---------------------------------------------------------------------------
// Request / Response types
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct SubscribeRequest {
    pub endpoint: String,
    pub p256dh: String,
    pub auth: String,
}

#[derive(Debug, Deserialize)]
pub struct ResubscribeRequest {
    pub old_endpoint: String,
    pub new_endpoint: String,
    pub new_p256dh: String,
    pub new_auth: String,
}

#[derive(Debug, Deserialize)]
pub struct UnsubscribeQuery {
    pub endpoint: String,
}

#[derive(Debug, Serialize)]
pub struct SubscribeResponse {
    pub ok: bool,
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

/// POST /api/push/subscribe
///
/// Subscribe the authenticated user to push notifications.
/// Uses INSERT OR IGNORE for idempotent subscribe.
pub async fn subscribe_handler(
    State(state): State<Arc<AppState>>,
    user: AuthenticatedUser,
    Json(req): Json<SubscribeRequest>,
) -> Result<(StatusCode, Json<SubscribeResponse>), AppError> {
    sqlx::query(
        "INSERT OR IGNORE INTO push_subscriptions (user_id, endpoint, p256dh, auth) VALUES (?, ?, ?, ?)",
    )
    .bind(&user.0)
    .bind(&req.endpoint)
    .bind(&req.p256dh)
    .bind(&req.auth)
    .execute(&state.pool)
    .await?;

    Ok((StatusCode::OK, Json(SubscribeResponse { ok: true })))
}

/// DELETE /api/push/unsubscribe?endpoint=...
///
/// Unsubscribe the authenticated user from push notifications.
/// Only deletes the user's own subscription (filtered by user_id).
pub async fn unsubscribe_handler(
    State(state): State<Arc<AppState>>,
    user: AuthenticatedUser,
    Query(query): Query<UnsubscribeQuery>,
) -> Result<(StatusCode, Json<SubscribeResponse>), AppError> {
    sqlx::query(
        "DELETE FROM push_subscriptions WHERE endpoint = ? AND user_id = ?",
    )
    .bind(&query.endpoint)
    .bind(&user.0)
    .execute(&state.pool)
    .await?;

    Ok((StatusCode::OK, Json(SubscribeResponse { ok: true })))
}

/// POST /api/push/resubscribe
///
/// Update a push subscription when the service worker receives a new endpoint.
/// Public — no authentication required (the service worker runs in an isolated
/// context with no access to the user's JWT).
pub async fn resubscribe_handler(
    State(state): State<Arc<AppState>>,
    Json(req): Json<ResubscribeRequest>,
) -> Result<(StatusCode, Json<SubscribeResponse>), AppError> {
    sqlx::query(
        "UPDATE push_subscriptions SET endpoint = ?, p256dh = ?, auth = ? WHERE endpoint = ?",
    )
    .bind(&req.new_endpoint)
    .bind(&req.new_p256dh)
    .bind(&req.new_auth)
    .bind(&req.old_endpoint)
    .execute(&state.pool)
    .await?;

    Ok((StatusCode::OK, Json(SubscribeResponse { ok: true })))
}
