//! Admin-backend JWT authentication — isolated from user auth.
//!
//! Admin JWTs carry `is_admin: true` and never query the `users` table
//! (admin is not a user row). Minted/verified by dedicated helpers so the
//! user-facing `validate_token`/`AuthenticatedUser` path can reject these
//! tokens, and so admin-only endpoints reject user tokens.

use axum::{
    extract::FromRequestParts,
    http::{request::Parts, StatusCode},
    response::{IntoResponse, Json, Response},
};
use chrono::Utc;
use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::sync::Arc;
use uuid::Uuid;

use crate::auth::TokenPair;
use crate::error::AppError;
use crate::AppState;

/// JWT claims for an admin principal. `is_admin` is always `true`;
/// `epoch` is always `0` (placeholder for serde parity with user claims,
/// never consulted because admin auth skips epoch verification).
#[derive(Debug, Serialize, Deserialize)]
pub struct AdminClaims {
    pub sub: String,
    pub exp: i64,
    pub iat: i64,
    pub jti: String,
    pub is_admin: bool,
    pub is_refresh: bool,
    pub epoch: i64,
}

const ADMIN_SUBJECT: &str = "admin";

/// Mint an admin access+refresh token pair. Both tokens carry
/// `is_admin: true`, fixed `sub = "admin"`, and `epoch = 0`. TTLs match
/// the user-facing constants so expiry semantics stay uniform.
pub fn create_admin_token_pair(secret: &str) -> Result<TokenPair, AppError> {
    let now = Utc::now().timestamp();
    let access_ttl = crate::auth::ACCESS_TTL_SECS as i64;
    let refresh_ttl = crate::auth::REFRESH_TTL_SECS as i64;

    let access_claims = AdminClaims {
        sub: ADMIN_SUBJECT.to_string(),
        exp: now + access_ttl,
        iat: now,
        jti: Uuid::now_v7().to_string(),
        is_admin: true,
        is_refresh: false,
        epoch: 0,
    };

    let refresh_claims = AdminClaims {
        sub: ADMIN_SUBJECT.to_string(),
        exp: now + refresh_ttl,
        iat: now,
        jti: Uuid::now_v7().to_string(),
        is_admin: true,
        is_refresh: true,
        epoch: 0,
    };

    let access_token = encode(
        &Header::default(),
        &access_claims,
        &EncodingKey::from_secret(secret.as_bytes()),
    )
    .map_err(AppError::from)?;

    let refresh_token = encode(
        &Header::default(),
        &refresh_claims,
        &EncodingKey::from_secret(secret.as_bytes()),
    )
    .map_err(AppError::from)?;

    Ok(TokenPair {
        access_token,
        refresh_token,
        expires_in: crate::auth::ACCESS_TTL_SECS,
    })
}

/// Verify an admin JWT and return its claims. Rejects:
/// - malformed/expired/wrong-signature tokens (jsonwebtoken `decode`)
/// - tokens whose `is_admin` claim is not `true` (user tokens are
///   forbidden on admin endpoints)
pub fn validate_admin_token(token: &str, secret: &str) -> Result<AdminClaims, AppError> {
    let token_data = decode::<AdminClaims>(
        token,
        &DecodingKey::from_secret(secret.as_bytes()),
        &Validation::default(),
    )
    .map_err(AppError::from)?;

    if !token_data.claims.is_admin {
        return Err(AppError::Unauthorized(
            "Not an admin token".to_string(),
        ));
    }
    Ok(token_data.claims)
}

/// Refresh an admin token pair. Validates the refresh token, requires
/// `is_refresh == true`, and signs a fresh admin pair (new jtis, new exp).
pub fn refresh_admin_token(refresh: &str, secret: &str) -> Result<TokenPair, AppError> {
    let claims = validate_admin_token(refresh, secret)?;
    if !claims.is_refresh {
        return Err(AppError::Unauthorized(
            "Not a refresh token".to_string(),
        ));
    }
    create_admin_token_pair(secret)
}

