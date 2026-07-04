pub mod admin;
pub mod middleware;
pub use middleware::AuthenticatedUser;

use argon2::{
    password_hash::{rand_core::OsRng, PasswordHash, PasswordHasher, PasswordVerifier, SaltString},
    Argon2,
};
use chrono::Utc;
use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use uuid::Uuid;

use crate::error::AppError;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Claims {
    pub sub: String,        // user_id
    pub exp: usize,         // expiry
    pub iat: usize,         // issued at
    pub jti: String,        // token ID
    pub is_refresh: bool,   // refresh token flag
    /// Epoch counter for forced logout. Tokens embed the epoch they were
    /// minted with; the auth layer rejects any token whose embedded epoch
    /// is older than the `users.token_epoch` column. `#[serde(default)]`
    /// so legacy tokens (pre-epoch) parse as epoch 0.
    #[serde(default)]
    pub epoch: i64,
    /// Admin-backend isolation flag. `true` marks a token minted by the
    /// admin login flow; such tokens are rejected by `validate_token` for
    /// user-facing endpoints. `#[serde(default)]` (false) for user tokens.
    #[serde(default)]
    pub is_admin: bool,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct TokenPair {
    pub access_token: String,
    pub refresh_token: String,
    pub expires_in: u32, // seconds
}

pub(crate) const ACCESS_TTL_SECS: u32 = 900;  // 15 minutes
pub(crate) const REFRESH_TTL_SECS: u32 = 604800; // 7 days

/// Create access + refresh token pair for a user.
///
/// `epoch` must be the current `users.token_epoch` value at sign time so
/// that a subsequent `token_epoch + 1` invalidates this pair.
pub fn create_token_pair(
    user_id: &str,
    secret: &str,
    epoch: i64,
) -> Result<TokenPair, jsonwebtoken::errors::Error> {
    let now = Utc::now().timestamp() as usize;

    let access_claims = Claims {
        sub: user_id.to_string(),
        exp: now + ACCESS_TTL_SECS as usize,
        iat: now,
        jti: Uuid::new_v4().to_string(),
        is_refresh: false,
        epoch,
        is_admin: false,
    };

    let refresh_claims = Claims {
        sub: user_id.to_string(),
        exp: now + REFRESH_TTL_SECS as usize,
        iat: now,
        jti: Uuid::new_v4().to_string(),
        is_refresh: true,
        epoch,
        is_admin: false,
    };

    let access_token = encode(
        &Header::default(),
        &access_claims,
        &EncodingKey::from_secret(secret.as_bytes()),
    )?;

    let refresh_token = encode(
        &Header::default(),
        &refresh_claims,
        &EncodingKey::from_secret(secret.as_bytes()),
    )?;

    Ok(TokenPair {
        access_token,
        refresh_token,
        expires_in: ACCESS_TTL_SECS,
    })
}

/// Validate any token (access or refresh) and return claims.
///
/// Rejects admin-backend tokens (`is_admin == true`) so admin JWTs cannot
/// be used against user-facing endpoints. Caller is still responsible for
/// the epoch check (see `verify_user_epoch`) — this function only verifies
/// the JWT signature, expiry, and admin isolation.
pub fn validate_token(token: &str, secret: &str) -> Result<Claims, jsonwebtoken::errors::Error> {
    let token_data = decode::<Claims>(
        token,
        &DecodingKey::from_secret(secret.as_bytes()),
        &Validation::default(),
    )?;
    if token_data.claims.is_admin {
        return Err(jsonwebtoken::errors::Error::from(
            jsonwebtoken::errors::ErrorKind::InvalidToken,
        ));
    }
    Ok(token_data.claims)
}

/// Verify that the user's current `token_epoch` in the DB matches the
/// epoch embedded in the JWT. Used by the HTTP middleware and the WS
/// upgrade handler to enforce forced logout.
///
/// Returns:
/// - `Ok(())` if the user exists and `token_epoch == claim_epoch`.
/// - `Err(Unauthorized)` if the user does not exist (deleted, or sub is
///   a non-user principal such as `"admin"`).
/// - `Err(Unauthorized)` if the epochs differ (token superseded by
///   `token_epoch + 1`).
pub async fn verify_user_epoch(
    pool: &SqlitePool,
    user_id: &str,
    claim_epoch: i64,
) -> Result<(), AppError> {
    let row = sqlx::query_scalar::<_, i64>(
        "SELECT token_epoch FROM users WHERE id = ?",
    )
    .bind(user_id)
    .fetch_optional(pool)
    .await
    .map_err(AppError::from)?;

    match row {
        Some(db_epoch) if db_epoch == claim_epoch => Ok(()),
        Some(_) => Err(AppError::Unauthorized(
            "Token has been superseded".to_string(),
        )),
        None => Err(AppError::Unauthorized("User not found".to_string())),
    }
}

/// Refresh an access token using a valid refresh token.
///
/// Verifies the refresh token signature + is_refresh flag, then queries
/// the DB for the user's current `token_epoch` and rejects the refresh
/// if the embedded epoch is stale. On success, signs a new pair using
/// the *current* DB epoch (so the new tokens track any concurrent
/// `token_epoch` increments that happened between mint and refresh).
pub async fn refresh_access_token(
    refresh_token: &str,
    secret: &str,
    pool: &SqlitePool,
) -> Result<TokenPair, AppError> {
    let claims = validate_token(refresh_token, secret).map_err(AppError::from)?;
    if !claims.is_refresh {
        return Err(AppError::Unauthorized(
            "Not a refresh token".to_string(),
        ));
    }

    let db_epoch = sqlx::query_scalar::<_, i64>(
        "SELECT token_epoch FROM users WHERE id = ?",
    )
    .bind(&claims.sub)
    .fetch_optional(pool)
    .await
    .map_err(AppError::from)?
    .ok_or_else(|| AppError::Unauthorized("User not found".to_string()))?;

    if db_epoch != claims.epoch {
        return Err(AppError::Unauthorized(
            "Token has been superseded".to_string(),
        ));
    }

    create_token_pair(&claims.sub, secret, db_epoch).map_err(AppError::from)
}

/// Hash a password with Argon2id
pub fn hash_password(password: &str) -> Result<String, argon2::password_hash::Error> {
    let salt = SaltString::generate(&mut OsRng);
    let argon2 = Argon2::default();
    let hash = argon2.hash_password(password.as_bytes(), &salt)?;
    Ok(hash.to_string())
}

/// Verify a password against an Argon2id hash
pub fn verify_password(password: &str, hash: &str) -> Result<bool, argon2::password_hash::Error> {
    let parsed_hash = PasswordHash::new(hash)?;
    Ok(Argon2::default()
        .verify_password(password.as_bytes(), &parsed_hash)
        .is_ok())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_password_hash_roundtrip() {
        let password = "SecurePass123!";
        let hash = hash_password(password).unwrap();
        assert!(verify_password(password, &hash).unwrap());
        assert!(!verify_password("WrongPass", &hash).unwrap());
    }

    #[test]
    fn test_token_creation_and_validation() {
        let secret = "test-secret";
        let pair = create_token_pair("user-1", secret, 0).unwrap();

        let claims = validate_token(&pair.access_token, secret).unwrap();
        assert_eq!(claims.sub, "user-1");
        assert!(!claims.is_refresh);
        assert_eq!(claims.epoch, 0);
        assert!(!claims.is_admin);

        let refresh_claims = validate_token(&pair.refresh_token, secret).unwrap();
        assert!(refresh_claims.is_refresh);
        assert_eq!(refresh_claims.epoch, 0);
    }

    #[test]
    fn test_token_expiry() {
        // Create a token that's already expired
        let claims = Claims {
            sub: "user-1".into(),
            exp: 0, // epoch 0 = expired
            iat: 0,
            jti: Uuid::new_v4().to_string(),
            is_refresh: false,
            epoch: 0,
            is_admin: false,
        };
        let secret = "test-secret";
        let token = encode(
            &Header::default(),
            &claims,
            &EncodingKey::from_secret(secret.as_bytes()),
        ).unwrap();

        let result = validate_token(&token, secret);
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_refresh_token_rejection() {
        let secret = "test-secret";
        let pool = sqlx::SqlitePool::connect(":memory:").await.unwrap();
        sqlx::migrate!("db/migrations").run(&pool).await.unwrap();
        sqlx::query("INSERT INTO users (id, username, password_hash) VALUES ('user-1', 'u', 'h')")
            .execute(&pool)
            .await
            .unwrap();

        let pair = create_token_pair("user-1", secret, 0).unwrap();

        // Trying to use access token as refresh token should fail
        let result = refresh_access_token(&pair.access_token, secret, &pool).await;
        assert!(result.is_err());
    }

    /// Admin-backend tokens (is_admin=true) must be rejected by
    /// validate_token so they cannot be used against user-facing endpoints.
    #[test]
    fn test_admin_token_rejected_by_validate_token() {
        let secret = "test-secret";
        let claims = Claims {
            sub: "admin".into(),
            exp: Utc::now().timestamp() as usize + 3600,
            iat: Utc::now().timestamp() as usize,
            jti: Uuid::new_v4().to_string(),
            is_refresh: false,
            epoch: 0,
            is_admin: true,
        };
        let token = encode(
            &Header::default(),
            &claims,
            &EncodingKey::from_secret(secret.as_bytes()),
        )
        .unwrap();

        let result = validate_token(&token, secret);
        assert!(result.is_err(), "admin token must be rejected");
    }

    /// A user token with a stale epoch (DB epoch > claim epoch) must be
    /// rejected by verify_user_epoch, simulating a forced logout.
    #[tokio::test]
    async fn test_verify_user_epoch_rejects_stale_token() {
        let pool = sqlx::SqlitePool::connect(":memory:").await.unwrap();
        sqlx::migrate!("db/migrations").run(&pool).await.unwrap();
        sqlx::query(
            "INSERT INTO users (id, username, password_hash, token_epoch) \
             VALUES ('user-stale', 'u', 'h', 1)",
        )
        .execute(&pool)
        .await
        .unwrap();

        // claim epoch = 0, DB epoch = 1 → mismatch → reject
        let result = verify_user_epoch(&pool, "user-stale", 0).await;
        assert!(result.is_err());

        // claim epoch = 1, DB epoch = 1 → match → ok
        let result = verify_user_epoch(&pool, "user-stale", 1).await;
        assert!(result.is_ok());
    }

    /// verify_user_epoch must reject tokens whose `sub` does not exist in
    /// the users table — covers deleted users and non-user principals
    /// like `sub = "admin"`.
    #[tokio::test]
    async fn test_verify_user_epoch_rejects_missing_user() {
        let pool = sqlx::SqlitePool::connect(":memory:").await.unwrap();
        sqlx::migrate!("db/migrations").run(&pool).await.unwrap();

        let result = verify_user_epoch(&pool, "admin", 0).await;
        assert!(result.is_err(), "sub='admin' has no users row");

        let result = verify_user_epoch(&pool, "deleted-user", 0).await;
        assert!(result.is_err(), "deleted user has no row");
    }

    /// refresh_access_token must reject a refresh token whose embedded
    /// epoch is older than the current DB value (forced logout path).
    #[tokio::test]
    async fn test_refresh_rejects_stale_epoch() {
        let pool = sqlx::SqlitePool::connect(":memory:").await.unwrap();
        sqlx::migrate!("db/migrations").run(&pool).await.unwrap();
        sqlx::query(
            "INSERT INTO users (id, username, password_hash, token_epoch) \
             VALUES ('user-1', 'u', 'h', 0)",
        )
        .execute(&pool)
        .await
        .unwrap();

        let secret = "test-secret";
        // Mint with epoch 0
        let pair = create_token_pair("user-1", secret, 0).unwrap();
        // Force logout: bump DB epoch
        sqlx::query("UPDATE users SET token_epoch = 1 WHERE id = 'user-1'")
            .execute(&pool)
            .await
            .unwrap();

        let result = refresh_access_token(&pair.refresh_token, secret, &pool).await;
        assert!(result.is_err(), "stale-epoch refresh must be rejected");
    }
}
