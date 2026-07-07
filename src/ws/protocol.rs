use serde::{Deserialize, Serialize};

/// Events sent from server to client
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ServerEvent {
    NewMsg {
        channel_id: String,
        cursor: i64,
        sender_id: String,
        msg_type: String,
        preview: String,
    },
    MsgUpdated {
        channel_id: String,
    },
    MsgDeleted {
        channel_id: String,
        cursor: i64,
    },
    FileDeleted {
        file_id: String,
        channel_id: String,
    },
    ReactionUpdate {
        channel_id: String,
        message_cursor: i64,
        reactions: Vec<ReactionSummary>,
    },
    ThreadReply {
        channel_id: String,
        thread_parent_cursor: i64,
        cursor: i64,
        sender_id: String,
        preview: String,
    },
    Typing {
        channel_id: String,
        user_id: String,
        thread_parent_cursor: Option<i64>,
    },
    Presence {
        user_id: String,
        status: String,
    },
    JoinRequest {
        channel_id: String,
        user_id: String,
        username: String,
    },
    Invitation {
        channel_id: String,
        channel_name: String,
        inviter_id: String,
        inviter_name: String,
    },
    ChannelArchived {
        channel_id: String,
    },
    ChannelUnarchived {
        channel_id: String,
    },
    MemberAdded {
        channel_id: String,
        user_id: String,
        username: String,
    },
    MemberRemoved {
        channel_id: String,
        user_id: String,
    },
    TrainUpdated {
        train_id: String,
        channel_id: String,
    },
    VoteUpdated {
        vote_id: String,
        channel_id: String,
    },
    Error {
        code: String,
        message: String,
    },
    Pong,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReactionSummary {
    pub emoji: String,
    pub count: i64,
}

/// Events sent from client to server
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ClientEvent {
    Ping,
    TypingStart {
        channel_id: String,
    },
    TypingStop {
        channel_id: String,
    },
    Subscribe {
        channel_id: String,
    },
    Unsubscribe {
        channel_id: String,
    },
}
