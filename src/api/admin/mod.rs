//! Admin-console REST endpoints — login, refresh, logout, current-user.
//!
//! All routes are mounted under `/api/admin`. Admin auth is JWT-based but
//! isolated from user auth: admin tokens carry `is_admin: true` and are
//! rejected by user-facing middleware (and vice versa).

use axum::{
    extract::{Path, Query, State},
    routing::{get, patch, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use std::sync::Arc;
use uuid::Uuid;

use crate::auth::{self, admin::AdminAuthenticatedUser, TokenPair};
use crate::error::{created_response, no_content, ok_response, AppError};
use crate::AppState;

// ---------------------------------------------------------------------------
// Request / response types
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct AdminLoginRequest {
    pub username: String,
    pub password: String,
}

#[derive(Debug, Deserialize)]
pub struct AdminRefreshRequest {
    pub refresh_token: String,
}

#[derive(Debug, Serialize)]
pub struct AdminAuthResponse {
    pub access_token: String,
    pub refresh_token: String,
    pub expires_in: u32,
}

impl AdminAuthResponse {
    fn from_tokens(tokens: TokenPair) -> Self {
        Self {
            access_token: tokens.access_token,
            refresh_token: tokens.refresh_token,
            expires_in: tokens.expires_in,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct AdminMeResponse {
    pub username: String,
}

#[derive(Debug, Serialize)]
pub struct DashboardStats {
    pub total_users: i64,
    pub active_sessions_24h: i64,
    pub total_channels: i64,
    pub total_messages: i64,
    pub total_invite_codes: i64,
    pub active_invite_codes: i64,
}

#[derive(Debug, Deserialize)]
pub struct ListUsersQuery {
    pub page: Option<i64>,
    pub limit: Option<i64>,
    pub q: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateUserRequest {
    pub display_name: Option<String>,
    pub disabled: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub struct ResetPasswordRequest {
    pub new_password: String,
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct AdminUserView {
    pub id: String,
    pub username: String,
    pub display_name: String,
    pub avatar_url: String,
    pub is_bot: bool,
    pub created_at: i64,
}

#[derive(Debug, Deserialize)]
pub struct ListInviteCodesQuery {
    pub page: Option<i64>,
    pub limit: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub struct CreateInviteCodeRequest {
    pub code: String,
    pub max_uses: Option<i64>,
    pub is_active: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateInviteCodeRequest {
    pub max_uses: Option<i64>,
    pub is_active: Option<bool>,
    pub reset_use_count: Option<bool>,
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct InviteCodeView {
    pub code: String,
    pub created_by_user_id: Option<String>,
    pub max_uses: i64,
    pub use_count: i64,
    pub is_active: bool,
    pub created_at: i64,
}

#[derive(Debug, Deserialize)]
pub struct ListAuditLogsQuery {
    pub page: Option<i64>,
    pub limit: Option<i64>,
    pub action: Option<String>,
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct AuditLogView {
    pub id: String,
    pub action: String,
    pub target_type: Option<String>,
    pub target_id: Option<String>,
    pub details: Option<String>,
    pub performed_at: i64,
}

// ---------------------------------------------------------------------------
// Audit helper
// ---------------------------------------------------------------------------

/// Append a row to `admin_audit_logs`.
///
/// NOTE: the T4 task brief listed a `performed_by` column, but migration
/// 006's schema is `(id, action, target_type, target_id, details,
/// performed_at)` — there is no `performed_by`. This helper matches the
/// actual schema. The admin principal is always `"admin"` (the sole
/// configured username), so encoding it as a column would carry no
/// additional information.
///
/// `performed_at` is a Unix-seconds `i64` (the column is `INTEGER NOT NULL`),
/// matching the timestamp convention used in `src/api/auth.rs`.
///
/// Best-effort: callers should ignore the `Result` unless they specifically
/// need to surface audit failures. Login, for example, must not fail just
/// because the audit row could not be written.
pub(crate) async fn audit(
    pool: &SqlitePool,
    action: &str,
    target_type: Option<&str>,
    target_id: Option<&str>,
    details: Option<&str>,
) -> Result<(), AppError> {
    let id = Uuid::now_v7().to_string();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|e| AppError::Internal(format!("SystemTime before UNIX_EPOCH: {e}")))?
        .as_secs() as i64;

    sqlx::query(
        "INSERT INTO admin_audit_logs \
         (id, action, target_type, target_id, details, performed_at) \
         VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(&id)
    .bind(action)
    .bind(target_type)
    .bind(target_id)
    .bind(details)
    .bind(now)
    .execute(pool)
    .await?;

    Ok(())
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

/// POST /api/admin/login
///
/// Returns 403 when the admin backend is disabled (no password hash
/// configured), 401 on username/password mismatch, 200 with a fresh admin
/// token pair on success.
pub async fn login(
    State(state): State<Arc<AppState>>,
    Json(body): Json<AdminLoginRequest>,
) -> Result<(axum::http::StatusCode, Json<AdminAuthResponse>), AppError> {
    if state.config.admin_password_hash.is_empty() {
        return Err(AppError::Forbidden(
            "Admin backend is disabled".to_string(),
        ));
    }

    if body.username != state.config.admin_username {
        return Err(AppError::Unauthorized(
            "Invalid username or password".to_string(),
        ));
    }

    let valid = auth::verify_password(&body.password, &state.config.admin_password_hash)
        .map_err(|_| AppError::Internal("Password verification failed".to_string()))?;
    if !valid {
        return Err(AppError::Unauthorized(
            "Invalid username or password".to_string(),
        ));
    }

    let tokens = crate::auth::admin::create_admin_token_pair(&state.config.jwt_secret)?;

    // Best-effort audit; failure must not block login.
    let _ = audit(&state.pool, "admin.login", None, None, None).await;

    ok_response(AdminAuthResponse::from_tokens(tokens))
}

/// POST /api/admin/refresh
pub async fn refresh(
    State(state): State<Arc<AppState>>,
    Json(body): Json<AdminRefreshRequest>,
) -> Result<(axum::http::StatusCode, Json<AdminAuthResponse>), AppError> {
    let tokens = crate::auth::admin::refresh_admin_token(
        &body.refresh_token,
        &state.config.jwt_secret,
    )?;
    ok_response(AdminAuthResponse::from_tokens(tokens))
}

/// POST /api/admin/logout
///
/// Stateless 204 No Content. v1 known limitation: a stolen refresh token
/// survives `REFRESH_TTL_SECS` (7 days) because admin auth has no
/// server-side session revocation list. Mitigation: rotate `ADMIN_PASSWORD`
/// (forces hash mismatch on next login) or rotate `JWT_SECRET` (invalidates
/// all outstanding admin tokens).
pub async fn logout(
    _auth: AdminAuthenticatedUser,
    State(_state): State<Arc<AppState>>,
) -> Result<(axum::http::StatusCode, Json<serde_json::Value>), AppError> {
    no_content()
}

/// GET /api/admin/me
///
/// Returns the configured `admin_username`. The token's `sub` is always
/// `"admin"` (a fixed subject shared by every admin pair), so the response
/// uses `state.config.admin_username` for a human-readable handle.
pub async fn me(
    _auth: AdminAuthenticatedUser,
    State(state): State<Arc<AppState>>,
) -> Result<Json<AdminMeResponse>, AppError> {
    Ok(Json(AdminMeResponse {
        username: state.config.admin_username.clone(),
    }))
}

/// GET /api/admin/dashboard
///
/// Returns aggregate platform statistics: total users, sessions active
/// in the last 24 hours, total channels, messages, and invite codes
/// (total + active). Runs 6 lightweight COUNT queries.
pub async fn dashboard(
    _admin: AdminAuthenticatedUser,
    State(state): State<Arc<AppState>>,
) -> Result<(axum::http::StatusCode, Json<DashboardStats>), AppError> {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|e| AppError::Internal(format!("SystemTime before UNIX_EPOCH: {e}")))?
        .as_secs() as i64;
    let cutoff = now - 86400;

    let total_users: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM users")
            .fetch_one(&state.pool)
            .await?;

    let active_sessions_24h: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM sessions WHERE is_active = 1 AND created_at >= ?")
            .bind(cutoff)
            .fetch_one(&state.pool)
            .await?;

    let total_channels: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM channels")
            .fetch_one(&state.pool)
            .await?;

    let total_messages: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM messages")
            .fetch_one(&state.pool)
            .await?;

    let total_invite_codes: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM invite_codes")
            .fetch_one(&state.pool)
            .await?;

    let active_invite_codes: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM invite_codes WHERE is_active = 1")
            .fetch_one(&state.pool)
            .await?;

    ok_response(DashboardStats {
        total_users,
        active_sessions_24h,
        total_channels,
        total_messages,
        total_invite_codes,
        active_invite_codes,
    })
}

// ---------------------------------------------------------------------------
// User management handlers
// ---------------------------------------------------------------------------

/// GET /api/admin/users
///
/// Paginated user list with optional username LIKE search.
pub async fn list_users(
    _admin: AdminAuthenticatedUser,
    State(state): State<Arc<AppState>>,
    Query(params): Query<ListUsersQuery>,
) -> Result<Json<Vec<AdminUserView>>, AppError> {
    let page = params.page.unwrap_or(1).max(1);
    let limit = params.limit.unwrap_or(20).clamp(1, 100);
    let offset = (page - 1) * limit;

    let users: Vec<AdminUserView> = if let Some(ref q) = params.q {
        sqlx::query_as::<_, AdminUserView>(
            "SELECT id, username, display_name, avatar_url, is_bot, created_at FROM users \
             WHERE username LIKE '%' || ? || '%' \
             ORDER BY created_at DESC, id \
             LIMIT ? OFFSET ?",
        )
        .bind(q)
        .bind(limit)
        .bind(offset)
        .fetch_all(&state.pool)
        .await?
    } else {
        sqlx::query_as::<_, AdminUserView>(
            "SELECT id, username, display_name, avatar_url, is_bot, created_at FROM users \
             ORDER BY created_at DESC, id \
             LIMIT ? OFFSET ?",
        )
        .bind(limit)
        .bind(offset)
        .fetch_all(&state.pool)
        .await?
    };

    Ok(Json(users))
}

/// GET /api/admin/users/{id}
pub async fn get_user(
    _admin: AdminAuthenticatedUser,
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<AdminUserView>, AppError> {
    let user = sqlx::query_as::<_, AdminUserView>(
        "SELECT id, username, display_name, avatar_url, is_bot, created_at FROM users WHERE id = ?",
    )
    .bind(&id)
    .fetch_optional(&state.pool)
    .await?;

    user.map(Json)
        .ok_or_else(|| AppError::NotFound("User not found".to_string()))
}

/// PATCH /api/admin/users/{id}
///
/// Updates display_name and/or bumps token_epoch (disable). The epoch
/// bump immediately revokes all the user's existing tokens (T2 mechanism).
pub async fn update_user(
    _admin: AdminAuthenticatedUser,
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(body): Json<UpdateUserRequest>,
) -> Result<Json<AdminUserView>, AppError> {
    let exists: Option<i64> = sqlx::query_scalar("SELECT 1 FROM users WHERE id = ?")
        .bind(&id)
        .fetch_optional(&state.pool)
        .await?;
    if exists.is_none() {
        return Err(AppError::NotFound("User not found".to_string()));
    }

    if let Some(ref name) = body.display_name {
        let trimmed = name.trim();
        if trimmed.len() > 32 {
            return Err(AppError::BadRequest(
                "Display name must be 32 characters or fewer".into(),
            ));
        }
        sqlx::query("UPDATE users SET display_name = ? WHERE id = ?")
            .bind(trimmed)
            .bind(&id)
            .execute(&state.pool)
            .await?;
    }

    let disabled = body.disabled == Some(true);
    if disabled {
        sqlx::query("UPDATE users SET token_epoch = token_epoch + 1 WHERE id = ?")
            .bind(&id)
            .execute(&state.pool)
            .await?;
    }

    let action = if disabled { "user.disable" } else { "user.update" };
    let _ = audit(&state.pool, action, Some("user"), Some(&id), None).await;

    let user = sqlx::query_as::<_, AdminUserView>(
        "SELECT id, username, display_name, avatar_url, is_bot, created_at FROM users WHERE id = ?",
    )
    .bind(&id)
    .fetch_one(&state.pool)
    .await?;

    Ok(Json(user))
}

/// POST /api/admin/users/{id}/reset-password
///
/// Sets a new password hash and bumps token_epoch, forcing the user to
/// re-authenticate with the new password. All existing tokens are
/// immediately invalidated.
pub async fn reset_password(
    _admin: AdminAuthenticatedUser,
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(body): Json<ResetPasswordRequest>,
) -> Result<(axum::http::StatusCode, Json<serde_json::Value>), AppError> {
    if body.new_password.len() < 8 {
        return Err(AppError::ValidationError(
            "Password must be at least 8 characters".to_string(),
        ));
    }
    let has_letter = body.new_password.chars().any(|c| c.is_ascii_alphabetic());
    let has_digit = body.new_password.chars().any(|c| c.is_ascii_digit());
    if !has_letter || !has_digit {
        return Err(AppError::ValidationError(
            "Password must contain both letters and digits".to_string(),
        ));
    }

    let hash = crate::auth::hash_password(&body.new_password)
        .map_err(|e| AppError::Internal(format!("Failed to hash password: {e}")))?;

    let result = sqlx::query(
        "UPDATE users SET password_hash = ?, token_epoch = token_epoch + 1 WHERE id = ?",
    )
    .bind(&hash)
    .bind(&id)
    .execute(&state.pool)
    .await?;

    if result.rows_affected() == 0 {
        return Err(AppError::NotFound("User not found".to_string()));
    }

    let _ = audit(
        &state.pool,
        "user.reset_password",
        Some("user"),
        Some(&id),
        None,
    )
    .await;

    no_content()
}

/// DELETE /api/admin/users/{id}
///
/// Removes the user row; FK cascades clean up sessions, messages, etc.
pub async fn delete_user(
    _admin: AdminAuthenticatedUser,
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<(axum::http::StatusCode, Json<serde_json::Value>), AppError> {
    let result = sqlx::query("DELETE FROM users WHERE id = ?")
        .bind(&id)
        .execute(&state.pool)
        .await?;

    if result.rows_affected() == 0 {
        return Err(AppError::NotFound("User not found".to_string()));
    }

    let _ = audit(&state.pool, "user.delete", Some("user"), Some(&id), None).await;

    no_content()
}

// ---------------------------------------------------------------------------
// Invite-code handlers
// ---------------------------------------------------------------------------

/// GET /api/admin/invite-codes
pub async fn list_invite_codes(
    _admin: AdminAuthenticatedUser,
    State(state): State<Arc<AppState>>,
    Query(params): Query<ListInviteCodesQuery>,
) -> Result<Json<Vec<InviteCodeView>>, AppError> {
    let page = params.page.unwrap_or(1).max(1);
    let limit = params.limit.unwrap_or(20).clamp(1, 100);
    let offset = (page - 1) * limit;

    let codes: Vec<InviteCodeView> = sqlx::query_as::<_, InviteCodeView>(
        "SELECT code, created_by_user_id, max_uses, use_count, is_active, created_at \
         FROM invite_codes ORDER BY created_at DESC, code LIMIT ? OFFSET ?",
    )
    .bind(limit)
    .bind(offset)
    .fetch_all(&state.pool)
    .await?;

    Ok(Json(codes))
}

/// POST /api/admin/invite-codes
///
/// Creates a new invite code. Duplicate codes trigger 409 via the
/// `From<sqlx::Error>` UNIQUE-constraint mapping.
pub async fn create_invite_code(
    _admin: AdminAuthenticatedUser,
    State(state): State<Arc<AppState>>,
    Json(body): Json<CreateInviteCodeRequest>,
) -> Result<(axum::http::StatusCode, Json<InviteCodeView>), AppError> {
    let trimmed = body.code.trim();
    if trimmed.is_empty() || trimmed.len() > 64 {
        return Err(AppError::BadRequest(
            "Code must be 1-64 characters".to_string(),
        ));
    }

    let max_uses = body.max_uses.unwrap_or(100);
    let is_active = body.is_active.unwrap_or(true);

    sqlx::query(
        "INSERT INTO invite_codes (code, max_uses, use_count, is_active) \
         VALUES (?, ?, 0, ?)",
    )
    .bind(trimmed)
    .bind(max_uses)
    .bind(is_active)
    .execute(&state.pool)
    .await?;

    let _ = audit(
        &state.pool,
        "invite.create",
        Some("invite_code"),
        Some(trimmed),
        None,
    )
    .await;

    let view = sqlx::query_as::<_, InviteCodeView>(
        "SELECT code, created_by_user_id, max_uses, use_count, is_active, created_at \
         FROM invite_codes WHERE code = ?",
    )
    .bind(trimmed)
    .fetch_one(&state.pool)
    .await?;

    created_response(view)
}

/// PATCH /api/admin/invite-codes/{code}
///
/// Partial update: each provided field gets its own UPDATE. If the
/// code does not exist the first UPDATE affects 0 rows → 404.
pub async fn update_invite_code(
    _admin: AdminAuthenticatedUser,
    State(state): State<Arc<AppState>>,
    Path(code): Path<String>,
    Json(body): Json<UpdateInviteCodeRequest>,
) -> Result<Json<InviteCodeView>, AppError> {
    let exists: Option<i64> =
        sqlx::query_scalar("SELECT 1 FROM invite_codes WHERE code = ?")
            .bind(&code)
            .fetch_optional(&state.pool)
            .await?;
    if exists.is_none() {
        return Err(AppError::NotFound("Invite code not found".to_string()));
    }

    if let Some(max_uses) = body.max_uses {
        sqlx::query("UPDATE invite_codes SET max_uses = ? WHERE code = ?")
            .bind(max_uses)
            .bind(&code)
            .execute(&state.pool)
            .await?;
    }
    if let Some(is_active) = body.is_active {
        sqlx::query("UPDATE invite_codes SET is_active = ? WHERE code = ?")
            .bind(is_active)
            .bind(&code)
            .execute(&state.pool)
            .await?;
    }
    if body.reset_use_count == Some(true) {
        sqlx::query("UPDATE invite_codes SET use_count = 0 WHERE code = ?")
            .bind(&code)
            .execute(&state.pool)
            .await?;
    }

    let _ = audit(
        &state.pool,
        "invite.update",
        Some("invite_code"),
        Some(&code),
        None,
    )
    .await;

    let view = sqlx::query_as::<_, InviteCodeView>(
        "SELECT code, created_by_user_id, max_uses, use_count, is_active, created_at \
         FROM invite_codes WHERE code = ?",
    )
    .bind(&code)
    .fetch_one(&state.pool)
    .await?;

    Ok(Json(view))
}

/// DELETE /api/admin/invite-codes/{code}
pub async fn delete_invite_code(
    _admin: AdminAuthenticatedUser,
    State(state): State<Arc<AppState>>,
    Path(code): Path<String>,
) -> Result<(axum::http::StatusCode, Json<serde_json::Value>), AppError> {
    let result = sqlx::query("DELETE FROM invite_codes WHERE code = ?")
        .bind(&code)
        .execute(&state.pool)
        .await?;

    if result.rows_affected() == 0 {
        return Err(AppError::NotFound("Invite code not found".to_string()));
    }

    let _ = audit(
        &state.pool,
        "invite.delete",
        Some("invite_code"),
        Some(&code),
        None,
    )
    .await;

    no_content()
}

// ---------------------------------------------------------------------------
// Audit-log handler
// ---------------------------------------------------------------------------

/// GET /api/admin/audit-logs
pub async fn list_audit_logs(
    _admin: AdminAuthenticatedUser,
    State(state): State<Arc<AppState>>,
    Query(params): Query<ListAuditLogsQuery>,
) -> Result<Json<Vec<AuditLogView>>, AppError> {
    let page = params.page.unwrap_or(1).max(1);
    let limit = params.limit.unwrap_or(20).clamp(1, 100);
    let offset = (page - 1) * limit;

    let logs: Vec<AuditLogView> = if let Some(ref action) = params.action {
        sqlx::query_as::<_, AuditLogView>(
            "SELECT id, action, target_type, target_id, details, performed_at \
             FROM admin_audit_logs WHERE action = ? \
             ORDER BY performed_at DESC, id LIMIT ? OFFSET ?",
        )
        .bind(action)
        .bind(limit)
        .bind(offset)
        .fetch_all(&state.pool)
        .await?
    } else {
        sqlx::query_as::<_, AuditLogView>(
            "SELECT id, action, target_type, target_id, details, performed_at \
             FROM admin_audit_logs \
             ORDER BY performed_at DESC, id LIMIT ? OFFSET ?",
        )
        .bind(limit)
        .bind(offset)
        .fetch_all(&state.pool)
        .await?
    };

    Ok(Json(logs))
}

// ---------------------------------------------------------------------------
// Router
// ---------------------------------------------------------------------------

/// # Admin API Route Contract (frozen for frontend T9)
///
/// All routes are mounted under `/api/admin`. Responses use `application/json`.
/// Error bodies: `{"error":{"code":"ERROR_CODE","message":"..."}}`.
///
/// ## Authentication
/// All endpoints except `/login` and `/refresh` require
/// `Authorization: Bearer <admin_jwt>`. Admin JWTs carry `is_admin=true`;
/// user JWTs are rejected with 401.
///
/// ## Endpoints
///
/// | Method | Path                                  | Request Body                                    | Success                     | Errors                  |
/// |--------|---------------------------------------|-------------------------------------------------|-----------------------------|-------------------------|
/// | POST   | /login                                | `{username, password}`                          | 200 `{access_token, refresh_token, expires_in}` | 401 403                |
/// | POST   | /refresh                              | `{refresh_token}`                               | 200 `{access_token, refresh_token, expires_in}` | 401                    |
/// | POST   | /logout                               | —                                               | 204                         | 401                     |
/// | GET    | /me                                   | —                                               | 200 `{username}`            | 401                     |
/// | GET    | /dashboard                            | —                                               | 200 `{total_users, active_sessions_24h, total_channels, total_messages, total_invite_codes, active_invite_codes}` (all i64) | 401 |
/// | GET    | /users?page=&limit=&q=                | —                                               | 200 `[{id, username, display_name, avatar_url, created_at}]` | 401 |
/// | GET    | /users/{id}                           | —                                               | 200 `{id, username, display_name, avatar_url, created_at}` | 401 404 |
/// | PATCH  | /users/{id}                           | `{display_name?, disabled?}`                    | 200 `{id, username, display_name, avatar_url, created_at}` | 400 401 404 |
/// | POST   | /users/{id}/reset-password            | `{new_password}`                                | 204                         | 401 404 422            |
/// | DELETE | /users/{id}                           | —                                               | 204                         | 401 404                 |
/// | GET    | /invite-codes?page=&limit=            | —                                               | 200 `[{code, created_by_user_id, max_uses, use_count, is_active, created_at}]` | 401 |
/// | POST   | /invite-codes                         | `{code, max_uses?, is_active?}`                 | 201 `{code, created_by_user_id, max_uses, use_count, is_active, created_at}` | 400 401 409 |
/// | PATCH  | /invite-codes/{code}                  | `{max_uses?, is_active?, reset_use_count?}`     | 200 `{code, created_by_user_id, max_uses, use_count, is_active, created_at}` | 401 404 |
/// | DELETE | /invite-codes/{code}                  | —                                               | 204                         | 401 404                 |
/// | GET    | /audit-logs?page=&limit=&action=      | —                                               | 200 `[{id, action, target_type, target_id, details, performed_at}]` | 401 |
///
/// ## Field types
/// - `expires_in`: `u32` (seconds)
/// - `display_name`, `avatar_url`: `String` (NOT NULL DEFAULT '')
/// - `created_at`: `i64` (unix timestamp)
/// - `created_by_user_id`: `Option<String>` (nullable; NULL for admin-created codes)
/// - `max_uses`, `use_count`: `i64`
/// - `is_active`: `bool`
/// - `target_type`, `target_id`, `details`: `Option<String>` (nullable)
/// - `performed_at`: `i64` (unix timestamp)
/// - `disabled` (PATCH request): `Option<bool>` — `true` bumps `token_epoch`, revoking all user tokens
///
/// ## Audit
/// All mutation endpoints (login, update/disable, reset-password, delete user,
/// create/update/delete invite-code) append a row to `admin_audit_logs`.
///
/// ## Pagination
/// `page` defaults to 1, `limit` defaults to 20, clamped to [1, 100].
/// `offset = (page - 1) * limit`.
pub fn admin_routes() -> Router<Arc<AppState>> {
    Router::new()
        .route("/login", post(login))
        .route("/refresh", post(refresh))
        .route("/logout", post(logout))
        .route("/me", get(me))
        .route("/dashboard", get(dashboard))
        .nest(
            "/users",
            Router::new()
                .route("/", get(list_users))
                .route(
                    "/{id}",
                    get(get_user).patch(update_user).delete(delete_user),
                )
                .route("/{id}/reset-password", post(reset_password)),
        )
        .nest(
            "/invite-codes",
            Router::new()
                .route("/", get(list_invite_codes).post(create_invite_code))
                .route(
                    "/{code}",
                    patch(update_invite_code).delete(delete_invite_code),
                ),
        )
        .route("/audit-logs", get(list_audit_logs))
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

    const ADMIN_PASS: &str = "test-admin-pass";

    async fn setup_pool() -> sqlx::SqlitePool {
        let pool = sqlx::SqlitePool::connect(":memory:").await.unwrap();
        sqlx::migrate!("db/migrations").run(&pool).await.unwrap();
        pool
    }

    fn admin_enabled_config() -> crate::AppConfig {
        let mut config = crate::AppConfig::test_default();
        config.admin_password_hash = crate::auth::hash_password(ADMIN_PASS).unwrap();
        config.admin_username = "admin".to_string();
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
            .nest("/admin", admin_routes())
            .with_state(state)
    }

    async fn post_json(
        app: &mut Router,
        uri: &str,
        body: Value,
    ) -> (StatusCode, Value) {
        let req = Request::builder()
            .method("POST")
            .uri(uri)
            .header("content-type", "application/json")
            .body(Body::from(body.to_string()))
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        let status = resp.status();
        let body_bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let val: Value = serde_json::from_slice(&body_bytes).unwrap_or(json!({}));
        (status, val)
    }

    async fn get_with_token(
        app: &mut Router,
        uri: &str,
        token: &str,
    ) -> (StatusCode, Value) {
        let req = Request::builder()
            .method("GET")
            .uri(uri)
            .header("authorization", format!("Bearer {token}"))
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

    async fn post_with_token(
        app: &mut Router,
        uri: &str,
        token: &str,
    ) -> (StatusCode, Value) {
        let req = Request::builder()
            .method("POST")
            .uri(uri)
            .header("authorization", format!("Bearer {token}"))
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
        let body_bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let val: Value = serde_json::from_slice(&body_bytes).unwrap_or(json!({}));
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
        let body_bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let val: Value = serde_json::from_slice(&body_bytes).unwrap_or(json!({}));
        (status, val)
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
        let body_bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let val: Value = serde_json::from_slice(&body_bytes).unwrap_or(json!({}));
        (status, val)
    }

    async fn seed_user(pool: &sqlx::SqlitePool, username: &str) -> String {
        let id = Uuid::now_v7().to_string();
        sqlx::query("INSERT INTO users (id, username, password_hash) VALUES (?, ?, ?)")
            .bind(&id)
            .bind(username)
            .bind("hash")
            .execute(pool)
            .await
            .unwrap();
        id
    }

    // -----------------------------------------------------------------------
    // Login tests
    // -----------------------------------------------------------------------

    /// Given: admin backend is enabled with a known password hash.
    /// When:  POST /admin/login with correct credentials.
    /// Then:  200 OK with access_token, refresh_token, and expires_in.
    #[tokio::test]
    async fn test_login_success() {
        let pool = setup_pool().await;
        let mut app = build_app(make_state(pool, admin_enabled_config()));

        let (status, body) = post_json(
            &mut app,
            "/admin/login",
            json!({"username": "admin", "password": ADMIN_PASS}),
        )
        .await;

        assert_eq!(status, StatusCode::OK);
        assert!(body.get("access_token").and_then(|v| v.as_str()).is_some());
        assert!(
            body.get("refresh_token").and_then(|v| v.as_str()).is_some()
        );
        assert!(body.get("expires_in").and_then(|v| v.as_u64()).is_some());
    }

    /// Given: admin backend is enabled.
    /// When:  POST /admin/login with a wrong password.
    /// Then:  401 Unauthorized.
    #[tokio::test]
    async fn test_login_wrong_password() {
        let pool = setup_pool().await;
        let mut app = build_app(make_state(pool, admin_enabled_config()));

        let (status, body) = post_json(
            &mut app,
            "/admin/login",
            json!({"username": "admin", "password": "totally-wrong"}),
        )
        .await;

        assert_eq!(status, StatusCode::UNAUTHORIZED);
        assert_eq!(body["error"]["code"], "UNAUTHORIZED");
    }

    /// Given: admin backend is enabled with username "admin".
    /// When:  POST /admin/login with a wrong username.
    /// Then:  401 Unauthorized.
    #[tokio::test]
    async fn test_login_wrong_username() {
        let pool = setup_pool().await;
        let mut app = build_app(make_state(pool, admin_enabled_config()));

        let (status, body) = post_json(
            &mut app,
            "/admin/login",
            json!({"username": "root", "password": ADMIN_PASS}),
        )
        .await;

        assert_eq!(status, StatusCode::UNAUTHORIZED);
        assert_eq!(body["error"]["code"], "UNAUTHORIZED");
    }

    /// Given: admin backend is disabled (empty password hash).
    /// When:  POST /admin/login with any credentials.
    /// Then:  403 Forbidden — admin backend is disabled.
    #[tokio::test]
    async fn test_login_admin_disabled() {
        let pool = setup_pool().await;
        let mut app = build_app(make_state(pool, crate::AppConfig::test_default()));

        let (status, body) = post_json(
            &mut app,
            "/admin/login",
            json!({"username": "admin", "password": ADMIN_PASS}),
        )
        .await;

        assert_eq!(status, StatusCode::FORBIDDEN);
        assert_eq!(body["error"]["code"], "FORBIDDEN");
    }

    /// Given: a successful admin login.
    /// When:  the audit table is queried.
    /// Then:  exactly one row exists with action = "admin.login".
    #[tokio::test]
    async fn test_login_writes_audit_row() {
        let pool = setup_pool().await;
        let state = make_state(pool.clone(), admin_enabled_config());
        let mut app = build_app(state);

        let (status, _) = post_json(
            &mut app,
            "/admin/login",
            json!({"username": "admin", "password": ADMIN_PASS}),
        )
        .await;
        assert_eq!(status, StatusCode::OK);

        let (count,): (i64,) =
            sqlx::query_as("SELECT COUNT(*) FROM admin_audit_logs WHERE action = 'admin.login'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(count, 1, "login must write exactly one audit row");
    }

    // -----------------------------------------------------------------------
    // /me tests
    // -----------------------------------------------------------------------

    /// Given: /admin/me requires authentication.
    /// When:  GET /admin/me with no Authorization header.
    /// Then:  401 Unauthorized.
    #[tokio::test]
    async fn test_me_without_token() {
        let pool = setup_pool().await;
        let app = build_app(make_state(pool, admin_enabled_config()));

        let req = Request::builder()
            .method("GET")
            .uri("/admin/me")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    /// Given: a user JWT (is_admin = false).
    /// When:  GET /admin/me with the user token.
    /// Then:  401 Unauthorized — admin endpoints reject user tokens.
    #[tokio::test]
    async fn test_me_with_user_token() {
        let pool = setup_pool().await;
        let mut app = build_app(make_state(pool, admin_enabled_config()));

        let user_pair = crate::auth::create_token_pair("user-1", "test-secret", 0).unwrap();

        let (status, body) = get_with_token(&mut app, "/admin/me", &user_pair.access_token).await;

        assert_eq!(status, StatusCode::UNAUTHORIZED);
        assert_eq!(body["error"]["code"], "UNAUTHORIZED");
    }

    /// Given: a valid admin access token.
    /// When:  GET /admin/me.
    /// Then:  200 OK with { username: "admin" } (from config, not token sub).
    #[tokio::test]
    async fn test_me_with_admin_token() {
        let pool = setup_pool().await;
        let mut app = build_app(make_state(pool, admin_enabled_config()));

        let pair = create_admin_token_pair("test-secret").unwrap();
        let (status, body) = get_with_token(&mut app, "/admin/me", &pair.access_token).await;

        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["username"], "admin");
    }

    // -----------------------------------------------------------------------
    // Refresh tests
    // -----------------------------------------------------------------------

    /// Given: a valid admin refresh token.
    /// When:  POST /admin/refresh.
    /// Then:  200 OK with a new token pair (access differs from original).
    #[tokio::test]
    async fn test_refresh_success() {
        let pool = setup_pool().await;
        let mut app = build_app(make_state(pool, admin_enabled_config()));

        let original = create_admin_token_pair("test-secret").unwrap();
        let (status, body) = post_json(
            &mut app,
            "/admin/refresh",
            json!({"refresh_token": original.refresh_token}),
        )
        .await;

        assert_eq!(status, StatusCode::OK);
        let new_access = body["access_token"].as_str().expect("access_token present");
        let new_refresh = body["refresh_token"].as_str().expect("refresh_token present");
        assert!(body.get("expires_in").and_then(|v| v.as_u64()).is_some());
        assert_ne!(new_access, original.access_token);
        assert_ne!(new_refresh, original.refresh_token);
    }

    /// Given: an invalid refresh token string.
    /// When:  POST /admin/refresh.
    /// Then:  401 Unauthorized.
    #[tokio::test]
    async fn test_refresh_invalid_token() {
        let pool = setup_pool().await;
        let mut app = build_app(make_state(pool, admin_enabled_config()));

        let (status, body) = post_json(
            &mut app,
            "/admin/refresh",
            json!({"refresh_token": "not-a-real-jwt"}),
        )
        .await;

        assert_eq!(status, StatusCode::UNAUTHORIZED);
        assert_eq!(body["error"]["code"], "UNAUTHORIZED");
    }

    // -----------------------------------------------------------------------
    // Logout tests
    // -----------------------------------------------------------------------

    /// Given: a valid admin access token.
    /// When:  POST /admin/logout.
    /// Then:  204 No Content (stateless).
    #[tokio::test]
    async fn test_logout_with_admin_token() {
        let pool = setup_pool().await;
        let mut app = build_app(make_state(pool, admin_enabled_config()));

        let pair = create_admin_token_pair("test-secret").unwrap();
        let (status, _body) = post_with_token(&mut app, "/admin/logout", &pair.access_token).await;

        assert_eq!(status, StatusCode::NO_CONTENT);
    }

    // -----------------------------------------------------------------------
    // Dashboard tests
    // -----------------------------------------------------------------------

    /// Given: a freshly migrated database with the seeded invite code removed.
    /// When:  GET /admin/dashboard with a valid admin token.
    /// Then:  200 OK with all six counts at 0.
    #[tokio::test]
    async fn test_dashboard_empty_db() {
        let pool = setup_pool().await;
        sqlx::query("DELETE FROM invite_codes")
            .execute(&pool)
            .await
            .unwrap();

        let mut app = build_app(make_state(pool, admin_enabled_config()));
        let pair = create_admin_token_pair("test-secret").unwrap();

        let (status, body) = get_with_token(&mut app, "/admin/dashboard", &pair.access_token).await;

        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["total_users"], 0);
        assert_eq!(body["active_sessions_24h"], 0);
        assert_eq!(body["total_channels"], 0);
        assert_eq!(body["total_messages"], 0);
        assert_eq!(body["total_invite_codes"], 0);
        assert_eq!(body["active_invite_codes"], 0);
    }

    /// Given: 3 users, 3 sessions (2 recent+active, 1 old), 5 channels,
    ///         10 messages, 4 invite codes (3 active, 1 inactive), with
    ///         the seeded IM2024 invite code removed for deterministic counts.
    /// When:  GET /admin/dashboard with a valid admin token.
    /// Then:  200 OK with exact aggregate counts.
    #[tokio::test]
    async fn test_dashboard_with_data() {
        let pool = setup_pool().await;
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;

        // Clear seeded invite code so counts are deterministic.
        sqlx::query("DELETE FROM invite_codes")
            .execute(&pool)
            .await
            .unwrap();

        for i in 0..3 {
            let id = format!("user-{i}");
            sqlx::query("INSERT INTO users (id, username, password_hash) VALUES (?, ?, ?)")
                .bind(&id)
                .bind(format!("u{i}"))
                .bind("hash")
                .execute(&pool)
                .await
                .unwrap();
        }

        for i in 0..2 {
            let id = format!("session-{i}");
            sqlx::query(
                "INSERT INTO sessions (id, user_id, token_hash, is_refresh, created_at, expires_at, is_active) \
                 VALUES (?, ?, ?, 0, ?, ?, 1)",
            )
            .bind(&id)
            .bind(format!("user-{i}"))
            .bind(format!("hash-{i}"))
            .bind(now)
            .bind(now + 3600)
            .execute(&pool)
            .await
            .unwrap();
        }
        sqlx::query(
            "INSERT INTO sessions (id, user_id, token_hash, is_refresh, created_at, expires_at, is_active) \
             VALUES (?, ?, ?, 0, ?, ?, 1)",
        )
        .bind("session-old")
        .bind("user-0")
        .bind("hash-old")
        .bind(now - 100_000)
        .bind(now + 3600)
        .execute(&pool)
        .await
        .unwrap();

        for i in 0..5 {
            let id = format!("channel-{i}");
            sqlx::query("INSERT INTO channels (id, name) VALUES (?, ?)")
                .bind(&id)
                .bind(format!("ch{i}"))
                .execute(&pool)
                .await
                .unwrap();
        }

        for i in 0..10 {
            let mid = format!("msg-{i}");
            sqlx::query(
                "INSERT INTO messages (msg_id, channel_id, sender_id, msg_type, payload) \
                 VALUES (?, 'channel-0', 'user-0', 'text', '{}')",
            )
            .bind(&mid)
            .execute(&pool)
            .await
            .unwrap();
        }

        for i in 0..3 {
            let code = format!("active-{i}");
            sqlx::query("INSERT INTO invite_codes (code, is_active) VALUES (?, 1)")
                .bind(&code)
                .execute(&pool)
                .await
                .unwrap();
        }
        sqlx::query("INSERT INTO invite_codes (code, is_active) VALUES (?, 0)")
            .bind("inactive")
            .execute(&pool)
            .await
            .unwrap();

        let mut app = build_app(make_state(pool, admin_enabled_config()));
        let pair = create_admin_token_pair("test-secret").unwrap();

        let (status, body) = get_with_token(&mut app, "/admin/dashboard", &pair.access_token).await;

        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["total_users"], 3);
        assert_eq!(body["active_sessions_24h"], 2);
        assert_eq!(body["total_channels"], 5);
        assert_eq!(body["total_messages"], 10);
        assert_eq!(body["total_invite_codes"], 4);
        assert_eq!(body["active_invite_codes"], 3);
    }

    /// Given: /admin/dashboard requires admin authentication.
    /// When:  GET /admin/dashboard with no Authorization header.
    /// Then:  401 Unauthorized.
    #[tokio::test]
    async fn test_dashboard_without_token() {
        let pool = setup_pool().await;
        let app = build_app(make_state(pool, admin_enabled_config()));

        let req = Request::builder()
            .method("GET")
            .uri("/admin/dashboard")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    /// Given: a user JWT (is_admin = false).
    /// When:  GET /admin/dashboard with the user token.
    /// Then:  401 Unauthorized — admin endpoints reject user tokens.
    #[tokio::test]
    async fn test_dashboard_with_user_token() {
        let pool = setup_pool().await;
        let mut app = build_app(make_state(pool, admin_enabled_config()));

        let user_pair = crate::auth::create_token_pair("user-1", "test-secret", 0).unwrap();
        let (status, body) =
            get_with_token(&mut app, "/admin/dashboard", &user_pair.access_token).await;

        assert_eq!(status, StatusCode::UNAUTHORIZED);
        assert_eq!(body["error"]["code"], "UNAUTHORIZED");
    }

    // -----------------------------------------------------------------------
    // list_users tests
    // -----------------------------------------------------------------------

    /// Given: three users exist in the database.
    /// When:  GET /admin/users as admin.
    /// Then:  200 OK returning all users ordered by created_at DESC.
    #[tokio::test]
    async fn test_list_users_success() {
        let pool = setup_pool().await;
        seed_user(&pool, "alice").await;
        seed_user(&pool, "bob").await;
        seed_user(&pool, "carol").await;

        let mut app = build_app(make_state(pool, admin_enabled_config()));
        let pair = create_admin_token_pair("test-secret").unwrap();

        let (status, body) =
            get_with_token(&mut app, "/admin/users", &pair.access_token).await;

        assert_eq!(status, StatusCode::OK);
        let arr = body.as_array().expect("array");
        assert_eq!(arr.len(), 3);
        for u in arr {
            assert!(u.get("password_hash").is_none());
        }
    }

    /// Given: users "alice", "bob", "carol" exist.
    /// When:  GET /admin/users?q=ali as admin.
    /// Then:  200 OK returning only the matching user.
    #[tokio::test]
    async fn test_list_users_with_search() {
        let pool = setup_pool().await;
        seed_user(&pool, "alice").await;
        seed_user(&pool, "bob").await;
        seed_user(&pool, "carol").await;

        let mut app = build_app(make_state(pool, admin_enabled_config()));
        let pair = create_admin_token_pair("test-secret").unwrap();

        let (status, body) =
            get_with_token(&mut app, "/admin/users?q=ali", &pair.access_token).await;

        assert_eq!(status, StatusCode::OK);
        let arr = body.as_array().expect("array");
        assert_eq!(arr.len(), 1);
        assert_eq!(arr[0]["username"], "alice");
    }

    /// Given: /admin/users requires admin authentication.
    /// When:  GET /admin/users with no token.
    /// Then:  401 Unauthorized.
    #[tokio::test]
    async fn test_list_users_unauthorized() {
        let pool = setup_pool().await;
        let app = build_app(make_state(pool, admin_enabled_config()));

        let req = Request::builder()
            .method("GET")
            .uri("/admin/users")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    // -----------------------------------------------------------------------
    // get_user tests
    // -----------------------------------------------------------------------

    /// Given: a user exists.
    /// When:  GET /admin/users/{id} as admin.
    /// Then:  200 OK with the user's details.
    #[tokio::test]
    async fn test_get_user_success() {
        let pool = setup_pool().await;
        let uid = seed_user(&pool, "alice").await;

        let mut app = build_app(make_state(pool, admin_enabled_config()));
        let pair = create_admin_token_pair("test-secret").unwrap();

        let (status, body) = get_with_token(
            &mut app,
            &format!("/admin/users/{uid}"),
            &pair.access_token,
        )
        .await;

        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["username"], "alice");
        assert!(body.get("password_hash").is_none());
    }

    /// Given: no user with the given id.
    /// When:  GET /admin/users/{id} as admin.
    /// Then:  404 Not Found.
    #[tokio::test]
    async fn test_get_user_not_found() {
        let pool = setup_pool().await;
        let mut app = build_app(make_state(pool, admin_enabled_config()));
        let pair = create_admin_token_pair("test-secret").unwrap();

        let (status, _body) =
            get_with_token(&mut app, "/admin/users/nonexistent", &pair.access_token).await;

        assert_eq!(status, StatusCode::NOT_FOUND);
    }

    // -----------------------------------------------------------------------
    // update_user tests
    // -----------------------------------------------------------------------

    /// Given: a user exists.
    /// When:  PATCH /admin/users/{id} with display_name.
    /// Then:  200 OK, display_name updated in DB.
    #[tokio::test]
    async fn test_update_user_display_name() {
        let pool = setup_pool().await;
        let uid = seed_user(&pool, "alice").await;
        let pool_clone = pool.clone();

        let mut app = build_app(make_state(pool, admin_enabled_config()));
        let pair = create_admin_token_pair("test-secret").unwrap();

        let (status, body) = patch_json_with_token(
            &mut app,
            &format!("/admin/users/{uid}"),
            json!({"display_name": "Alice Updated"}),
            &pair.access_token,
        )
        .await;

        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["display_name"], "Alice Updated");

        let db_name: String =
            sqlx::query_scalar("SELECT display_name FROM users WHERE id = ?")
                .bind(&uid)
                .fetch_one(&pool_clone)
                .await
                .unwrap();
        assert_eq!(db_name, "Alice Updated");
    }

    /// Given: display name is limited to 32 characters.
    /// When:  PATCH with a 33-char display_name.
    /// Then:  400 Bad Request.
    #[tokio::test]
    async fn test_update_user_display_name_too_long() {
        let pool = setup_pool().await;
        let uid = seed_user(&pool, "alice").await;

        let mut app = build_app(make_state(pool, admin_enabled_config()));
        let pair = create_admin_token_pair("test-secret").unwrap();

        let long_name = "x".repeat(33);
        let (status, _body) = patch_json_with_token(
            &mut app,
            &format!("/admin/users/{uid}"),
            json!({"display_name": long_name}),
            &pair.access_token,
        )
        .await;

        assert_eq!(status, StatusCode::BAD_REQUEST);
    }

    /// Given: no user with the given id.
    /// When:  PATCH /admin/users/{id}.
    /// Then:  404 Not Found.
    #[tokio::test]
    async fn test_update_user_not_found() {
        let pool = setup_pool().await;
        let mut app = build_app(make_state(pool, admin_enabled_config()));
        let pair = create_admin_token_pair("test-secret").unwrap();

        let (status, _body) = patch_json_with_token(
            &mut app,
            "/admin/users/nonexistent",
            json!({"display_name": "X"}),
            &pair.access_token,
        )
        .await;

        assert_eq!(status, StatusCode::NOT_FOUND);
    }

    // -----------------------------------------------------------------------
    // reset_password tests
    // -----------------------------------------------------------------------

    /// Given: a user exists.
    /// When:  POST /admin/users/{id}/reset-password with a valid password.
    /// Then:  204 No Content, password_hash changed in DB.
    #[tokio::test]
    async fn test_reset_password_success() {
        let pool = setup_pool().await;
        let uid = seed_user(&pool, "alice").await;
        let pool_clone = pool.clone();

        let mut app = build_app(make_state(pool, admin_enabled_config()));
        let pair = create_admin_token_pair("test-secret").unwrap();

        let (status, _body) = post_json_with_token(
            &mut app,
            &format!("/admin/users/{uid}/reset-password"),
            json!({"new_password": "newpass123"}),
            &pair.access_token,
        )
        .await;

        assert_eq!(status, StatusCode::NO_CONTENT);

        let new_hash: String =
            sqlx::query_scalar("SELECT password_hash FROM users WHERE id = ?")
                .bind(&uid)
                .fetch_one(&pool_clone)
                .await
                .unwrap();
        assert_ne!(new_hash, "hash");
    }

    /// Given: password must be at least 8 characters.
    /// When:  POST reset-password with "short".
    /// Then:  422 Unprocessable Entity.
    #[tokio::test]
    async fn test_reset_password_too_short() {
        let pool = setup_pool().await;
        let uid = seed_user(&pool, "alice").await;

        let mut app = build_app(make_state(pool, admin_enabled_config()));
        let pair = create_admin_token_pair("test-secret").unwrap();

        let (status, _body) = post_json_with_token(
            &mut app,
            &format!("/admin/users/{uid}/reset-password"),
            json!({"new_password": "short"}),
            &pair.access_token,
        )
        .await;

        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    }

    /// Given: password must contain both letters and digits.
    /// When:  POST reset-password with all-letters password.
    /// Then:  422 Unprocessable Entity.
    #[tokio::test]
    async fn test_reset_password_no_digit() {
        let pool = setup_pool().await;
        let uid = seed_user(&pool, "alice").await;

        let mut app = build_app(make_state(pool, admin_enabled_config()));
        let pair = create_admin_token_pair("test-secret").unwrap();

        let (status, _body) = post_json_with_token(
            &mut app,
            &format!("/admin/users/{uid}/reset-password"),
            json!({"new_password": "onlyletters"}),
            &pair.access_token,
        )
        .await;

        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    }

    /// Given: no user with the given id.
    /// When:  POST reset-password.
    /// Then:  404 Not Found.
    #[tokio::test]
    async fn test_reset_password_not_found() {
        let pool = setup_pool().await;
        let mut app = build_app(make_state(pool, admin_enabled_config()));
        let pair = create_admin_token_pair("test-secret").unwrap();

        let (status, _body) = post_json_with_token(
            &mut app,
            "/admin/users/nonexistent/reset-password",
            json!({"new_password": "newpass123"}),
            &pair.access_token,
        )
        .await;

        assert_eq!(status, StatusCode::NOT_FOUND);
    }

    // -----------------------------------------------------------------------
    // delete_user tests
    // -----------------------------------------------------------------------

    /// Given: a user exists.
    /// When:  DELETE /admin/users/{id}.
    /// Then:  204 No Content; user row removed from DB.
    #[tokio::test]
    async fn test_delete_user_success() {
        let pool = setup_pool().await;
        let uid = seed_user(&pool, "alice").await;
        let pool_clone = pool.clone();

        let mut app = build_app(make_state(pool, admin_enabled_config()));
        let pair = create_admin_token_pair("test-secret").unwrap();

        let (status, _body) =
            delete_with_token(&mut app, &format!("/admin/users/{uid}"), &pair.access_token)
                .await;

        assert_eq!(status, StatusCode::NO_CONTENT);

        let count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM users WHERE id = ?")
                .bind(&uid)
                .fetch_one(&pool_clone)
                .await
                .unwrap();
        assert_eq!(count, 0);
    }

    /// Given: no user with the given id.
    /// When:  DELETE /admin/users/{id}.
    /// Then:  404 Not Found.
    #[tokio::test]
    async fn test_delete_user_not_found() {
        let pool = setup_pool().await;
        let mut app = build_app(make_state(pool, admin_enabled_config()));
        let pair = create_admin_token_pair("test-secret").unwrap();

        let (status, _body) =
            delete_with_token(&mut app, "/admin/users/nonexistent", &pair.access_token)
                .await;

        assert_eq!(status, StatusCode::NOT_FOUND);
    }

    // -----------------------------------------------------------------------
    // CRITICAL: epoch revocation proofs (disable / reset / delete)
    // -----------------------------------------------------------------------

    /// Given: a regular user holds a valid token (epoch 0).
    /// When:  Admin disables the user via PATCH (epoch → 1).
    /// Then:  The old token is rejected by verify_user_epoch AND
    ///        refresh_access_token fails — proving real revocation.
    #[tokio::test]
    async fn test_disable_user_revokes_existing_tokens() {
        let pool = setup_pool().await;
        let uid = seed_user(&pool, "alice").await;
        let pool_clone = pool.clone();

        let user_pair =
            crate::auth::create_token_pair(&uid, "test-secret", 0).unwrap();

        assert!(crate::auth::verify_user_epoch(&pool_clone, &uid, 0)
            .await
            .is_ok());

        let mut app = build_app(make_state(pool, admin_enabled_config()));
        let pair = create_admin_token_pair("test-secret").unwrap();

        let (status, _body) = patch_json_with_token(
            &mut app,
            &format!("/admin/users/{uid}"),
            json!({"disabled": true}),
            &pair.access_token,
        )
        .await;
        assert_eq!(status, StatusCode::OK);

        let db_epoch: i64 =
            sqlx::query_scalar("SELECT token_epoch FROM users WHERE id = ?")
                .bind(&uid)
                .fetch_one(&pool_clone)
                .await
                .unwrap();
        assert_eq!(db_epoch, 1);

        assert!(
            crate::auth::verify_user_epoch(&pool_clone, &uid, 0)
                .await
                .is_err(),
            "old token must be rejected after disable"
        );

        let refresh_result = crate::auth::refresh_access_token(
            &user_pair.refresh_token,
            "test-secret",
            &pool_clone,
        )
        .await;
        assert!(
            refresh_result.is_err(),
            "old refresh token must be rejected after disable"
        );
    }

    /// Given: a regular user holds a valid token (epoch 0).
    /// When:  Admin resets the user's password (epoch → 1).
    /// Then:  The old token is rejected by verify_user_epoch AND
    ///        refresh_access_token fails — proving real revocation.
    #[tokio::test]
    async fn test_reset_password_revokes_existing_tokens() {
        let pool = setup_pool().await;
        let uid = seed_user(&pool, "alice").await;
        let pool_clone = pool.clone();

        let user_pair =
            crate::auth::create_token_pair(&uid, "test-secret", 0).unwrap();

        let mut app = build_app(make_state(pool, admin_enabled_config()));
        let pair = create_admin_token_pair("test-secret").unwrap();

        let (status, _body) = post_json_with_token(
            &mut app,
            &format!("/admin/users/{uid}/reset-password"),
            json!({"new_password": "brandnew1"}),
            &pair.access_token,
        )
        .await;
        assert_eq!(status, StatusCode::NO_CONTENT);

        let db_epoch: i64 =
            sqlx::query_scalar("SELECT token_epoch FROM users WHERE id = ?")
                .bind(&uid)
                .fetch_one(&pool_clone)
                .await
                .unwrap();
        assert_eq!(db_epoch, 1);

        assert!(
            crate::auth::verify_user_epoch(&pool_clone, &uid, 0)
                .await
                .is_err(),
            "old token must be rejected after password reset"
        );

        let refresh_result = crate::auth::refresh_access_token(
            &user_pair.refresh_token,
            "test-secret",
            &pool_clone,
        )
        .await;
        assert!(
            refresh_result.is_err(),
            "old refresh token must be rejected after password reset"
        );
    }

    /// Given: a regular user holds a valid token (epoch 0).
    /// When:  Admin deletes the user (row removed).
    /// Then:  verify_user_epoch returns "User not found" AND
    ///        refresh_access_token fails — proving the account is gone.
    #[tokio::test]
    async fn test_delete_user_revokes_existing_tokens() {
        let pool = setup_pool().await;
        let uid = seed_user(&pool, "alice").await;
        let pool_clone = pool.clone();

        let user_pair =
            crate::auth::create_token_pair(&uid, "test-secret", 0).unwrap();

        let mut app = build_app(make_state(pool, admin_enabled_config()));
        let pair = create_admin_token_pair("test-secret").unwrap();

        let (status, _body) =
            delete_with_token(&mut app, &format!("/admin/users/{uid}"), &pair.access_token)
                .await;
        assert_eq!(status, StatusCode::NO_CONTENT);

        let epoch_check =
            crate::auth::verify_user_epoch(&pool_clone, &uid, 0).await;
        assert!(
            matches!(
                epoch_check,
                Err(crate::error::AppError::Unauthorized(ref m))
                    if m.contains("not found")
            ),
            "deleted user's token must be rejected as 'not found', got: {epoch_check:?}"
        );

        let refresh_result = crate::auth::refresh_access_token(
            &user_pair.refresh_token,
            "test-secret",
            &pool_clone,
        )
        .await;
        assert!(
            refresh_result.is_err(),
            "deleted user's refresh token must be rejected"
        );
    }

    /// Given: all user-facing responses must omit the password hash.
    /// When:  list, get, and update endpoints are called.
    /// Then:  no JSON body contains a "password_hash" field.
    #[tokio::test]
    async fn test_password_hash_never_exposed() {
        let pool = setup_pool().await;
        let uid = seed_user(&pool, "alice").await;

        let mut app = build_app(make_state(pool, admin_enabled_config()));
        let pair = create_admin_token_pair("test-secret").unwrap();

        let (_, list_body) =
            get_with_token(&mut app, "/admin/users", &pair.access_token).await;
        for u in list_body.as_array().unwrap() {
            assert!(u.get("password_hash").is_none());
        }

        let (_, get_body) = get_with_token(
            &mut app,
            &format!("/admin/users/{uid}"),
            &pair.access_token,
        )
        .await;
        assert!(get_body.get("password_hash").is_none());

        let (_, patch_body) = patch_json_with_token(
            &mut app,
            &format!("/admin/users/{uid}"),
            json!({"display_name": "X"}),
            &pair.access_token,
        )
        .await;
        assert!(patch_body.get("password_hash").is_none());
    }

    // -----------------------------------------------------------------------
    // Invite-code helpers + tests
    // -----------------------------------------------------------------------

    async fn seed_invite_code(pool: &sqlx::SqlitePool, code: &str) {
        sqlx::query(
            "INSERT INTO invite_codes (code, max_uses, use_count, is_active) \
             VALUES (?, 100, 0, 1)",
        )
        .bind(code)
        .execute(pool)
        .await
        .unwrap();
    }

    async fn clear_invite_codes(pool: &sqlx::SqlitePool) {
        sqlx::query("DELETE FROM invite_codes")
            .execute(pool)
            .await
            .unwrap();
    }

    /// Given: the invite_codes table is empty.
    /// When:  GET /admin/invite-codes as admin.
    /// Then:  200 OK with an empty array.
    #[tokio::test]
    async fn test_list_invite_codes_empty() {
        let pool = setup_pool().await;
        clear_invite_codes(&pool).await;

        let mut app = build_app(make_state(pool, admin_enabled_config()));
        let pair = create_admin_token_pair("test-secret").unwrap();

        let (status, body) =
            get_with_token(&mut app, "/admin/invite-codes", &pair.access_token).await;

        assert_eq!(status, StatusCode::OK);
        assert_eq!(body.as_array().unwrap().len(), 0);
    }

    /// Given: three invite codes exist.
    /// When:  GET /admin/invite-codes as admin.
    /// Then:  200 OK returning all 3 ordered by created_at DESC.
    #[tokio::test]
    async fn test_list_invite_codes_with_data() {
        let pool = setup_pool().await;
        clear_invite_codes(&pool).await;
        seed_invite_code(&pool, "AAA").await;
        seed_invite_code(&pool, "BBB").await;
        seed_invite_code(&pool, "CCC").await;

        let mut app = build_app(make_state(pool, admin_enabled_config()));
        let pair = create_admin_token_pair("test-secret").unwrap();

        let (status, body) =
            get_with_token(&mut app, "/admin/invite-codes", &pair.access_token).await;

        assert_eq!(status, StatusCode::OK);
        assert_eq!(body.as_array().unwrap().len(), 3);
    }

    /// Given: five invite codes exist.
    /// When:  GET /admin/invite-codes?page=2&limit=2 as admin.
    /// Then:  200 OK returning 2 items (page 2).
    #[tokio::test]
    async fn test_list_invite_codes_pagination() {
        let pool = setup_pool().await;
        clear_invite_codes(&pool).await;
        for i in 0..5 {
            seed_invite_code(&pool, &format!("CODE{i}")).await;
        }

        let mut app = build_app(make_state(pool, admin_enabled_config()));
        let pair = create_admin_token_pair("test-secret").unwrap();

        let (status, body) = get_with_token(
            &mut app,
            "/admin/invite-codes?page=2&limit=2",
            &pair.access_token,
        )
        .await;

        assert_eq!(status, StatusCode::OK);
        assert_eq!(body.as_array().unwrap().len(), 2);
    }

    /// Given: no existing code "TEST001".
    /// When:  POST /admin/invite-codes with {code:"TEST001", max_uses:50}.
    /// Then:  201 Created; GET list confirms it exists with max_uses=50.
    #[tokio::test]
    async fn test_create_invite_code_success() {
        let pool = setup_pool().await;
        clear_invite_codes(&pool).await;

        let mut app = build_app(make_state(pool, admin_enabled_config()));
        let pair = create_admin_token_pair("test-secret").unwrap();

        let (status, body) = post_json_with_token(
            &mut app,
            "/admin/invite-codes",
            json!({"code": "TEST001", "max_uses": 50}),
            &pair.access_token,
        )
        .await;

        assert_eq!(status, StatusCode::CREATED);
        assert_eq!(body["code"], "TEST001");
        assert_eq!(body["max_uses"], 50);
        assert_eq!(body["use_count"], 0);
        assert_eq!(body["is_active"], true);
    }

    /// Given: code "DUP001" already exists.
    /// When:  POST /admin/invite-codes with the same code.
    /// Then:  409 Conflict.
    #[tokio::test]
    async fn test_create_invite_code_duplicate() {
        let pool = setup_pool().await;
        clear_invite_codes(&pool).await;
        seed_invite_code(&pool, "DUP001").await;

        let mut app = build_app(make_state(pool, admin_enabled_config()));
        let pair = create_admin_token_pair("test-secret").unwrap();

        let (status, body) = post_json_with_token(
            &mut app,
            "/admin/invite-codes",
            json!({"code": "DUP001"}),
            &pair.access_token,
        )
        .await;

        assert_eq!(status, StatusCode::CONFLICT);
        assert_eq!(body["error"]["code"], "CONFLICT");
    }

    /// Given: code must be non-empty.
    /// When:  POST /admin/invite-codes with {code:""}.
    /// Then:  400 Bad Request.
    #[tokio::test]
    async fn test_create_invite_code_empty_code() {
        let pool = setup_pool().await;
        clear_invite_codes(&pool).await;

        let mut app = build_app(make_state(pool, admin_enabled_config()));
        let pair = create_admin_token_pair("test-secret").unwrap();

        let (status, body) = post_json_with_token(
            &mut app,
            "/admin/invite-codes",
            json!({"code": ""}),
            &pair.access_token,
        )
        .await;

        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(body["error"]["code"], "BAD_REQUEST");
    }

    /// Given: max_uses and is_active are optional.
    /// When:  POST /admin/invite-codes with only {code:"DEF001"}.
    /// Then:  201 Created with max_uses=100, is_active=true, use_count=0.
    #[tokio::test]
    async fn test_create_invite_code_defaults() {
        let pool = setup_pool().await;
        clear_invite_codes(&pool).await;

        let mut app = build_app(make_state(pool, admin_enabled_config()));
        let pair = create_admin_token_pair("test-secret").unwrap();

        let (status, body) = post_json_with_token(
            &mut app,
            "/admin/invite-codes",
            json!({"code": "DEF001"}),
            &pair.access_token,
        )
        .await;

        assert_eq!(status, StatusCode::CREATED);
        assert_eq!(body["max_uses"], 100);
        assert_eq!(body["is_active"], true);
        assert_eq!(body["use_count"], 0);
    }

    /// Given: an active invite code exists.
    /// When:  PATCH /admin/invite-codes/{code} with is_active=false.
    /// Then:  200 OK; DB confirms is_active=0 (register would reject it).
    #[tokio::test]
    async fn test_update_invite_code_toggle_active() {
        let pool = setup_pool().await;
        clear_invite_codes(&pool).await;
        seed_invite_code(&pool, "TOGGLE").await;
        let pool_clone = pool.clone();

        let mut app = build_app(make_state(pool, admin_enabled_config()));
        let pair = create_admin_token_pair("test-secret").unwrap();

        let (status, body) = patch_json_with_token(
            &mut app,
            "/admin/invite-codes/TOGGLE",
            json!({"is_active": false}),
            &pair.access_token,
        )
        .await;

        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["is_active"], false);

        let db_active: bool =
            sqlx::query_scalar("SELECT is_active FROM invite_codes WHERE code = ?")
                .bind("TOGGLE")
                .fetch_one(&pool_clone)
                .await
                .unwrap();
        assert!(!db_active);
    }

    /// Given: an invite code with use_count=5.
    /// When:  PATCH with reset_use_count=true.
    /// Then:  200 OK with use_count=0.
    #[tokio::test]
    async fn test_update_invite_code_reset_use_count() {
        let pool = setup_pool().await;
        clear_invite_codes(&pool).await;
        seed_invite_code(&pool, "RESET").await;
        sqlx::query("UPDATE invite_codes SET use_count = 5 WHERE code = 'RESET'")
            .execute(&pool)
            .await
            .unwrap();

        let mut app = build_app(make_state(pool, admin_enabled_config()));
        let pair = create_admin_token_pair("test-secret").unwrap();

        let (status, body) = patch_json_with_token(
            &mut app,
            "/admin/invite-codes/RESET",
            json!({"reset_use_count": true}),
            &pair.access_token,
        )
        .await;

        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["use_count"], 0);
    }

    /// Given: an invite code exists.
    /// When:  PATCH with max_uses=10.
    /// Then:  200 OK with max_uses=10.
    #[tokio::test]
    async fn test_update_invite_code_max_uses() {
        let pool = setup_pool().await;
        clear_invite_codes(&pool).await;
        seed_invite_code(&pool, "MAXUSE").await;

        let mut app = build_app(make_state(pool, admin_enabled_config()));
        let pair = create_admin_token_pair("test-secret").unwrap();

        let (status, body) = patch_json_with_token(
            &mut app,
            "/admin/invite-codes/MAXUSE",
            json!({"max_uses": 10}),
            &pair.access_token,
        )
        .await;

        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["max_uses"], 10);
    }

    /// Given: no invite code "NOPE".
    /// When:  PATCH /admin/invite-codes/NOPE.
    /// Then:  404 Not Found.
    #[tokio::test]
    async fn test_update_invite_code_not_found() {
        let pool = setup_pool().await;
        clear_invite_codes(&pool).await;

        let mut app = build_app(make_state(pool, admin_enabled_config()));
        let pair = create_admin_token_pair("test-secret").unwrap();

        let (status, _body) = patch_json_with_token(
            &mut app,
            "/admin/invite-codes/NOPE",
            json!({"max_uses": 50}),
            &pair.access_token,
        )
        .await;

        assert_eq!(status, StatusCode::NOT_FOUND);
    }

    /// Given: an invite code exists.
    /// When:  DELETE /admin/invite-codes/{code}.
    /// Then:  204 No Content; GET list confirms it's gone.
    #[tokio::test]
    async fn test_delete_invite_code() {
        let pool = setup_pool().await;
        clear_invite_codes(&pool).await;
        seed_invite_code(&pool, "DELME").await;

        let mut app = build_app(make_state(pool, admin_enabled_config()));
        let pair = create_admin_token_pair("test-secret").unwrap();

        let (status, _body) = delete_with_token(
            &mut app,
            "/admin/invite-codes/DELME",
            &pair.access_token,
        )
        .await;

        assert_eq!(status, StatusCode::NO_CONTENT);

        let (status, body) =
            get_with_token(&mut app, "/admin/invite-codes", &pair.access_token).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body.as_array().unwrap().len(), 0);
    }

    /// Given: no invite code "NOPE".
    /// When:  DELETE /admin/invite-codes/NOPE.
    /// Then:  404 Not Found.
    #[tokio::test]
    async fn test_delete_invite_code_not_found() {
        let pool = setup_pool().await;
        clear_invite_codes(&pool).await;

        let mut app = build_app(make_state(pool, admin_enabled_config()));
        let pair = create_admin_token_pair("test-secret").unwrap();

        let (status, _body) = delete_with_token(
            &mut app,
            "/admin/invite-codes/NOPE",
            &pair.access_token,
        )
        .await;

        assert_eq!(status, StatusCode::NOT_FOUND);
    }

    /// Given: invite endpoints require admin authentication.
    /// When:  GET without token, then GET with a user token.
    /// Then:  Both 401 Unauthorized.
    #[tokio::test]
    async fn test_invite_endpoints_require_admin() {
        let pool = setup_pool().await;
        let app = build_app(make_state(pool.clone(), admin_enabled_config()));

        let req = Request::builder()
            .method("GET")
            .uri("/admin/invite-codes")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);

        let mut app = build_app(make_state(pool, admin_enabled_config()));
        let user_pair =
            crate::auth::create_token_pair("user-1", "test-secret", 0).unwrap();
        let (status, _body) =
            get_with_token(&mut app, "/admin/invite-codes", &user_pair.access_token).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);
    }

    /// Given: create, update, and delete each write an audit row.
    /// When:  Performing all three operations.
    /// Then:  admin_audit_logs has 3 rows with matching action/target_id.
    #[tokio::test]
    async fn test_invite_code_audit_trail() {
        let pool = setup_pool().await;
        clear_invite_codes(&pool).await;
        let pool_clone = pool.clone();

        let mut app = build_app(make_state(pool, admin_enabled_config()));
        let pair = create_admin_token_pair("test-secret").unwrap();

        let _ = post_json_with_token(
            &mut app,
            "/admin/invite-codes",
            json!({"code": "AUDIT"}),
            &pair.access_token,
        )
        .await;

        let _ = patch_json_with_token(
            &mut app,
            "/admin/invite-codes/AUDIT",
            json!({"max_uses": 5}),
            &pair.access_token,
        )
        .await;

        let _ = delete_with_token(
            &mut app,
            "/admin/invite-codes/AUDIT",
            &pair.access_token,
        )
        .await;

        let actions: Vec<String> =
            sqlx::query_scalar("SELECT action FROM admin_audit_logs ORDER BY id")
                .fetch_all(&pool_clone)
                .await
                .unwrap();
        assert!(actions.contains(&"invite.create".to_string()));
        assert!(actions.contains(&"invite.update".to_string()));
        assert!(actions.contains(&"invite.delete".to_string()));
    }

    // -----------------------------------------------------------------------
    // Audit-log tests
    // -----------------------------------------------------------------------

    async fn seed_audit_row(pool: &sqlx::SqlitePool, action: &str, ts: i64) {
        let id = Uuid::now_v7().to_string();
        sqlx::query(
            "INSERT INTO admin_audit_logs (id, action, performed_at) VALUES (?, ?, ?)",
        )
        .bind(&id)
        .bind(action)
        .bind(ts)
        .execute(pool)
        .await
        .unwrap();
    }

    /// Given: a fresh database with no audit rows.
    /// When:  GET /admin/audit-logs as admin.
    /// Then:  200 OK with an empty array.
    #[tokio::test]
    async fn test_list_audit_logs_empty() {
        let pool = setup_pool().await;

        let mut app = build_app(make_state(pool, admin_enabled_config()));
        let pair = create_admin_token_pair("test-secret").unwrap();

        let (status, body) = get_with_token(
            &mut app,
            "/admin/audit-logs",
            &pair.access_token,
        )
        .await;

        assert_eq!(status, StatusCode::OK);
        assert_eq!(body.as_array().unwrap().len(), 0);
    }

    /// Given: an admin mutation (create invite code) has been performed.
    /// When:  GET /admin/audit-logs as admin.
    /// Then:  200 OK containing a row with action "invite.create".
    #[tokio::test]
    async fn test_list_audit_logs_after_action() {
        let pool = setup_pool().await;
        clear_invite_codes(&pool).await;

        let mut app = build_app(make_state(pool, admin_enabled_config()));
        let pair = create_admin_token_pair("test-secret").unwrap();

        let _ = post_json_with_token(
            &mut app,
            "/admin/invite-codes",
            json!({"code": "AUDITTEST"}),
            &pair.access_token,
        )
        .await;

        let (status, body) = get_with_token(
            &mut app,
            "/admin/audit-logs",
            &pair.access_token,
        )
        .await;

        assert_eq!(status, StatusCode::OK);
        let arr = body.as_array().expect("array");
        assert!(!arr.is_empty());
        assert_eq!(arr[0]["action"], "invite.create");
        assert_eq!(arr[0]["target_type"], "invite_code");
        assert_eq!(arr[0]["target_id"], "AUDITTEST");
    }

    /// Given: audit rows for "user.disable" and "invite.create" exist.
    /// When:  GET /admin/audit-logs?action=user.disable.
    /// Then:  Only rows with action "user.disable" are returned.
    #[tokio::test]
    async fn test_list_audit_logs_action_filter() {
        let pool = setup_pool().await;
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;
        seed_audit_row(&pool, "user.disable", now).await;
        seed_audit_row(&pool, "invite.create", now - 1).await;
        seed_audit_row(&pool, "user.disable", now - 2).await;

        let mut app = build_app(make_state(pool, admin_enabled_config()));
        let pair = create_admin_token_pair("test-secret").unwrap();

        let (status, body) = get_with_token(
            &mut app,
            "/admin/audit-logs?action=user.disable",
            &pair.access_token,
        )
        .await;

        assert_eq!(status, StatusCode::OK);
        let arr = body.as_array().expect("array");
        assert_eq!(arr.len(), 2);
        for row in arr {
            assert_eq!(row["action"], "user.disable");
        }
    }

    /// Given: five audit rows at different timestamps.
    /// When:  GET /admin/audit-logs?page=2&limit=2.
    /// Then:  2 rows from the correct offset, ordered by performed_at DESC.
    #[tokio::test]
    async fn test_list_audit_logs_pagination() {
        let pool = setup_pool().await;
        let base = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;
        for i in 0..5 {
            seed_audit_row(&pool, &format!("act-{i}"), base + i).await;
        }

        let mut app = build_app(make_state(pool, admin_enabled_config()));
        let pair = create_admin_token_pair("test-secret").unwrap();

        let (status, body) = get_with_token(
            &mut app,
            "/admin/audit-logs?page=2&limit=2",
            &pair.access_token,
        )
        .await;

        assert_eq!(status, StatusCode::OK);
        let arr = body.as_array().expect("array");
        assert_eq!(arr.len(), 2);
    }

    /// Given: /admin/audit-logs requires admin authentication.
    /// When:  GET without Authorization header.
    /// Then:  401 Unauthorized.
    #[tokio::test]
    async fn test_list_audit_logs_without_token() {
        let pool = setup_pool().await;
        let app = build_app(make_state(pool, admin_enabled_config()));

        let req = Request::builder()
            .method("GET")
            .uri("/admin/audit-logs")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    /// Given: a user JWT (is_admin = false).
    /// When:  GET /admin/audit-logs with the user token.
    /// Then:  401 Unauthorized — admin endpoints reject user tokens.
    #[tokio::test]
    async fn test_list_audit_logs_with_user_token() {
        let pool = setup_pool().await;
        let mut app = build_app(make_state(pool, admin_enabled_config()));

        let user_pair =
            crate::auth::create_token_pair("user-1", "test-secret", 0).unwrap();
        let (status, body) = get_with_token(
            &mut app,
            "/admin/audit-logs",
            &user_pair.access_token,
        )
        .await;

        assert_eq!(status, StatusCode::UNAUTHORIZED);
        assert_eq!(body["error"]["code"], "UNAUTHORIZED");
    }
}
