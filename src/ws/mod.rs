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

/// Capacity of each per-channel broadcast channel.
const BROADCAST_CHANNEL_CAPACITY: usize = 256;

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
/// per-channel broadcasters, and connection metadata.
///
/// All methods take `&self` — interior mutability via DashMap.
pub struct ConnectionPool {
    /// user_id -> set of active connection_ids
    pub user_connections: DashMap<UserId, DashSet<ConnectionId>>,
    /// channel_id -> broadcast sender for server-event fan-out
    pub channel_broadcasters: DashMap<ChannelId, broadcast::Sender<ServerEvent>>,
    /// connection_id -> metadata
    pub connections: DashMap<ConnectionId, ConnectionMeta>,
    /// Keeps a broadcast channel alive for connections that haven't
    /// subscribed to any real channel yet. Its subscribers simply
    /// block forever (the select! loop handles other branches).
    #[allow(dead_code)]
    idle_tx: broadcast::Sender<ServerEvent>,

    /// Tracks typing state: `"{channel_id}:{user_id}"` -> last-seen `Instant`.
    /// Entries older than 5 seconds are considered stale and cleaned
    /// up lazily.
    pub typing_timeouts: DashMap<String, Instant>,
}

impl Default for ConnectionPool {
    fn default() -> Self {
        Self::new()
    }
}

impl ConnectionPool {
    pub fn new() -> Self {
        let (idle_tx, _) = broadcast::channel(1);
        Self {
            user_connections: DashMap::new(),
            channel_broadcasters: DashMap::new(),
            connections: DashMap::new(),
            idle_tx,
            typing_timeouts: DashMap::new(),
        }
    }

    /// Register a new WebSocket connection for a user.
    ///
    /// Subscribes the connection to the given channels and returns
    /// a broadcast receiver that the per-connection send loop polls.
    #[instrument(skip(self))]
    pub fn register(
        &self,
        user_id: &str,
        connection_id: &str,
        channels: &[String],
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
                subscribed_channels: channels.to_vec(),
                joined_at: Instant::now(),
            },
        );

        let mut rx: Option<broadcast::Receiver<ServerEvent>> = None;
        for channel_id in channels {
            let tx = self
                .channel_broadcasters
                .entry(channel_id.clone())
                .or_insert_with(|| {
                    let (tx, _) = broadcast::channel(BROADCAST_CHANNEL_CAPACITY);
                    tx
                })
                .clone();
            rx = Some(tx.subscribe());
        }

    // Broadcast presence-online to every subscribed channel.
    let presence = ServerEvent::Presence {
        user_id: user_id.to_string(),
        status: "online".to_string(),
    };
    for channel_id in channels {
        self.broadcast_to_channel(channel_id, &presence);
    }

    // If no channels provided, subscribe to the idle sender
    // (blocks forever — avoids busy-looping on RecvError::Closed).
    rx.unwrap_or_else(|| self.idle_tx.subscribe())
    }

    /// Remove a connection, broadcast presence-offline, and clean up
    /// empty channel broadcasters.
    #[instrument(skip(self))]
    pub fn unregister(&self, user_id: &str, connection_id: &str) {
        // Snapshot subscribed channels *before* removing the connection.
        let channels: Vec<String> = self
            .connections
            .get(connection_id)
            .map(|m| m.subscribed_channels.clone())
            .unwrap_or_default();

        self.connections.remove(connection_id);

        if let Some(entry) = self.user_connections.get_mut(user_id) {
            entry.remove(connection_id);
            if entry.is_empty() {
                drop(entry);
                self.user_connections.remove(user_id);
            }
        }

        // Broadcast presence-offline to every subscribed channel.
        let presence = ServerEvent::Presence {
            user_id: user_id.to_string(),
            status: "offline".to_string(),
        };
        for channel_id in &channels {
            self.broadcast_to_channel(channel_id, &presence);

            // Remove the per-channel broadcaster if no receivers remain.
            let is_empty = self
                .channel_broadcasters
                .get(channel_id)
                .map(|tx| tx.receiver_count() == 0)
                .unwrap_or(true);
            if is_empty {
                self.channel_broadcasters.remove(channel_id);
            }
        }

        debug!(user_id, connection_id, "Unregistered WebSocket connection");
    }

    /// Fan-out a ServerEvent to all connections subscribed to a channel.
    ///
    /// Returns the number of active subscribers that received the event.
    #[instrument(skip(self, event))]
    pub fn broadcast_to_channel(&self, channel_id: &str, event: &ServerEvent) -> usize {
        let Some(sender) = self.channel_broadcasters.get(channel_id) else {
            return 0;
        };

        match sender.send(event.clone()) {
            Ok(n) => {
                debug!(channel_id, n, "Broadcast event");
                n
            }
            Err(_) => {
                0
            }
        }
    }

    /// Return the set of user IDs currently connected to a channel.
    pub fn get_channel_members(&self, channel_id: &str) -> Vec<UserId> {
        if !self.channel_broadcasters.contains_key(channel_id) {
            return vec![];
        }

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

    let user_id = claims.sub;
    info!(user_id, "WebSocket upgrade requested");

    ws.on_upgrade(move |socket| handle_socket(socket, user_id, state.ws_pool.clone()))
}

