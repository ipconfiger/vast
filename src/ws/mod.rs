pub mod protocol;

use axum::extract::{
    ws::{Message, WebSocket, WebSocketUpgrade},
    Query, State,
};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use dashmap::{DashMap, DashSet};
use serde_json::json;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::broadcast;
use tracing::{debug, info, instrument, warn};
use uuid::Uuid;

use crate::auth;
use crate::AppState;

use protocol::{ClientEvent, ServerEvent};

// ---------------------------------------------------------------------------
// Type aliases
// ---------------------------------------------------------------------------

pub type ConnectionId = String;
pub type UserId = String;
pub type ChannelId = String;

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// How often the server sends a Ping to the client.
const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(15);

/// Maximum time without any message from the client before we disconnect.
const HEARTBEAT_TIMEOUT: Duration = Duration::from_secs(60);

// ---------------------------------------------------------------------------
// Connection metadata
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct ConnectionMeta {
    pub connection_id: ConnectionId,
    pub user_id: UserId,
    pub subscribed_channels: Vec<ChannelId>,
    pub joined_at: Instant,
}

// ---------------------------------------------------------------------------
// Connection pool
// ---------------------------------------------------------------------------

/// WebSocket connection pool that tracks user connections,
/// a global broadcast channel, and connection metadata.
///
/// All methods take `&self` — interior mutability via DashMap.
pub struct ConnectionPool {
    user_connections: DashMap<UserId, DashSet<ConnectionId>>,
    connections: DashMap<ConnectionId, ConnectionMeta>,
    pub(crate) global_tx: broadcast::Sender<ServerEvent>,
    typing_timeouts: DashMap<String, Instant>,
    /// In-memory cache of each connected user's channel memberships, used
    /// for receiver-side event filtering (C3 cross-channel isolation).
    /// Loaded from the DB on connect via `init_user_channels` and kept in
    /// sync incrementally as memberships change.
    user_channels: DashMap<UserId, DashSet<ChannelId>>,
}

impl Default for ConnectionPool {
    fn default() -> Self {
        Self::new()
    }
}

impl ConnectionPool {
    pub fn new() -> Self {
        let (global_tx, _) = broadcast::channel(256);
        Self {
            user_connections: DashMap::new(),
            connections: DashMap::new(),
            global_tx,
            typing_timeouts: DashMap::new(),
            user_channels: DashMap::new(),
        }
    }

    /// Register a new WebSocket connection for a user.
    ///
    /// Subscribes the connection to the global broadcast channel and
    /// returns a receiver that the per-connection send loop polls.
    #[instrument(skip(self))]
    pub fn register(
        &self,
        user_id: &str,
        connection_id: &str,
    ) -> broadcast::Receiver<ServerEvent> {
        self.user_connections
            .entry(user_id.to_string())
            .or_default()
            .insert(connection_id.to_string());

        self.connections.insert(
            connection_id.to_string(),
            ConnectionMeta {
                connection_id: connection_id.to_string(),
                user_id: user_id.to_string(),
                subscribed_channels: Vec::new(),
                joined_at: Instant::now(),
            },
        );

        self.global_tx.subscribe()
    }

    /// Remove a connection and broadcast presence-offline.
    #[instrument(skip(self))]
    pub fn unregister(&self, user_id: &str, connection_id: &str) {
        self.connections.remove(connection_id);

        if let Some(entry) = self.user_connections.get_mut(user_id) {
            entry.remove(connection_id);
            if entry.is_empty() {
                drop(entry);
                self.user_connections.remove(user_id);
            }
        }

        // Broadcast presence-offline globally.
        let presence = ServerEvent::Presence {
            user_id: user_id.to_string(),
            status: "offline".to_string(),
        };
        let _ = self.global_tx.send(presence);

        debug!(user_id, connection_id, "Unregistered WebSocket connection");

        // Drop the in-memory channel-membership cache once the user has no
        // remaining connections (it is reloaded fresh on the next connect).
        if !self.is_user_online(user_id) {
            self.user_channels.remove(user_id);
        }
    }

    /// Fan-out a ServerEvent via the global broadcast channel.
    ///
    /// Returns 0 — the return value is retained for compatibility with
    /// existing callers (mainly tests) but is no longer meaningful since
    /// all events go through the single global channel.
    #[instrument(skip(self, event))]
    pub fn broadcast_to_channel(&self, _channel_id: &str, event: &ServerEvent) -> usize {
        self.global_tx.send(event.clone()).ok();
        0
    }

    /// Return the set of user IDs currently connected to a channel.
    pub fn get_channel_members(&self, channel_id: &str) -> Vec<UserId> {
        let mut members: Vec<UserId> = Vec::new();
        for entry in self.connections.iter() {
            if entry.subscribed_channels.contains(&channel_id.to_string())
                && !members.contains(&entry.user_id)
            {
                members.push(entry.user_id.clone());
            }
        }
        members
    }

    /// Convenience wrapper — push a `ServerEvent` to every connection
    /// subscribed to `channel_id`.  Called by the message handler after a
    /// DB insert to fan-out `NewMsg` / `MsgDeleted` / etc.
    pub fn notify_channel(&self, channel_id: &str, event: ServerEvent) -> usize {
        self.broadcast_to_channel(channel_id, &event)
    }

