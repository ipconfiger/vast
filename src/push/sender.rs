use web_push::WebPushClient;

use crate::push::get_vapid_config;
use crate::ws::ConnectionPool;
use sqlx::SqlitePool;
use std::sync::Arc;
use tracing::warn;

const MAX_PUSHES_PER_MESSAGE: u32 = 50;

// ---------------------------------------------------------------------------
// DB row types (local to this module)
// ---------------------------------------------------------------------------

#[derive(Debug, sqlx::FromRow)]
struct ChannelMember {
    user_id: String,
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct PushSubscriptionRow {
    endpoint: String,
    p256dh: String,
    auth: String,
}

// ---------------------------------------------------------------------------
// Public entry point
// ---------------------------------------------------------------------------

/// Dispatch push notifications to offline channel members after a new message.
///
/// Called from `send_message` handler AFTER `notify_channel` so online users
/// already received the WebSocket event. Queries ALL channel members from DB,
/// skips the sender and online users, then sends web-push to every remaining
/// user. Each push is [`tokio::spawn`]'d — the HTTP response is never blocked.
pub async fn dispatch_push_for_message(
    pool: SqlitePool,
    ws_pool: Arc<ConnectionPool>,
    channel_id: String,
    sender_id: String,
    preview: String,
) {
    let members: Vec<ChannelMember> = match sqlx::query_as(
        "SELECT user_id FROM channel_members WHERE channel_id = ?",
    )
    .bind(&channel_id)
    .fetch_all(&pool)
    .await
    {
        Ok(m) => m,
        Err(e) => {
            warn!(%channel_id, error = %e, "Failed to query channel members for push dispatch");
            return;
        }
    };

    let vapid_config = match get_vapid_config(&pool).await {
        Ok(c) => c,
        Err(e) => {
            warn!(error = %e, "Failed to get VAPID config for push dispatch");
            return;
        }
    };

    let private_key = Arc::new(vapid_config.private_key);
    let subject = Arc::new(vapid_config.subject);

    let mut push_count = 0u32;

    for member in members {
        if push_count >= MAX_PUSHES_PER_MESSAGE {
            break;
        }

        if member.user_id == sender_id {
            continue;
        }

        if ws_pool.is_user_online(&member.user_id) {
            continue;
        }

        let subs: Vec<PushSubscriptionRow> = match sqlx::query_as(
            "SELECT endpoint, p256dh, auth FROM push_subscriptions WHERE user_id = ?",
        )
        .bind(&member.user_id)
        .fetch_all(&pool)
        .await
        {
            Ok(s) => s,
            Err(e) => {
                warn!(user_id = %member.user_id, error = %e, "Failed to query push subscriptions");
                continue;
            }
        };

        for sub in subs {
            if push_count >= MAX_PUSHES_PER_MESSAGE {
                break;
            }
            push_count += 1;

            let pool = pool.clone();
            let channel_id = channel_id.clone();
            let preview = preview.clone();
            let private_key = Arc::clone(&private_key);
            let subject = Arc::clone(&subject);

            tokio::spawn(async move {
                send_push_to_subscription(
                    &pool,
                    &sub,
                    &channel_id,
                    &preview,
                    &private_key,
                    &subject,
                )
                .await;
            });
        }
    }
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

async fn send_push_to_subscription(
    pool: &SqlitePool,
    sub: &PushSubscriptionRow,
    channel_id: &str,
    preview: &str,
    private_key: &str,
    _subject: &str,
) {
    let payload = serde_json::json!({
        "channel_id": channel_id,
        "sender_name": "",
        "preview": preview,
        "url": format!("/channels/{channel_id}"),
    })
    .to_string()
    .into_bytes();

    send_with_retry(pool, &sub.endpoint, || {
        let sub_info = web_push::SubscriptionInfo::new(
            &sub.endpoint,
            &sub.p256dh,
            &sub.auth,
        );

        let sig = web_push::VapidSignatureBuilder::from_pem(
            private_key.as_bytes(),
            &sub_info,
        )
        .ok()?
        .build()
        .ok()?;

        let mut mb = web_push::WebPushMessageBuilder::new(&sub_info);
        mb.set_payload(web_push::ContentEncoding::Aes128Gcm, &payload);
        mb.set_vapid_signature(sig);
        mb.build().ok()
    })
    .await;
}

async fn send_with_retry<F>(
    pool: &SqlitePool,
    endpoint: &str,
    build_message: F,
)
where
    F: Fn() -> Option<web_push::WebPushMessage>,
{
    let client = match web_push::IsahcWebPushClient::new() {
        Ok(c) => c,
        Err(e) => {
            warn!(%endpoint, error = %e, "Failed to create IsahcWebPushClient");
            return;
        }
    };

    let message = match build_message() {
        Some(m) => m,
        None => {
            warn!(%endpoint, "Failed to build push message");
            return;
        }
    };

    match client.send(message).await {
        Ok(_) => (),
        Err(e) => {
            let err_str = e.to_string();

            if err_str.contains("410") {
                warn!(%endpoint, "Push subscription 410 Gone, deleting");
                let _ = sqlx::query("DELETE FROM push_subscriptions WHERE endpoint = ?")
                    .bind(endpoint)
                    .execute(pool)
                    .await;
                return;
            }

            if err_str.contains("429") {
                warn!(%endpoint, "Push rate limited (429), retrying after 1 s");
                tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                let retry_message = match build_message() {
                    Some(m) => m,
                    None => {
                        warn!(%endpoint, "Failed to rebuild push message for retry");
                        return;
                    }
                };
                match client.send(retry_message).await {
        Ok(_) => (),
                    Err(e2) => {
                        warn!(%endpoint, error = %e2, "Push retry after 429 failed");
                    }
                }
                return;
            }

            warn!(%endpoint, error = %e, "Push notification failed");
        }
    }
}
