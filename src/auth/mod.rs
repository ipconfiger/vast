pub mod middleware;
pub use middleware::AuthenticatedUser;

use argon2::{
    password_hash::{rand_core::OsRng, PasswordHash, PasswordHasher, PasswordVerifier, SaltString},
    Argon2,
};
use chrono::Utc;
use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Claims {
    pub sub: String,        // user_id
    pub exp: usize,         // expiry
    pub iat: usize,         // issued at
    pub jti: String,        // token ID
    pub is_refresh: bool,   // refresh token flag
}

#[derive(Debug, Serialize, Deserialize)]
pub struct TokenPair {
    pub access_token: String,
    pub refresh_token: String,
    pub expires_in: u32, // seconds
}

const ACCESS_TTL_SECS: u32 = 900;  // 15 minutes
const REFRESH_TTL_SECS: u32 = 604800; // 7 days

/// Create access + refresh token pair for a user
pub fn create_token_pair(user_id: &str, secret: &str) -> Result<TokenPair, jsonwebtoken::errors::Error> {
    let now = Utc::now().timestamp() as usize;

    let access_claims = Claims {
        sub: user_id.to_string(),
        exp: now + ACCESS_TTL_SECS as usize,
        iat: now,
        jti: Uuid::new_v4().to_string(),
        is_refresh: false,
    };

    let refresh_claims = Claims {
        sub: user_id.to_string(),
        exp: now + REFRESH_TTL_SECS as usize,
        iat: now,
        jti: Uuid::new_v4().to_string(),
        is_refresh: true,
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

/// Validate any token (access or refresh) and return claims
pub fn validate_token(token: &str, secret: &str) -> Result<Claims, jsonwebtoken::errors::Error> {
    let token_data = decode::<Claims>(
        token,
        &DecodingKey::from_secret(secret.as_bytes()),
        &Validation::default(),
    )?;
    Ok(token_data.claims)
}

/// Refresh an access token using a valid refresh token
pub fn refresh_access_token(refresh_token: &str, secret: &str) -> Result<TokenPair, jsonwebtoken::errors::Error> {
    let claims = validate_token(refresh_token, secret)?;
    if !claims.is_refresh {
        return Err(jsonwebtoken::errors::Error::from(
            jsonwebtoken::errors::ErrorKind::InvalidToken,
        ));
    }
    create_token_pair(&claims.sub, secret)
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
        let pair = create_token_pair("user-1", secret).unwrap();

        let claims = validate_token(&pair.access_token, secret).unwrap();
        assert_eq!(claims.sub, "user-1");
        assert!(!claims.is_refresh);

        let refresh_claims = validate_token(&pair.refresh_token, secret).unwrap();
        assert!(refresh_claims.is_refresh);
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

    #[test]
    fn test_refresh_token_rejection() {
        let secret = "test-secret";
        let pair = create_token_pair("user-1", secret).unwrap();

        // Trying to use access token as refresh token should fail
        let result = refresh_access_token(&pair.access_token, secret);
        assert!(result.is_err());
    }
}