    pub fn subscribe_channel(&self, connection_id: &str, channel_id: &str) {
        if let Some(mut meta) = self.connections.get_mut(connection_id)
            && !meta.subscribed_channels.iter().any(|c| c == channel_id)
        {
            meta.subscribed_channels.push(channel_id.to_string());
        }
    }

    pub fn unsubscribe_channel(&self, connection_id: &str, channel_id: &str) {
        if let Some(mut meta) = self.connections.get_mut(connection_id) {
            meta.subscribed_channels.retain(|c| c != channel_id);
        }
    }

    // ── Typing indicators ──────────────────────────────────────────

    /// Record that `user_id` is typing in `channel_id`.
    /// Also lazily cleans up stale typing entries.
    pub fn record_typing(&self, channel_id: &str, user_id: &str) {
        self.typing_timeouts
            .insert(format!("{channel_id}:{user_id}"), Instant::now());
        self.cleanup_stale_typing();
    }

    /// Remove the typing state for `user_id` in `channel_id`.
    pub fn remove_typing(&self, channel_id: &str, user_id: &str) {
        self.typing_timeouts.remove(&format!("{channel_id}:{user_id}"));
    }

    /// Remove any typing entries older than 5 seconds.
    pub fn cleanup_stale_typing(&self) {
        let cutoff = Instant::now() - Duration::from_secs(5);
        self.typing_timeouts.retain(|_, last_seen| *last_seen > cutoff);
    }

    pub fn is_user_online(&self, user_id: &str) -> bool {
        self.user_connections
            .get(user_id)
            .is_some_and(|entry| !entry.is_empty())
    }

    // ── Channel-membership cache (C3 isolation) ─────────────────────

    /// Load (merge) the given user's channel memberships from the DB into
    /// the in-memory cache. Called once when a user connects. Only inserts
    /// new entries — never overwrites the existing set, so concurrent
    /// incremental updates are preserved.
    pub async fn init_user_channels(&self, pool: &sqlx::SqlitePool, user_id: &str) {
        let channel_ids: Vec<String> = sqlx::query_scalar(
            "SELECT channel_id FROM channel_members WHERE user_id = ?",
        )
        .bind(user_id)
        .fetch_all(pool)
        .await
        .unwrap_or_else(|e| {
            warn!(%e, user_id, "Failed to load user channels from DB");
            Vec::new()
        });

        let set = self.user_channels.entry(user_id.to_string()).or_default();
        for ch in channel_ids {
            set.insert(ch);
        }
    }

    /// Return true if `user_id` is cached as a member of `channel_id`.
    /// Two hash lookups; no DB access.
    pub fn is_channel_member(&self, user_id: &str, channel_id: &str) -> bool {
        self.user_channels
            .get(user_id)
            .is_some_and(|set| set.contains(channel_id))
    }

    /// Record that `user_id` is now a member of `channel_id`.
    pub fn add_user_channel(&self, user_id: &str, channel_id: &str) {
        self.user_channels
            .entry(user_id.to_string())
            .or_default()
            .insert(channel_id.to_string());
    }

    /// Record that `user_id` is no longer a member of `channel_id`.
    pub fn remove_user_channel(&self, user_id: &str, channel_id: &str) {
        if let Some(set) = self.user_channels.get(user_id) {
            set.remove(channel_id);
        }
    }

    /// Register a bot connection (same internal bookkeeping as user connections).
    pub fn register_bot(&self, bot_user_id: &str, connection_id: &str) -> broadcast::Receiver<ServerEvent> {
        self.register(bot_user_id, connection_id)
    }

    /// Unregister a bot connection.
    pub fn unregister_bot(&self, bot_user_id: &str, connection_id: &str) {
        self.unregister(bot_user_id, connection_id);
    }
}

// ---------------------------------------------------------------------------
// WS upgrade handler
// ---------------------------------------------------------------------------

/// Axum handler for GET /ws?token=<jwt>
///
/// Validates the JWT *before* upgrading to WebSocket.  Returns 401
/// (JSON) when the token is missing or invalid so the client can
/// distinguish an auth failure from a protocol error.
#[instrument(skip(ws, state))]
pub async fn ws_handler(
    ws: WebSocketUpgrade,
    Query(params): Query<HashMap<String, String>>,
    State(state): State<Arc<AppState>>,
) -> impl axum::response::IntoResponse {
    let token = params.get("token").cloned().unwrap_or_default();

    let claims = match auth::validate_token(&token, &state.config.jwt_secret) {
        Ok(claims) => claims,
        Err(e) => {
            warn!(%e, "WebSocket connection rejected: invalid token");
            return (
                StatusCode::UNAUTHORIZED,
                axum::Json(json!({
                    "error": {
                        "code": "UNAUTHORIZED",
                        "message": "Invalid or missing authentication token"
                    }
                })),
            )
                .into_response();
        }
    };

    // Enforce token_epoch (forced logout) and reject tokens whose sub is
    // not a users row (deleted user, admin principal).
    if let Err(e) = auth::verify_user_epoch(&state.pool, &claims.sub, claims.epoch).await {
        warn!(%e, "WebSocket connection rejected: epoch check failed");
        return (
            StatusCode::UNAUTHORIZED,
            axum::Json(json!({
                "error": {
                    "code": "UNAUTHORIZED",
                    "message": "Token has been superseded"
                }
            })),
        )
            .into_response();
    }

    let user_id = claims.sub;
    info!(user_id, "WebSocket upgrade requested");

    let ws_pool = state.ws_pool.clone();
    let db_pool = state.pool.clone();
    ws.on_upgrade(move |socket| handle_socket(socket, user_id, ws_pool, db_pool))
}

