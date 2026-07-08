pub mod sender;

use axum::{extract::State, Json};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use openssl::ec::{EcGroup, EcKey};
use openssl::nid::Nid;
use sqlx::SqlitePool;
use std::sync::Arc;

use crate::error::AppError;
use crate::AppState;

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

pub struct VapidConfig {
    pub private_key: String,
    pub public_key: String,
    pub subject: String,
}

#[derive(Debug, sqlx::FromRow)]
struct SettingRow {
    #[allow(dead_code)]
    key: String,
    value: String,
}

// ---------------------------------------------------------------------------
// Key generation
// ---------------------------------------------------------------------------

fn generate_vapid_key_pair() -> Result<(String, String), AppError> {
    let group = EcGroup::from_curve_name(Nid::X9_62_PRIME256V1)
        .map_err(|e| AppError::Internal(format!("Failed to create EC group: {e}")))?;

    let ec_key = EcKey::generate(&group)
        .map_err(|e| AppError::Internal(format!("Failed to generate EC key: {e}")))?;

    // Private key as PKCS#8 PEM
    let private_pem = ec_key
        .private_key_to_pem()
        .map_err(|e| AppError::Internal(format!("Failed to encode private key: {e}")))?;

    let private_key = String::from_utf8(private_pem)
        .map_err(|e| AppError::Internal(format!("Invalid UTF-8 in private key: {e}")))?;

    // Public key as uncompressed point (65 bytes: 0x04 || x || y) → base64url
    let mut ctx = openssl::bn::BigNumContext::new()
        .map_err(|e| AppError::Internal(format!("Failed to create BN context: {e}")))?;

    let public_bytes = ec_key
        .public_key()
        .to_bytes(
            &group,
            openssl::ec::PointConversionForm::UNCOMPRESSED,
            &mut ctx,
        )
        .map_err(|e| AppError::Internal(format!("Failed to encode public key: {e}")))?;

    let public_key = URL_SAFE_NO_PAD.encode(&public_bytes);

    Ok((private_key, public_key))
}

// ---------------------------------------------------------------------------
// Config loading / initialization
// ---------------------------------------------------------------------------

/// Load VAPID configuration from environment variables, then the settings table.
/// If no keys exist, generates a fresh ES256 key pair and stores it.
pub async fn init_vapid_keys(pool: &SqlitePool) -> Result<VapidConfig, AppError> {
    // Try loading existing config
    let mut config = get_vapid_config(pool).await?;

    // If keys are missing, generate new ones
    if config.private_key.is_empty() || config.public_key.is_empty() {
        let (private_key, public_key) = generate_vapid_key_pair()?;
        let subject = std::env::var("VAPID_SUBJECT")
            .unwrap_or_else(|_| "mailto:push@vast.local".to_string());

        // Store generated keys to settings table
        sqlx::query("INSERT OR REPLACE INTO settings (key, value) VALUES (?, ?)")
            .bind("vapid_private_key")
            .bind(&private_key)
            .execute(pool)
            .await?;

        sqlx::query("INSERT OR REPLACE INTO settings (key, value) VALUES (?, ?)")
            .bind("vapid_public_key")
            .bind(&public_key)
            .execute(pool)
            .await?;

        sqlx::query("INSERT OR REPLACE INTO settings (key, value) VALUES (?, ?)")
            .bind("vapid_subject")
            .bind(&subject)
            .execute(pool)
            .await?;

        config = VapidConfig {
            private_key,
            public_key,
            subject,
        };
    }

    Ok(config)
}

/// Read VAPID configuration. Checks env vars first, then falls back to the
/// settings table. Returns empty strings for missing keys (caller should
/// generate).
pub async fn get_vapid_config(pool: &SqlitePool) -> Result<VapidConfig, AppError> {
    // Env vars take precedence
    let private_key = std::env::var("VAPID_PRIVATE_KEY").unwrap_or_default();
    let public_key = std::env::var("VAPID_PUBLIC_KEY").unwrap_or_default();
    let subject = std::env::var("VAPID_SUBJECT").unwrap_or_default();

    if !private_key.is_empty() || !public_key.is_empty() {
        return Ok(VapidConfig {
            private_key,
            public_key,
            subject: if subject.is_empty() {
                "mailto:push@vast.local".to_string()
            } else {
                subject
            },
        });
    }

    // Fall back to settings table
    let private_key = sqlx::query_as::<_, SettingRow>(
        "SELECT key, value FROM settings WHERE key = 'vapid_private_key'",
    )
    .fetch_optional(pool)
    .await?
    .map(|r| r.value)
    .unwrap_or_default();

    let public_key = sqlx::query_as::<_, SettingRow>(
        "SELECT key, value FROM settings WHERE key = 'vapid_public_key'",
    )
    .fetch_optional(pool)
    .await?
    .map(|r| r.value)
    .unwrap_or_default();

    let subject = sqlx::query_as::<_, SettingRow>(
        "SELECT key, value FROM settings WHERE key = 'vapid_subject'",
    )
    .fetch_optional(pool)
    .await?
    .map(|r| r.value)
    .unwrap_or_else(|| {
        std::env::var("VAPID_SUBJECT").unwrap_or_else(|_| "mailto:push@vast.local".to_string())
    });

    Ok(VapidConfig {
        private_key,
        public_key,
        subject,
    })
}

// ---------------------------------------------------------------------------
// Handler
// ---------------------------------------------------------------------------

/// Public endpoint — no auth required (needed before login for SW registration).
pub async fn vapid_public_key_handler(
    State(state): State<Arc<AppState>>,
) -> Result<Json<serde_json::Value>, AppError> {
    let config = get_vapid_config(&state.pool).await?;

    // If no key exists yet, generate one on first access
    if config.public_key.is_empty() {
        let (_private_key, public_key) = generate_vapid_key_pair()?;
        let subject = std::env::var("VAPID_SUBJECT")
            .unwrap_or_else(|_| "mailto:push@vast.local".to_string());

        sqlx::query("INSERT OR REPLACE INTO settings (key, value) VALUES (?, ?)")
            .bind("vapid_private_key")
            .bind(&_private_key)
            .execute(&state.pool)
            .await?;

        sqlx::query("INSERT OR REPLACE INTO settings (key, value) VALUES (?, ?)")
            .bind("vapid_public_key")
            .bind(&public_key)
            .execute(&state.pool)
            .await?;

        sqlx::query("INSERT OR REPLACE INTO settings (key, value) VALUES (?, ?)")
            .bind("vapid_subject")
            .bind(&subject)
            .execute(&state.pool)
            .await?;

        return Ok(Json(serde_json::json!({"public_key": public_key})));
    }

    Ok(Json(serde_json::json!({"public_key": config.public_key})))
}
