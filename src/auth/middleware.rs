use axum::{
    extract::FromRequestParts,
    http::{request::Parts, StatusCode},
    response::{IntoResponse, Json, Response},
};
use serde_json::json;
use sqlx::SqlitePool;
use std::sync::Arc;

use crate::error::AppError;
use crate::AppState;

/// Extract the authenticated user_id from request
pub struct AuthenticatedUser(pub String);

impl FromRequestParts<Arc<AppState>> for AuthenticatedUser {
    type Rejection = Response;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &Arc<AppState>,
    ) -> Result<Self, Self::Rejection> {
        let auth_header = parts
            .headers
            .get("authorization")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.strip_prefix("Bearer "));

        match auth_header {
            Some(token) => {
                let secret = &state.config.jwt_secret;
                match super::validate_token(token, secret) {
                    Ok(claims) => Ok(AuthenticatedUser(claims.sub)),
                    Err(_) => Err((
                        StatusCode::UNAUTHORIZED,
                        Json(json!({"error": {"code": "UNAUTHORIZED", "message": "Invalid or expired token"}})),
                    )
                        .into_response()),
                }
            }
            None => Err((
                StatusCode::UNAUTHORIZED,
                Json(json!({"error": {"code": "UNAUTHORIZED", "message": "Missing authorization header"}})),
            )
                .into_response()),
        }
    }
}

/// Check if user is a member of the specified channel.
/// Returns 403 Forbidden if not a member.
pub async fn require_membership(
    pool: &SqlitePool,
    user_id: &str,
    channel_id: &str,
) -> Result<(), AppError> {
    let exists = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM channel_members WHERE channel_id = ? AND user_id = ?",
    )
    .bind(channel_id)
    .bind(user_id)
    .fetch_one(pool)
    .await?;

    if exists == 0 {
        return Err(AppError::Forbidden(
            "You are not a member of this channel".to_string(),
        ));
    }
    Ok(())
}

/// Check if user has one of the specified roles in the channel.
/// Returns 403 Forbidden if not a member or if role is insufficient.
pub async fn require_role(
    pool: &SqlitePool,
    user_id: &str,
    channel_id: &str,
    allowed_roles: &[&str],
) -> Result<(), AppError> {
    let row = sqlx::query_scalar::<_, String>(
        "SELECT role FROM channel_members WHERE channel_id = ? AND user_id = ?",
    )
    .bind(channel_id)
    .bind(user_id)
    .fetch_optional(pool)
    .await?;

    match row {
        Some(role) if allowed_roles.contains(&role.as_str()) => Ok(()),
        Some(_) => Err(AppError::Forbidden(
            "Insufficient permissions".to_string(),
        )),
        None => Err(AppError::Forbidden(
            "You are not a member of this channel".to_string(),
        )),
    }
}