// ---------------------------------------------------------------------------
// Bot WS handler
// ---------------------------------------------------------------------------

/// Axum handler for GET /ws/bot?key=<connection_key>
///
/// Authenticates a bot connector by its `connection_key`, then upgrades to
/// WebSocket. The bot's user_id is resolved from the `bots` table and the
/// connection is registered in the connection pool with `is_bot = true`.
#[instrument(skip(ws, state))]
pub async fn ws_bot_handler(
    ws: WebSocketUpgrade,
    Query(params): Query<HashMap<String, String>>,
    State(state): State<Arc<AppState>>,
) -> impl axum::response::IntoResponse {
    let key = params.get("key").cloned().unwrap_or_default();
    if key.is_empty() {
        return (
            StatusCode::UNAUTHORIZED,
            axum::Json(json!({
                "error": { "code": "UNAUTHORIZED", "message": "Missing connection key" }
            })),
        )
            .into_response();
    }

    // Look up the bot by connection_key and resolve its user_id.
    let bot: (String, String, String) = match sqlx::query_as::<_, (String, String, String)>(
        "SELECT b.user_id, b.id, b.name FROM bots b
         WHERE b.connection_key = ? AND b.connection_mode = 'connector' AND b.is_active = 1",
    )
    .bind(&key)
    .fetch_optional(&state.pool)
    .await
    {
        Ok(Some(row)) => row,
        Ok(None) => {
            warn!("WebSocket bot connection rejected: invalid key");
            return (
                StatusCode::UNAUTHORIZED,
                axum::Json(json!({
                    "error": { "code": "UNAUTHORIZED", "message": "Invalid connection key" }
                })),
            ).into_response();
        }
        Err(e) => {
            warn!(%e, "WebSocket bot connection: DB error");
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                axum::Json(json!({
                    "error": { "code": "INTERNAL", "message": "Database error" }
                })),
            ).into_response();
        }
    };

    let (bot_user_id, bot_id, bot_name) = bot;
    info!(bot_user_id, bot_id, bot_name, "Bot WebSocket upgrade requested");

    let db_pool = state.pool.clone();
    ws.on_upgrade(move |socket| handle_bot_socket(socket, bot_user_id, bot_id, state.ws_pool.clone(), db_pool))
}