/// Extractor for admin-only routes. Pulls a Bearer token from the
/// Authorization header and runs it through `validate_admin_token`.
/// No DB lookup is performed — admin principals are not stored in the
/// `users` table.
#[derive(Debug)]
pub struct AdminAuthenticatedUser(pub String);

impl FromRequestParts<Arc<AppState>> for AdminAuthenticatedUser {
    type Rejection = Response;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &Arc<AppState>,
    ) -> Result<Self, Self::Rejection> {
        let token = parts
            .headers
            .get("authorization")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.strip_prefix("Bearer "));

        match token {
            Some(token) => match validate_admin_token(token, &state.config.jwt_secret) {
                Ok(claims) => Ok(AdminAuthenticatedUser(claims.sub)),
                Err(_) => Err((
                    StatusCode::UNAUTHORIZED,
                    Json(json!({"error": {"code": "UNAUTHORIZED", "message": "Invalid or expired token"}})),
                )
                    .into_response()),
            },
            None => Err((
                StatusCode::UNAUTHORIZED,
                Json(json!({"error": {"code": "UNAUTHORIZED", "message": "Missing authorization header"}})),
            )
                .into_response()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::{create_token_pair, validate_token, Claims};
    use crate::{AppConfig, AppState};
    use axum::body::Body;
    use axum::http::{header, Request};

    const SECRET: &str = "test-secret";

    /// Given: create_admin_token_pair mints a pair.
    /// When:  both tokens are validated.
    /// Then:  sub="admin", is_admin=true, access is_refresh=false, refresh
    ///        is_refresh=true, epoch=0.
    #[test]
    fn admin_pair_roundtrip_validates_both_tokens() {
        let pair = create_admin_token_pair(SECRET).expect("pair creation");

        let access = validate_admin_token(&pair.access_token, SECRET).expect("access validates");
        assert_eq!(access.sub, ADMIN_SUBJECT);
        assert!(access.is_admin);
        assert!(!access.is_refresh);
        assert_eq!(access.epoch, 0);

        let refresh = validate_admin_token(&pair.refresh_token, SECRET).expect("refresh validates");
        assert_eq!(refresh.sub, ADMIN_SUBJECT);
        assert!(refresh.is_admin);
        assert!(refresh.is_refresh);
        assert_eq!(refresh.epoch, 0);

        assert!(
            pair.expires_in > 0,
            "expires_in must be a positive duration"
        );
    }

    /// Given: a user token minted by `create_token_pair` (is_admin=false).
    /// When:  validate_admin_token runs.
    /// Then:  it rejects the token — admin endpoints must not accept user JWTs.
    #[test]
    fn user_token_rejected_by_validate_admin_token() {
        let user_pair = create_token_pair("user-1", SECRET, 0).expect("user pair");
        let err = validate_admin_token(&user_pair.access_token, SECRET)
            .expect_err("user token must be rejected by admin path");
        assert!(matches!(err, AppError::Unauthorized(_)));
    }

    /// Given: a token signed with a different secret.
    /// When:  validate_admin_token runs.
    /// Then:  signature verification fails → Unauthorized.
    #[test]
    fn admin_token_wrong_secret_rejected() {
        let pair = create_admin_token_pair(SECRET).expect("pair");
        let err = validate_admin_token(&pair.access_token, "other-secret")
            .expect_err("wrong-secret token must be rejected");
        assert!(matches!(err, AppError::Unauthorized(_)));
    }

    /// Given: an admin token with exp in the past.
    /// When:  validate_admin_token runs.
    /// Then:  jsonwebtoken's expiry check rejects it.
    #[test]
    fn expired_admin_token_rejected() {
        let claims = AdminClaims {
            sub: ADMIN_SUBJECT.into(),
            exp: 1,
            iat: 0,
            jti: Uuid::now_v7().to_string(),
            is_admin: true,
            is_refresh: false,
            epoch: 0,
        };
        let token = encode(
            &Header::default(),
            &claims,
            &EncodingKey::from_secret(SECRET.as_bytes()),
        )
        .expect("encode");
        assert!(validate_admin_token(&token, SECRET).is_err());
    }

    /// Given: a tampered token (last char of payload swapped).
    /// When:  validate_admin_token runs.
    /// Then:  signature verification fails.
    #[test]
    fn tampered_admin_token_rejected() {
        let pair = create_admin_token_pair(SECRET).expect("pair");
        let mut token = pair.access_token;
        // Flip the final non-`.` character of the signature segment.
        let last_idx = token.len().saturating_sub(2);
        let original = token.as_bytes()[last_idx];
        let flipped = if original == b'A' { b'B' } else { b'A' };
        // SAFETY: ASCII chars in JWT signature, flipping stays ASCII.
        let mut bytes = std::mem::take(&mut token).into_bytes();
        if let Some(byte) = bytes.get_mut(last_idx) {
            *byte = flipped;
        }
        token = String::from_utf8(bytes).expect("ASCII");
        assert!(validate_admin_token(&token, SECRET).is_err());
    }

    /// Given: a valid admin refresh token.
    /// When:  refresh_admin_token runs.
    /// Then:  a new pair is returned, both new tokens validate, and the new
    ///        access token differs from the original (new jti).
    #[test]
    fn admin_refresh_roundtrip_produces_new_pair() {
        let original = create_admin_token_pair(SECRET).expect("pair");
        let refreshed = refresh_admin_token(&original.refresh_token, SECRET).expect("refresh");

        assert_ne!(
            refreshed.access_token, original.access_token,
            "refresh must mint a new access token"
        );
        assert_ne!(
            refreshed.refresh_token, original.refresh_token,
            "refresh must mint a new refresh token"
        );

        let access = validate_admin_token(&refreshed.access_token, SECRET).expect("new access");
        assert!(access.is_admin);
        assert!(!access.is_refresh);
        assert_eq!(access.sub, ADMIN_SUBJECT);
    }

    /// Given: an admin access token passed as a refresh.
    /// When:  refresh_admin_token runs.
    /// Then:  Unauthorized (is_refresh==false).
    #[test]
    fn admin_refresh_rejects_access_token() {
        let pair = create_admin_token_pair(SECRET).expect("pair");
        let err = refresh_admin_token(&pair.access_token, SECRET)
            .expect_err("access token must not be usable as refresh");
        assert!(matches!(err, AppError::Unauthorized(_)));
    }

    /// Given: a user refresh token (is_admin=false, is_refresh=true).
    /// When:  refresh_admin_token runs.
    /// Then:  Unauthorized — validate_admin_token rejects user tokens first.
    #[test]
    fn user_refresh_token_rejected_by_refresh_admin_token() {
        let user_pair = create_token_pair("user-1", SECRET, 0).expect("user pair");
        let err = refresh_admin_token(&user_pair.refresh_token, SECRET)
            .expect_err("user refresh must be rejected");
        assert!(matches!(err, AppError::Unauthorized(_)));
    }

    /// Given: admin access token signed with the server's own JWT secret.
    /// When:  user-facing `validate_token` is called on it.
    /// Then:  it rejects the admin token (user endpoints must not accept
    ///        admin JWTs). This proves the cross-isolation is symmetric:
    ///        user path rejects admin tokens AND admin path rejects user tokens.
    #[test]
    fn admin_token_rejected_by_user_validate_token() {
        let pair = create_admin_token_pair(SECRET).expect("pair");
        assert!(
            validate_token(&pair.access_token, SECRET).is_err(),
            "user-facing validate_token must reject admin tokens"
        );
    }

    /// Given: admin access token serialized as raw `Claims`.
    /// When:  decoded through user-facing Claims schema.
    /// Then:  is_admin==true is preserved across the two schemas (the
    ///        struct shapes are intentionally compatible for serde parity).
    #[test]
    fn admin_claims_decode_through_user_claims_schema() {
        let pair = create_admin_token_pair(SECRET).expect("pair");
        let decoded = decode::<Claims>(
            &pair.access_token,
            &DecodingKey::from_secret(SECRET.as_bytes()),
            &Validation::default(),
        )
        .expect("user-schema decode");
        assert!(decoded.claims.is_admin);
    }

    // ----- Extractor scenarios ----------------------------------------------

    async fn run_extractor(
        state: Arc<AppState>,
        auth_header: Option<&str>,
    ) -> Result<AdminAuthenticatedUser, Response> {
        let mut req_builder = Request::builder().method("GET").uri("/admin/whatever");
        if let Some(value) = auth_header {
            req_builder = req_builder.header(header::AUTHORIZATION, value);
        }
        let request = req_builder.body(Body::empty()).expect("request build");
        let (mut parts, _body) = request.into_parts();
        AdminAuthenticatedUser::from_request_parts(&mut parts, &state).await
    }

    /// Given: a valid admin Bearer token in Authorization header.
    /// When:  the extractor runs.
    /// Then:  it returns AdminAuthenticatedUser("admin") without touching the DB.
    #[tokio::test]
    async fn extractor_accepts_valid_admin_token() {
        let state = extractor_state(SECRET).await;
        let pair = create_admin_token_pair(SECRET).expect("pair");
        let value = format!("Bearer {}", pair.access_token);

        let principal = run_extractor(state, Some(&value))
            .await
            .expect("valid admin token must authenticate");
        assert_eq!(principal.0, ADMIN_SUBJECT);
    }

    /// Given: no Authorization header on the request.
    /// When:  the extractor runs.
    /// Then:  401 rejection with "Missing authorization header".
    #[tokio::test]
    async fn extractor_rejects_missing_header() {
        let state = extractor_state(SECRET).await;
        let rejection = run_extractor(state, None)
            .await
            .expect_err("missing header must reject");
        assert_eq!(rejection.status(), StatusCode::UNAUTHORIZED);
    }

    /// Given: a user token (is_admin=false) in Authorization header.
    /// When:  the extractor runs.
    /// Then:  401 — admin endpoints must reject user JWTs.
    #[tokio::test]
    async fn extractor_rejects_user_token() {
        let state = extractor_state(SECRET).await;
        let user_pair = create_token_pair("user-1", SECRET, 0).expect("user pair");
        let value = format!("Bearer {}", user_pair.access_token);

        let rejection = run_extractor(state, Some(&value))
            .await
            .expect_err("user token must be rejected on admin extractor");
        assert_eq!(rejection.status(), StatusCode::UNAUTHORIZED);
    }

    /// Given: a malformed Authorization header (no Bearer prefix, garbage).
    /// When:  the extractor runs.
    /// Then:  401.
    #[tokio::test]
    async fn extractor_rejects_malformed_header() {
        let state = extractor_state(SECRET).await;
        let rejection = run_extractor(state, Some("garbage"))
            .await
            .expect_err("malformed header must reject");
        assert_eq!(rejection.status(), StatusCode::UNAUTHORIZED);

        // Also reject "Bearer " with no actual token value.
        let state = extractor_state(SECRET).await;
        let rejection = run_extractor(state, Some("Bearer "))
            .await
            .expect_err("empty bearer token must reject");
        assert_eq!(rejection.status(), StatusCode::UNAUTHORIZED);
    }

    /// Given: an admin token signed with a different secret than the server.
    /// When:  the extractor runs.
    /// Then:  401 (signature verification fails inside validate_admin_token).
    #[tokio::test]
    async fn extractor_rejects_wrong_secret_token() {
        let state = extractor_state(SECRET).await;
        let pair = create_admin_token_pair("other-secret").expect("pair");
        let value = format!("Bearer {}", pair.access_token);

        let rejection = run_extractor(state, Some(&value))
            .await
            .expect_err("wrong-secret token must be rejected");
        assert_eq!(rejection.status(), StatusCode::UNAUTHORIZED);
    }

    /// Build a minimal AppState for extractor tests. No users row seeded
    /// (admin auth skips the DB). Lazily sets up an in-memory pool.
    async fn extractor_state(secret: &str) -> Arc<AppState> {
        let pool = sqlx::SqlitePool::connect("sqlite::memory:")
            .await
            .expect("pool");
        Arc::new(AppState {
            pool,
            ws_pool: Arc::new(crate::ws::ConnectionPool::new()),
            config: AppConfig {
                jwt_secret: secret.to_string(),
                ..AppConfig::test_default()
            },
        })
    }
}
