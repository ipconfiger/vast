use axum::{
    extract::{Path, State},
    routing::{delete, patch, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::auth::middleware::{require_membership, require_role, AuthenticatedUser};
use crate::error::{created_response, no_content, ok_response, ApiResult, AppError};
use crate::AppState;

pub fn routes() -> Router<Arc<AppState>> {
    Router::new().nest("/{channel_id}/members", member_routes())
}

fn member_routes() -> Router<Arc<AppState>> {
    Router::new()
        .route("/", post(add_member).get(list_members))
        .route("/{user_id}", delete(remove_member))
        .route("/{user_id}/role", patch(update_role))
}

// ---------------------------------------------------------------------------
// Request / Response types
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct AddMemberRequest {
    user_id: String,
    #[serde(default = "default_member_role")]
    role: String,
}

fn default_member_role() -> String {
    "member".to_string()
}

#[derive(Serialize)]
struct UserResponse {
    id: String,
    username: String,
    display_name: String,
    avatar_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    status: Option<String>,
    created_at: i64,
}

#[derive(Serialize)]
struct MemberResponse {
    id: String,
    channel_id: String,
    user_id: String,
    role: String,
    joined_at: i64,
    user: UserResponse,
}

#[derive(Deserialize)]
struct UpdateRoleRequest {
    role: String,
}

const VALID_ROLES: &[&str] = &["owner", "admin", "member"];

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

async fn add_member(
    auth: AuthenticatedUser,
    State(state): State<Arc<AppState>>,
    Path(channel_id): Path<String>,
    Json(body): Json<AddMemberRequest>,
) -> ApiResult<serde_json::Value> {
    require_role(&state.pool, &auth.0, &channel_id, &["owner", "admin"]).await?;

    if !VALID_ROLES.contains(&body.role.as_str()) {
        return Err(AppError::Forbidden(format!(
            "Invalid role: {}. Must be owner, admin, or member",
            body.role
        )));
    }

    if body.role != "member" {
        require_role(&state.pool, &auth.0, &channel_id, &["owner"]).await?;
    }

    sqlx::query(
        "INSERT OR IGNORE INTO channel_members (channel_id, user_id, role) VALUES (?, ?, ?)",
    )
    .bind(&channel_id)
    .bind(&body.user_id)
    .bind(&body.role)
    .execute(&state.pool)
    .await?;

    created_response(serde_json::json!({"status": "ok"}))
}

async fn remove_member(
    auth: AuthenticatedUser,
    State(state): State<Arc<AppState>>,
    Path((channel_id, target_user_id)): Path<(String, String)>,
) -> ApiResult<serde_json::Value> {
    if auth.0 == target_user_id {
        let role: Option<String> = sqlx::query_scalar(
            "SELECT role FROM channel_members WHERE channel_id = ? AND user_id = ?",
        )
        .bind(&channel_id)
        .bind(&auth.0)
        .fetch_optional(&state.pool)
        .await?;

        return match role {
            Some(r) if r == "owner" => Err(AppError::Forbidden(
                "Cannot leave channel as owner. Transfer ownership first.".to_string(),
            )),
            Some(_) => {
                sqlx::query(
                    "DELETE FROM channel_members WHERE channel_id = ? AND user_id = ?",
                )
                .bind(&channel_id)
                .bind(&target_user_id)
                .execute(&state.pool)
                .await?;
                no_content()
            }
            None => Err(AppError::NotFound("Member not found".to_string())),
        };
    }

    require_role(&state.pool, &auth.0, &channel_id, &["owner"]).await?;

    let target_role: Option<String> = sqlx::query_scalar(
        "SELECT role FROM channel_members WHERE channel_id = ? AND user_id = ?",
    )
    .bind(&channel_id)
    .bind(&target_user_id)
    .fetch_optional(&state.pool)
    .await?;

    match target_role {
        Some(r) if r == "owner" => Err(AppError::Forbidden(
            "Cannot remove the channel owner".to_string(),
        )),
        None => Err(AppError::NotFound("Member not found".to_string())),
        _ => {
            sqlx::query("DELETE FROM channel_members WHERE channel_id = ? AND user_id = ?")
                .bind(&channel_id)
                .bind(&target_user_id)
                .execute(&state.pool)
                .await?;
            no_content()
        }
    }
}

async fn update_role(
    auth: AuthenticatedUser,
    State(state): State<Arc<AppState>>,
    Path((channel_id, target_user_id)): Path<(String, String)>,
    Json(body): Json<UpdateRoleRequest>,
) -> ApiResult<serde_json::Value> {
    require_role(&state.pool, &auth.0, &channel_id, &["owner"]).await?;

    if !VALID_ROLES.contains(&body.role.as_str()) {
        return Err(AppError::Forbidden(format!(
            "Invalid role: {}. Must be owner, admin, or member",
            body.role
        )));
    }

    let target_role: Option<String> = sqlx::query_scalar(
        "SELECT role FROM channel_members WHERE channel_id = ? AND user_id = ?",
    )
    .bind(&channel_id)
    .bind(&target_user_id)
    .fetch_optional(&state.pool)
    .await?;

    match target_role {
        Some(r) if r == "owner" && target_user_id != auth.0 => Err(AppError::Forbidden(
            "Cannot change the role of the channel owner".to_string(),
        )),
        None => Err(AppError::NotFound("Member not found".to_string())),
        _ => {
            sqlx::query(
                "UPDATE channel_members SET role = ? WHERE channel_id = ? AND user_id = ?",
            )
            .bind(&body.role)
            .bind(&channel_id)
            .bind(&target_user_id)
            .execute(&state.pool)
            .await?;
            ok_response(serde_json::json!({"status": "ok"}))
        }
    }
}

async fn list_members(
    auth: AuthenticatedUser,
    State(state): State<Arc<AppState>>,
    Path(channel_id): Path<String>,
) -> ApiResult<Vec<MemberResponse>> {
    require_membership(&state.pool, &auth.0, &channel_id).await?;

    let members = sqlx::query_as::<_, (String, String, String, String, String, String, Option<String>, i64, i64)>(
        "SELECT cm.channel_id, cm.user_id, cm.role, u.id, u.username, u.display_name, u.avatar_url, u.created_at, cm.joined_at
         FROM channel_members cm
         JOIN users u ON u.id = cm.user_id
         WHERE cm.channel_id = ?
         ORDER BY cm.joined_at ASC",
    )
    .bind(&channel_id)
    .fetch_all(&state.pool)
    .await?
    .into_iter()
    .map(|(channel_id, user_id, role, uid, username, display_name, avatar_url, created_at, joined_at)| {
        // Generate a synthetic ID using channel_id and user_id
        let id = format!("{}:{}", channel_id, user_id);
        MemberResponse {
            id,
            channel_id,
            user_id,
            role,
            joined_at,
            user: UserResponse {
                id: uid,
                username,
                display_name,
                avatar_url,
                status: None,
                created_at,
            },
        }
    })
    .collect();

    ok_response(members)
}