/// Per-bot-connection loop. Identical in structure to `handle_socket` but
/// registers as a bot and subscribes to all channels the bot is a member of.
async fn handle_bot_socket(
    mut socket: WebSocket,
    bot_user_id: UserId,
    bot_id: String,
    pool: Arc<ConnectionPool>,
    db_pool: sqlx::SqlitePool,
) {
    let connection_id = Uuid::new_v4().to_string();
    let _ = pool.register_bot(&bot_user_id, &connection_id);
    info!(bot_user_id, %connection_id, "Bot WebSocket connected");

    // Subscribe to channels the bot is a member of.
    let presence = ServerEvent::Presence {
        user_id: bot_user_id.clone(),
        status: "online".to_string(),
    };
    let _ = pool.global_tx.send(presence);

    let mut broadcast_rx = pool.global_tx.subscribe();
    let mut last_heartbeat = Instant::now();
    let mut pending_mention: Option<String> = None;

    loop {
        tokio::select! {
            ws_msg = socket.recv() => {
                match ws_msg {
                    Some(Ok(Message::Text(text))) => {
                        last_heartbeat = Instant::now();
                        match serde_json::from_str::<ClientEvent>(text.as_str()) {
                            Ok(ClientEvent::BotReply { mention_id, channel_id, text }) => {
                                info!(%mention_id, %channel_id, "Bot replied");
                                // Insert the bot reply as a message and broadcast.
                                let msg_id = Uuid::new_v4().to_string();
                                let payload = serde_json::json!({"text": text}).to_string();
                                if let Ok(inserted_id) = sqlx::query_scalar::<_, i64>(
                                    "INSERT INTO messages (msg_id, channel_id, sender_id, msg_type, payload)
                                     VALUES (?, ?, ?, 'text', ?) RETURNING id",
                                )
                                .bind(&msg_id)
                                .bind(&channel_id)
                                .bind(&bot_user_id)
                                .bind(&payload)
                                .fetch_one(&db_pool)
                                .await
                                {
                                    pool.notify_channel(
                                        &channel_id,
                                        ServerEvent::NewMsg {
                                            channel_id: channel_id.clone(),
                                            cursor: inserted_id,
                                            sender_id: bot_user_id.clone(),
                                            msg_type: "text".to_string(),
                                            preview: text.chars().take(100).collect(),
                                            quoted_message_id: None,
                                        },
                                    );
                                }
                            }
                            Ok(ClientEvent::Ping) => {
                                let pong = serde_json::to_string(&ServerEvent::Pong).unwrap();
                                let _ = socket.send(Message::Text(pong.into())).await;
                            }
                            _ => {
                                debug!("Ignored unknown bot client event");
                            }
                        }
                    }
                    Some(Ok(Message::Close(_))) => break,
                    Some(Ok(Message::Pong(_))) => {
                        last_heartbeat = Instant::now();
                    }
                    Some(Ok(_)) => {
                        last_heartbeat = Instant::now();
                    }
                    Some(Err(e)) => {
                        warn!(%e, "Bot WebSocket recv error");
                        break;
                    }
                    None => break,
                }
            }

            // ── Outbound broadcast ───────────────────────────────
            broadcast_msg = broadcast_rx.recv() => {
                match broadcast_msg {
                    Ok(event) => {
                        // Only deliver a BotMention to the bot it targets;
                        // other connected bots must drop it to avoid a reply
                        // storm where every bot replies to every mention.
                        if !should_deliver_to_bot(&event, &bot_user_id) {
                            continue;
                        }
                        // Forward BotMention events to this specific bot.
                        if let ServerEvent::BotMention { mention_id, .. } = &event {
                            pending_mention = Some(mention_id.clone());
                        }
                        let json = serde_json::to_string(&event).unwrap_or_default();
                        if socket.send(Message::Text(json.into())).await.is_err() {
                            break;
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(n)) => {
                        warn!(n, "Bot broadcast channel lagged");
                    }
                    Err(broadcast::error::RecvError::Closed) => break,
                }
            }

            // ── Heartbeat ────────────────────────────────────────
            _ = tokio::time::sleep(HEARTBEAT_INTERVAL) => {
                if last_heartbeat.elapsed() > HEARTBEAT_TIMEOUT {
                    warn!(bot_user_id, "Bot heartbeat timeout");
                    break;
                }
                if socket.send(Message::Ping(Vec::new().into())).await.is_err() {
                    break;
                }
            }
        }
    }

    // Cleanup
    let presence = ServerEvent::Presence {
        user_id: bot_user_id.clone(),
        status: "offline".to_string(),
    };
    let _ = pool.global_tx.send(presence);
    pool.unregister_bot(&bot_user_id, &connection_id);
    info!(bot_user_id, "Bot WebSocket disconnected");
}

// ---------------------------------------------------------------------------
// Per-socket handler
// ---------------------------------------------------------------------------

/// Multiplexed loop that reads from three sources:
///
/// 1. WebSocket receive — parse incoming ClientEvent messages.
/// 2. Global broadcast channel — push ServerEvent fan-out to the client.
/// 3. Heartbeat timer — send periodic Ping frames (15 s interval,
///    60 s idle timeout).
#[instrument(skip(socket, pool, db_pool))]
async fn handle_socket(
    mut socket: WebSocket,
    user_id: UserId,
    pool: Arc<ConnectionPool>,
    db_pool: sqlx::SqlitePool,
) {
    let connection_id = Uuid::new_v4().to_string();
    info!(user_id, connection_id, "WebSocket connection opened");

    let mut broadcast_rx = pool.register(&user_id, &connection_id);
    // Prime the channel-membership cache from the DB before processing any
    // broadcast events so receiver-side filtering (C3) is correct from the
    // very first event.
    pool.init_user_channels(&db_pool, &user_id).await;
    let mut last_heartbeat = Instant::now();

    loop {
        tokio::select! {
            // ── Incoming WebSocket message ──────────────────────────
            ws_msg = socket.recv() => {
                match ws_msg {
                    Some(Ok(Message::Text(text))) => {
                        last_heartbeat = Instant::now();
                        match serde_json::from_str::<ClientEvent>(text.as_str()) {
                            Ok(ClientEvent::Ping) => {
                                debug!("Ping received, sending Pong");
                                let pong = serde_json::to_string(&ServerEvent::Pong)
                                    .expect("Pong is always serializable");
                                let _ = socket.send(Message::Text(pong.into())).await;
                            }
                            Ok(ClientEvent::TypingStart { channel_id }) => {
                                pool.record_typing(&channel_id, &user_id);
                                pool.broadcast_to_channel(
                                    &channel_id,
                                    &ServerEvent::Typing {
                                        channel_id: channel_id.clone(),
                                        user_id: user_id.clone(),
                                        thread_parent_cursor: None,
                                    },
                                );
                            }
                            Ok(ClientEvent::TypingStop { channel_id }) => {
                                pool.remove_typing(&channel_id, &user_id);
                            }
                            Ok(ClientEvent::Subscribe { channel_id }) => {
                                info!(user_id, %channel_id, %connection_id, "subscribe");
                                pool.subscribe_channel(&connection_id, &channel_id);
                            }
                            Ok(ClientEvent::Unsubscribe { channel_id }) => {
                                info!(user_id, %channel_id, %connection_id, "unsubscribe");
                                pool.unsubscribe_channel(&connection_id, &channel_id);
                            }
                            Ok(ClientEvent::BotReply { .. }) => {
                                // BotReply is handled by the bot WS loop; ignore in user sockets.
                            }
                            Err(e) => {
                                warn!(%e, "Failed to parse ClientEvent from client");
                            }
                        }
                    }
                    Some(Ok(Message::Close(_))) => {
                        info!("WebSocket close frame received");
                        break;
                    }
                    Some(Ok(Message::Pong(_))) => {
                        last_heartbeat = Instant::now();
                    }
                    Some(Ok(_)) => {
                        // Binary, Ping from client — reset keepalive timer.
                        last_heartbeat = Instant::now();
                    }
                    Some(Err(e)) => {
                        warn!(%e, "WebSocket recv error");
                        break;
                    }
                    None => {
                        debug!("WebSocket stream ended");
                        break;
                    }
                }
            }

            // ── Incoming broadcast event ────────────────────────────
            event = broadcast_rx.recv() => {
                match event {
                    Ok(event) => {
                        // BotMention carries a bot's system_prompt, model, and
                        // the channel message history. It is intended solely
                        // for bot WebSocket connections and must never be
                        // forwarded to user clients (privacy leak).
                        if matches!(event, ServerEvent::BotMention { .. }) {
                            continue;
                        }
                        // C3: receiver-side channel isolation. Drop any
                        // channel-scoped event the user is not a member of,
                        // and DM events they are not a participant of.
                        if !should_deliver(&event, &user_id, &pool) {
                            continue;
                        }
                        // Exclude sender from receiving their own
                        // typing indicator.
                        let is_self_typing = match &event {
                            ServerEvent::Typing {
                                user_id: uid, ..
                            } => *uid == user_id,
                            _ => false,
                        };
                        if is_self_typing {
                            continue;
                        }
                        let text = match serde_json::to_string(&event) {
                            Ok(t) => t,
                            Err(e) => {
                                warn!(%e, "Failed to serialize ServerEvent");
                                continue;
                            }
                        };
                        if socket.send(Message::Text(text.into())).await.is_err() {
                            warn!("Failed to forward broadcast event to WebSocket");
                            break;
                        }
                    }
                    Err(broadcast::error::RecvError::Closed) => {
                        debug!("Broadcast channel closed");
                        break;
                    }
                    Err(broadcast::error::RecvError::Lagged(n)) => {
                        warn!(%n, "Broadcast receiver lagged — dropped {n} messages");
                    }
                }
            }

            // ── Heartbeat ───────────────────────────────────────────
            _ = tokio::time::sleep(HEARTBEAT_INTERVAL) => {
                // Clean up stale typing entries every heartbeat cycle.
                pool.cleanup_stale_typing();

                if last_heartbeat.elapsed() > HEARTBEAT_TIMEOUT {
                    warn!(user_id, connection_id, "WebSocket heartbeat timeout");
                    let _ = socket.send(Message::Close(None)).await;
                    break;
                }
                if socket.send(Message::Ping(Vec::new().into())).await.is_err() {
                    break;
                }
            }
        }
    }

    pool.unregister(&user_id, &connection_id);
    info!(user_id, connection_id, "WebSocket connection closed");
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

/// Decide whether a broadcast `event` should be delivered to the bot
/// connection identified by `bot_user_id`. A `BotMention` targets a single
/// bot and must not fan out to other connected bots (prevents reply storms);
/// all other event types are delivered to every bot connection.
fn should_deliver_to_bot(event: &ServerEvent, bot_user_id: &str) -> bool {
    match event {
        ServerEvent::BotMention { bot_user_id: target, .. } => target == bot_user_id,
        _ => true,
    }
}

/// Decide whether a broadcast `event` should be delivered to the user
/// connection identified by `user_id` (C3 cross-channel isolation).
///
/// * Channel-scoped content events (`channel_id()` is `Some`) are dropped
///   unless the user is a cached member of that channel.
/// * DM events are delivered only to their participants.
/// * Everything else (presence, management, errors) is delivered.
fn should_deliver(event: &ServerEvent, user_id: &str, pool: &ConnectionPool) -> bool {
    if let Some(ch) = event.channel_id()
        && !pool.is_channel_member(user_id, ch)
    {
        return false;
    }
    match event {
        ServerEvent::DmCreated { participant_ids, .. }
        | ServerEvent::DmClosed { participant_ids, .. } => {
            return participant_ids.contains(&user_id.to_string());
        }
        _ => {}
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_pool() -> ConnectionPool {
        ConnectionPool::new()
    }

    // ── Pool initialisation ──────────────────────────────────────────

    #[test]
    fn new_pool_is_empty() {
        let pool = make_pool();
        assert!(pool.connections.is_empty());
        assert!(pool.user_connections.is_empty());
        assert!(pool.typing_timeouts.is_empty());
        // Global channel exists but has no subscribers yet.
        assert_eq!(pool.global_tx.receiver_count(), 0);
    }

    // ── Bot mention targeting (C2: no reply storm) ──────────────────

    #[test]
    fn bot_mention_delivered_only_to_target_bot() {
        let mention = ServerEvent::BotMention {
            bot_user_id: "bot-a".to_string(),
            mention_id: "m1".to_string(),
            channel_id: "c1".to_string(),
            messages: vec![],
            model: "gpt-4o-mini".to_string(),
            system_prompt: "p".to_string(),
        };
        // Target bot receives it.
        assert!(should_deliver_to_bot(&mention, "bot-a"));
        // Any other connected bot must not.
        assert!(!should_deliver_to_bot(&mention, "bot-b"));
    }

    #[test]
    fn non_mention_events_delivered_to_all_bots() {
        // Pong (and other non-targeted events) go to every bot connection.
        assert!(should_deliver_to_bot(&ServerEvent::Pong, "bot-a"));
        assert!(should_deliver_to_bot(&ServerEvent::Pong, "bot-b"));
    }

    // ── Channel-membership cache (C3: cross-channel isolation) ───────

    #[test]
    fn is_channel_member_reflects_add_and_remove() {
        let pool = make_pool();
        assert!(!pool.is_channel_member("user-a", "ch-1"));
        pool.add_user_channel("user-a", "ch-1");
        assert!(pool.is_channel_member("user-a", "ch-1"));
        // A different channel for the same user is unaffected.
        assert!(!pool.is_channel_member("user-a", "ch-2"));
        pool.remove_user_channel("user-a", "ch-1");
        assert!(!pool.is_channel_member("user-a", "ch-1"));
    }

    #[test]
    fn should_deliver_filters_non_member_content_events() {
        let pool = make_pool();
        let event = ServerEvent::NewMsg {
            channel_id: "ch-1".into(),
            cursor: 1,
            sender_id: "sender".into(),
            msg_type: "text".into(),
            preview: "hi".into(),
            quoted_message_id: None,
        };
        // Non-member is filtered out.
        assert!(!should_deliver(&event, "user-a", &pool));
        // Member passes.
        pool.add_user_channel("user-a", "ch-1");
        assert!(should_deliver(&event, "user-a", &pool));
    }

    #[test]
    fn should_deliver_passes_non_channel_events() {
        let pool = make_pool();
        assert!(should_deliver(&ServerEvent::Pong, "user-a", &pool));
        assert!(should_deliver(
            &ServerEvent::Presence {
                user_id: "u".into(),
                status: "online".into(),
            },
            "user-a",
            &pool
        ));
        assert!(should_deliver(
            &ServerEvent::MemberAdded {
                channel_id: "ch-1".into(),
                user_id: "u".into(),
                username: "n".into(),
            },
            "user-a",
            &pool
        ));
    }

    #[test]
    fn should_deliver_filters_dm_non_participant() {
        let pool = make_pool();
        let participants = vec!["u1".to_string(), "u2".to_string()];

        let created = ServerEvent::DmCreated {
            dm_channel_id: "d1".into(),
            participant_ids: participants.clone(),
        };
        assert!(!should_deliver(&created, "u3", &pool));
        assert!(should_deliver(&created, "u1", &pool));

        let closed = ServerEvent::DmClosed {
            dm_channel_id: "d1".into(),
            participant_ids: participants,
        };
        assert!(!should_deliver(&closed, "u3", &pool));
        assert!(should_deliver(&closed, "u2", &pool));
    }

    #[test]
    fn unregister_clears_user_channels_on_last_connection() {
        let pool = make_pool();
        let _rx = pool.register("user-a", "conn-1");
        pool.add_user_channel("user-a", "ch-1");
        assert!(pool.is_channel_member("user-a", "ch-1"));

        pool.unregister("user-a", "conn-1");
        // Cache cleared once the user has no remaining connections.
        assert!(!pool.is_channel_member("user-a", "ch-1"));
    }

    #[test]
    fn unregister_keeps_channels_with_other_connections() {
        let pool = make_pool();
        let _rx1 = pool.register("user-a", "conn-1");
        let _rx2 = pool.register("user-a", "conn-2");
        pool.add_user_channel("user-a", "ch-1");

        pool.unregister("user-a", "conn-1");
        // Still online via conn-2 — cache must be retained.
        assert!(pool.is_channel_member("user-a", "ch-1"));
    }

    // ── Register ─────────────────────────────────────────────────────

    #[test]
    fn register_inserts_connection() {
        let pool = make_pool();
        let _rx = pool.register("user-a", "conn-1");

        assert!(pool.connections.contains_key("conn-1"));
        let meta = pool.connections.get("conn-1").unwrap();
        assert_eq!(meta.user_id, "user-a");
        assert!(meta.subscribed_channels.is_empty());
        assert_eq!(meta.connection_id, "conn-1");
    }

    #[test]
    fn register_inserts_into_user_connections() {
        let pool = make_pool();
        let _rx = pool.register("user-a", "conn-1");

        let set = pool.user_connections.get("user-a").unwrap();
        assert!(set.contains("conn-1"));
    }

    #[test]
    fn register_subscribes_to_global_channel() {
        let pool = make_pool();
        let _rx = pool.register("user-a", "conn-1");

        // Global channel should have one subscriber.
        assert_eq!(pool.global_tx.receiver_count(), 1);
    }

    #[test]
    fn register_multiple_connections_increase_receiver_count() {
        let pool = make_pool();
        let _rx1 = pool.register("user-a", "conn-1");
        let _rx2 = pool.register("user-b", "conn-2");

        assert_eq!(pool.global_tx.receiver_count(), 2);
    }

    #[test]
    fn register_receiver_is_valid() {
        let pool = make_pool();
        let rx = pool.register("user-a", "conn-1");

        // Receiver should not be closed and starts empty.
        assert_eq!(rx.len(), 0);
    }

    // ── Unregister ───────────────────────────────────────────────────

    #[test]
    fn unregister_removes_connection() {
        let pool = make_pool();
        let _rx = pool.register("user-a", "conn-1");
        pool.unregister("user-a", "conn-1");

        assert!(!pool.connections.contains_key("conn-1"));
    }

    #[test]
    fn unregister_removes_from_user_connections() {
        let pool = make_pool();
        let _rx = pool.register("user-a", "conn-1");
        pool.unregister("user-a", "conn-1");

        // User entry should be gone (only connection).
        assert!(!pool.user_connections.contains_key("user-a"));
    }

    #[test]
    fn unregister_keeps_user_with_other_connections() {
        let pool = make_pool();
        let _rx1 = pool.register("user-a", "conn-1");
        let _rx2 = pool.register("user-a", "conn-2");
        pool.unregister("user-a", "conn-1");

        let set = pool.user_connections.get("user-a").unwrap();
        assert!(!set.contains("conn-1"));
        assert!(set.contains("conn-2"));
    }

    #[test]
    fn unregister_broadcasts_presence_offline() {
        let pool = make_pool();
        let mut rx = pool.register("user-a", "conn-1");
        // Drain any initial events (none expected from register).
        while rx.try_recv().is_ok() {}

        pool.unregister("user-a", "conn-1");

        // The presence-offline event should have been broadcast globally.
        let event = rx.try_recv().expect("expected presence-offline event");
        match event {
            ServerEvent::Presence { user_id, status } => {
                assert_eq!(user_id, "user-a");
                assert_eq!(status, "offline");
            }
            other => panic!("expected Presence, got {other:?}"),
        }
    }

    #[test]
    fn unregister_presence_offline_delivered_to_all() {
        let pool = make_pool();
        let mut rx1 = pool.register("user-a", "conn-1");
        let mut rx2 = pool.register("user-b", "conn-2");
        while rx1.try_recv().is_ok() {}
        while rx2.try_recv().is_ok() {}

        pool.unregister("user-a", "conn-1");

        // Both receivers should get the presence-offline event.
        let event1 = rx1.try_recv().expect("rx1 expected presence-offline");
        assert!(matches!(event1, ServerEvent::Presence { user_id, status: _ } if user_id == "user-a"));
        let event2 = rx2.try_recv().expect("rx2 expected presence-offline");
        assert!(matches!(event2, ServerEvent::Presence { user_id, status: _ } if user_id == "user-a"));
    }

    // ── Broadcast ────────────────────────────────────────────────────

    #[test]
    fn broadcast_to_channel_delivers_to_subscriber() {
        let pool = make_pool();
        let mut rx = pool.register("user-a", "conn-1");
        // Drain any initial events.
        while rx.try_recv().is_ok() {}

        let n = pool.broadcast_to_channel(
            "ch-1",
            &ServerEvent::ChannelArchived {
                channel_id: "ch-1".into(),
            },
        );
        assert_eq!(n, 0);

        let event = rx.try_recv().unwrap();
        match event {
            ServerEvent::ChannelArchived { channel_id } => {
                assert_eq!(channel_id, "ch-1");
            }
            other => panic!("expected ChannelArchived, got {other:?}"),
        }
    }

    #[test]
    fn broadcast_to_channel_unknown_channel_returns_zero() {
        let pool = make_pool();
        let n = pool.broadcast_to_channel(
            "no-such-channel",
            &ServerEvent::Pong,
        );
        assert_eq!(n, 0);
    }

    #[test]
    fn broadcast_to_channel_no_subscribers_returns_zero() {
        let pool = make_pool();
        let rx = pool.register("user-a", "conn-1");
        drop(rx); // no active receivers
        let n = pool.broadcast_to_channel(
            "ch-1",
            &ServerEvent::Pong,
        );
        // With no receivers, send returns Ok(0); our wrapper also returns 0.
        assert_eq!(n, 0);
    }

    #[test]
    fn broadcast_to_channel_all_subscribers_receive() {
        let pool = make_pool();
        let mut rx1 = pool.register("user-a", "conn-1");
        let mut rx2 = pool.register("user-b", "conn-2");
        // Drain any initial events.
        while rx1.try_recv().is_ok() {}
        while rx2.try_recv().is_ok() {}

        pool.broadcast_to_channel(
            "ch-1",
            &ServerEvent::NewMsg {
                channel_id: "ch-1".into(),
                cursor: 1,
                sender_id: "sender".into(),
                msg_type: "text".into(),
                preview: "hello".into(),
                quoted_message_id: None,
            },
        );

        assert!(rx1.try_recv().is_ok());
        assert!(rx2.try_recv().is_ok());
    }

    // ── Notify channel ───────────────────────────────────────────────

    #[test]
    fn notify_channel_pushes_event() {
        let pool = make_pool();
        let mut rx = pool.register("user-a", "conn-1");
        // Drain any initial events.
        while rx.try_recv().is_ok() {}

        let n = pool.notify_channel(
            "ch-1",
            ServerEvent::NewMsg {
                channel_id: "ch-1".into(),
                cursor: 42,
                sender_id: "sender-1".into(),
                msg_type: "text".into(),
                preview: "hello".into(),
                quoted_message_id: None,
            },
        );
        assert_eq!(n, 0);

        let event = rx.try_recv().unwrap();
        match event {
            ServerEvent::NewMsg { cursor, .. } => assert_eq!(cursor, 42),
            other => panic!("expected NewMsg, got {other:?}"),
        }
    }

    // ── Subscribe / Unsubscribe ──────────────────────────────────────

    #[test]
    fn subscribe_channel_adds_to_meta() {
        let pool = make_pool();
        let _rx = pool.register("user-a", "conn-1");
        pool.subscribe_channel("conn-1", "ch-1");

        let meta = pool.connections.get("conn-1").unwrap();
        assert!(meta.subscribed_channels.contains(&"ch-1".to_string()));
    }

    #[test]
    fn subscribe_channel_does_not_duplicate() {
        let pool = make_pool();
        let _rx = pool.register("user-a", "conn-1");
        pool.subscribe_channel("conn-1", "ch-1");
        pool.subscribe_channel("conn-1", "ch-1");

        let meta = pool.connections.get("conn-1").unwrap();
        assert_eq!(meta.subscribed_channels.len(), 1);
    }

    #[test]
    fn unsubscribe_channel_removes_from_meta() {
        let pool = make_pool();
        let _rx = pool.register("user-a", "conn-1");
        pool.subscribe_channel("conn-1", "ch-1");
        pool.unsubscribe_channel("conn-1", "ch-1");

        let meta = pool.connections.get("conn-1").unwrap();
        assert!(!meta.subscribed_channels.contains(&"ch-1".to_string()));
    }

    // ── Channel members ──────────────────────────────────────────────

    #[test]
    fn get_channel_members_returns_unique_user_ids() {
        let pool = make_pool();
        let _rx1 = pool.register("user-a", "conn-1");
        let _rx2 = pool.register("user-b", "conn-2");
        let _rx3 = pool.register("user-a", "conn-3"); // same user

        pool.subscribe_channel("conn-1", "ch-1");
        pool.subscribe_channel("conn-2", "ch-1");
        pool.subscribe_channel("conn-3", "ch-1");

        let members = pool.get_channel_members("ch-1");
        assert_eq!(members.len(), 2);
        assert!(members.contains(&"user-a".to_string()));
        assert!(members.contains(&"user-b".to_string()));
    }

    #[test]
    fn get_channel_members_unknown_channel_returns_empty() {
        let pool = make_pool();
        let _rx = pool.register("user-a", "conn-1");
        pool.subscribe_channel("conn-1", "ch-1");

        let members = pool.get_channel_members("no-such-channel");
        assert!(members.is_empty());
    }

    #[test]
    fn get_channel_members_respects_channel_boundaries() {
        let pool = make_pool();
        let _rx1 = pool.register("user-a", "conn-1");
        let _rx2 = pool.register("user-b", "conn-2");

        pool.subscribe_channel("conn-1", "ch-1");
        pool.subscribe_channel("conn-2", "ch-2");

        let ch1 = pool.get_channel_members("ch-1");
        let ch2 = pool.get_channel_members("ch-2");
        assert_eq!(ch1, vec!["user-a".to_string()]);
        assert_eq!(ch2, vec!["user-b".to_string()]);
    }

    #[test]
    fn get_channel_members_requires_explicit_subscribe() {
        let pool = make_pool();
        // Register but never subscribe_channel — should not show up as member.
        let _rx = pool.register("user-a", "conn-1");

        let members = pool.get_channel_members("ch-1");
        assert!(members.is_empty());
    }

    // ── Typing indicators ────────────────────────────────────────────

    #[test]
    fn record_typing_adds_to_state() {
        let pool = make_pool();
        pool.record_typing("ch-1", "user-a");
        assert!(pool.typing_timeouts.contains_key("ch-1:user-a"));
    }

    #[test]
    fn remove_typing_clears_state() {
        let pool = make_pool();
        pool.record_typing("ch-1", "user-a");
        pool.remove_typing("ch-1", "user-a");
        assert!(!pool.typing_timeouts.contains_key("ch-1:user-a"));
    }

    #[test]
    fn typing_timeout_cleans_stale_entries() {
        let pool = make_pool();
        pool.typing_timeouts
            .insert("ch-1:user-a".into(), Instant::now() - Duration::from_secs(10));
        pool.cleanup_stale_typing();
        assert!(pool.typing_timeouts.is_empty());
    }

    #[test]
    fn typing_recent_not_cleaned() {
        let pool = make_pool();
        pool.typing_timeouts.insert("ch-1:user-a".into(), Instant::now());
        pool.cleanup_stale_typing();
        assert!(pool.typing_timeouts.contains_key("ch-1:user-a"));
    }

    #[test]
    fn multiple_typists_in_same_channel() {
        let pool = make_pool();
        pool.record_typing("ch-1", "user-a");
        pool.record_typing("ch-1", "user-b");
        assert!(pool.typing_timeouts.contains_key("ch-1:user-a"));
        assert!(pool.typing_timeouts.contains_key("ch-1:user-b"));
    }

    #[test]
    fn typing_broadcast_delivers_to_other_users() {
        let pool = make_pool();
        let mut rx_other = pool.register("user-b", "conn-b");
        // Drain any initial events.
        while rx_other.try_recv().is_ok() {}

        pool.broadcast_to_channel(
            "ch-1",
            &ServerEvent::Typing {
                channel_id: "ch-1".into(),
                user_id: "user-a".into(),
                thread_parent_cursor: None,
            },
        );

        let event = rx_other.try_recv().expect("expected Typing event");
        match event {
            ServerEvent::Typing {
                channel_id,
                user_id,
                thread_parent_cursor,
            } => {
                assert_eq!(channel_id, "ch-1");
                assert_eq!(user_id, "user-a");
                assert_eq!(thread_parent_cursor, None);
            }
            other => panic!("expected Typing, got {other:?}"),
        }
    }
}