// ---------------------------------------------------------------------------
// Per-socket handler
// ---------------------------------------------------------------------------

/// Multiplexed loop that reads from three sources:
///
/// 1. WebSocket receive — parse incoming ClientEvent messages.
/// 2. Broadcast channel — push ServerEvent fan-out to the client.
/// 3. Heartbeat timer — send periodic Ping frames (15 s interval,
///    60 s idle timeout).
#[instrument(skip(socket, pool))]
async fn handle_socket(mut socket: WebSocket, user_id: UserId, pool: Arc<ConnectionPool>) {
    let connection_id = Uuid::new_v4().to_string();
    info!(user_id, connection_id, "WebSocket connection opened");

    let initial_channels: Vec<String> = Vec::new();
    let mut broadcast_rx = pool.register(&user_id, &connection_id, &initial_channels);
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
        assert!(pool.channel_broadcasters.is_empty());
    }

    // ── Register ─────────────────────────────────────────────────────

    #[test]
    fn register_inserts_connection() {
        let pool = make_pool();
        let channels = vec!["ch-1".to_string()];
        let _rx = pool.register("user-a", "conn-1", &channels);

        assert!(pool.connections.contains_key("conn-1"));
        let meta = pool.connections.get("conn-1").unwrap();
        assert_eq!(meta.user_id, "user-a");
        assert_eq!(meta.subscribed_channels, channels);
        assert_eq!(meta.connection_id, "conn-1");
    }

    #[test]
    fn register_inserts_into_user_connections() {
        let pool = make_pool();
        let _rx = pool.register("user-a", "conn-1", &["ch-1".into()]);

        let set = pool.user_connections.get("user-a").unwrap();
        assert!(set.contains("conn-1"));
    }

    #[test]
    fn register_creates_channel_broadcaster() {
        let pool = make_pool();
        let _rx = pool.register("user-a", "conn-1", &["ch-1".into()]);

        assert!(pool.channel_broadcasters.contains_key("ch-1"));
    }

    #[test]
    fn register_multiple_channels_subscribes_to_last() {
        let pool = make_pool();
        let channels = vec!["ch-a".to_string(), "ch-b".to_string()];
        let _rx = pool.register("user-a", "conn-1", &channels);

        // Both broadcasters exist.
        assert!(pool.channel_broadcasters.contains_key("ch-a"));
        assert!(pool.channel_broadcasters.contains_key("ch-b"));
    }

    #[test]
    fn register_with_empty_channels_uses_idle_tx() {
        let pool = make_pool();
        let rx = pool.register("user-a", "conn-1", &[]);
        // Idle receiver: no events, not closed.
        assert_eq!(rx.len(), 0);
    }

    // ── Unregister ───────────────────────────────────────────────────

    #[test]
    fn unregister_removes_connection() {
        let pool = make_pool();
        let _rx = pool.register("user-a", "conn-1", &["ch-1".into()]);
        pool.unregister("user-a", "conn-1");

        assert!(!pool.connections.contains_key("conn-1"));
    }

    #[test]
    fn unregister_removes_from_user_connections() {
        let pool = make_pool();
        let _rx = pool.register("user-a", "conn-1", &["ch-1".into()]);
        pool.unregister("user-a", "conn-1");

        // User entry should be gone (only connection).
        assert!(!pool.user_connections.contains_key("user-a"));
    }

    #[test]
    fn unregister_keeps_user_with_other_connections() {
        let pool = make_pool();
        let _rx1 = pool.register("user-a", "conn-1", &["ch-1".into()]);
        let _rx2 = pool.register("user-a", "conn-2", &["ch-1".into()]);
        pool.unregister("user-a", "conn-1");

        let set = pool.user_connections.get("user-a").unwrap();
        assert!(!set.contains("conn-1"));
        assert!(set.contains("conn-2"));
    }

    #[test]
    fn unregister_broadcasts_presence_offline() {
        let pool = make_pool();
        let mut rx = pool.register("user-a", "conn-1", &["ch-1".into()]);
        // Drain the subscription message (none expected, but let's be safe).
        while rx.try_recv().is_ok() {}

        pool.unregister("user-a", "conn-1");

        // The presence-offline event should have been broadcast.
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
    fn unregister_cleans_up_empty_broadcaster() {
        let pool = make_pool();
        let _rx = pool.register("user-a", "conn-1", &["ch-1".into()]);
        drop(_rx); // simulate handle_socket going out of scope
        pool.unregister("user-a", "conn-1");

        // Broadcaster should have been removed (0 receivers left).
        assert!(!pool.channel_broadcasters.contains_key("ch-1"));
    }

    #[test]
    fn unregister_keeps_broadcaster_if_others_listening() {
        let pool = make_pool();
        let _rx1 = pool.register("user-a", "conn-1", &["ch-1".into()]);
        let _rx2 = pool.register("user-b", "conn-2", &["ch-1".into()]);
        pool.unregister("user-a", "conn-1");

        // Broadcaster still alive (conn-2 still subscribed).
        assert!(pool.channel_broadcasters.contains_key("ch-1"));
    }

    // ── Broadcast ────────────────────────────────────────────────────

    #[test]
    fn broadcast_to_channel_delivers_to_subscriber() {
        let pool = make_pool();
        let mut rx = pool.register("user-a", "conn-1", &["ch-1".into()]);
        // Drain the presence-online emitted during register.
        while rx.try_recv().is_ok() {}

        let n = pool.broadcast_to_channel(
            "ch-1",
            &ServerEvent::ChannelArchived {
                channel_id: "ch-1".into(),
            },
        );
        assert_eq!(n, 1);

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
        let rx = pool.register("user-a", "conn-1", &["ch-1".into()]);
        drop(rx); // no active receivers
        let n = pool.broadcast_to_channel(
            "ch-1",
            &ServerEvent::Pong,
        );
        // send() with no receivers returns Ok(0).
        assert_eq!(n, 0);
    }

    // ── Notify channel ───────────────────────────────────────────────

    #[test]
    fn notify_channel_pushes_event() {
        let pool = make_pool();
        let mut rx = pool.register("user-a", "conn-1", &["ch-1".into()]);
        // Drain the presence-online emitted during register.
        while rx.try_recv().is_ok() {}

        let n = pool.notify_channel(
            "ch-1",
            ServerEvent::NewMsg {
                channel_id: "ch-1".into(),
                cursor: 42,
                sender_id: "sender-1".into(),
                msg_type: "text".into(),
                preview: "hello".into(),
            },
        );
        assert_eq!(n, 1);

        let event = rx.try_recv().unwrap();
        match event {
            ServerEvent::NewMsg { cursor, .. } => assert_eq!(cursor, 42),
            other => panic!("expected NewMsg, got {other:?}"),
        }
    }

    // ── Register broadcasts presence online ──────────────────────────

    #[test]
    fn register_broadcasts_presence_online() {
        let pool = make_pool();
        // Observer subscribes first so it can witness the broadcast.
        let mut rx_obs = pool.register("observer", "conn-obs", &["ch-1".into()]);
        // Drain the observer's own presence-online (emitted during register).
        while rx_obs.try_recv().is_ok() {}

        let _rx_user = pool.register("user-a", "conn-1", &["ch-1".into()]);

        let event = rx_obs.try_recv().expect("expected presence-online");
        match event {
            ServerEvent::Presence { user_id, status } => {
                assert_eq!(user_id, "user-a");
                assert_eq!(status, "online");
            }
            other => panic!("expected Presence, got {other:?}"),
        }
    }

    #[test]
    fn register_broadcasts_presence_to_all_channels() {
        let pool = make_pool();
        let mut rx_ch1 = pool.register("obs-a", "conn-obs-a", &["ch-1".into()]);
        let mut rx_ch2 = pool.register("obs-b", "conn-obs-b", &["ch-2".into()]);
        while rx_ch1.try_recv().is_ok() {}
        while rx_ch2.try_recv().is_ok() {}

        let channels = vec!["ch-1".to_string(), "ch-2".to_string()];
        let _rx_user = pool.register("user-a", "conn-1", &channels);

        // Observer on ch-1 sees online.
        let event = rx_ch1.try_recv().expect("expected presence on ch-1");
        match event {
            ServerEvent::Presence { user_id, status } => {
                assert_eq!(user_id, "user-a");
                assert_eq!(status, "online");
            }
            other => panic!("expected Presence, got {other:?}"),
        }

        // Observer on ch-2 sees online.
        let event = rx_ch2.try_recv().expect("expected presence on ch-2");
        match event {
            ServerEvent::Presence { user_id, status } => {
                assert_eq!(user_id, "user-a");
                assert_eq!(status, "online");
            }
            other => panic!("expected Presence, got {other:?}"),
        }
    }

    // ── Channel members ──────────────────────────────────────────────

    #[test]
    fn get_channel_members_returns_unique_user_ids() {
        let pool = make_pool();
        let _rx1 = pool.register("user-a", "conn-1", &["ch-1".into()]);
        let _rx2 = pool.register("user-b", "conn-2", &["ch-1".into()]);
        let _rx3 = pool.register("user-a", "conn-3", &["ch-1".into()]); // same user

        let members = pool.get_channel_members("ch-1");
        assert_eq!(members.len(), 2);
        assert!(members.contains(&"user-a".to_string()));
        assert!(members.contains(&"user-b".to_string()));
    }

    #[test]
    fn get_channel_members_unknown_channel_returns_empty() {
        let pool = make_pool();
        let members = pool.get_channel_members("no-such-channel");
        assert!(members.is_empty());
    }

    #[test]
    fn get_channel_members_respects_channel_boundaries() {
        let pool = make_pool();
        let _rx1 = pool.register("user-a", "conn-1", &["ch-1".into()]);
        let _rx2 = pool.register("user-b", "conn-2", &["ch-2".into()]);

        let ch1 = pool.get_channel_members("ch-1");
        let ch2 = pool.get_channel_members("ch-2");
        assert_eq!(ch1, vec!["user-a".to_string()]);
        assert_eq!(ch2, vec!["user-b".to_string()]);
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
        let mut rx_other = pool.register("user-b", "conn-b", &["ch-1".into()]);
        // Drain the presence-online broadcast from register.
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
