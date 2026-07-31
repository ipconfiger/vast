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
        quoted_message_id: Option<i64>,
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
        quoted_message_id: Option<i64>,
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
    DmCreated {
        dm_channel_id: String,
        participant_ids: Vec<String>,
    },
    DmClosed {
        dm_channel_id: String,
        participant_ids: Vec<String>,
    },
    Pong,
    /// Push a mention context to a connected bot connector.
    BotMention {
        /// The bot user this mention targets. Only the matching bot
        /// connection should process it; others must drop it.
        bot_user_id: String,
        mention_id: String,
        channel_id: String,
        messages: Vec<serde_json::Value>,
        model: String,
        system_prompt: String,
    },
}

impl ServerEvent {
    /// Return the `channel_id` of content events that must be filtered by
    /// channel membership on the receiving end (C3 isolation).
    ///
    /// Management events (member added/removed, invitations, archive, join
    /// requests), user-scoped events (Presence, Pong, Error), DM events
    /// (filtered separately by participant list) and BotMention (filtered
    /// separately for bots) all return `None`.
    pub fn channel_id(&self) -> Option<&str> {
        match self {
            Self::NewMsg { channel_id, .. }
            | Self::MsgUpdated { channel_id }
            | Self::MsgDeleted { channel_id, .. }
            | Self::FileDeleted { channel_id, .. }
            | Self::ReactionUpdate { channel_id, .. }
            | Self::ThreadReply { channel_id, .. }
            | Self::Typing { channel_id, .. }
            | Self::TrainUpdated { channel_id, .. }
            | Self::VoteUpdated { channel_id, .. } => Some(channel_id.as_str()),
            _ => None,
        }
    }
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
    /// Bot connector sends this when the LLM has produced a reply.
    BotReply {
        mention_id: String,
        channel_id: String,
        text: String,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn channel_id_for_content_events() {
        assert_eq!(
            ServerEvent::NewMsg {
                channel_id: "c1".into(),
                cursor: 1,
                sender_id: "s".into(),
                msg_type: "text".into(),
                preview: "p".into(),
                quoted_message_id: None,
            }
            .channel_id(),
            Some("c1")
        );
        assert_eq!(
            ServerEvent::MsgUpdated { channel_id: "c1".into() }.channel_id(),
            Some("c1")
        );
        assert_eq!(
            ServerEvent::MsgDeleted { channel_id: "c1".into(), cursor: 1 }.channel_id(),
            Some("c1")
        );
        assert_eq!(
            ServerEvent::FileDeleted {
                file_id: "f1".into(),
                channel_id: "c1".into(),
            }
            .channel_id(),
            Some("c1")
        );
        assert_eq!(
            ServerEvent::ReactionUpdate {
                channel_id: "c1".into(),
                message_cursor: 1,
                reactions: vec![],
            }
            .channel_id(),
            Some("c1")
        );
        assert_eq!(
            ServerEvent::ThreadReply {
                channel_id: "c1".into(),
                thread_parent_cursor: 1,
                cursor: 2,
                sender_id: "s".into(),
                preview: "p".into(),
                quoted_message_id: None,
            }
            .channel_id(),
            Some("c1")
        );
        assert_eq!(
            ServerEvent::Typing {
                channel_id: "c1".into(),
                user_id: "u".into(),
                thread_parent_cursor: None,
            }
            .channel_id(),
            Some("c1")
        );
        assert_eq!(
            ServerEvent::TrainUpdated {
                train_id: "t1".into(),
                channel_id: "c1".into(),
            }
            .channel_id(),
            Some("c1")
        );
        assert_eq!(
            ServerEvent::VoteUpdated {
                vote_id: "v1".into(),
                channel_id: "c1".into(),
            }
            .channel_id(),
            Some("c1")
        );
    }

    #[test]
    fn channel_id_none_for_management_and_user_events() {
        assert_eq!(ServerEvent::Pong.channel_id(), None);
        assert_eq!(
            ServerEvent::Presence {
                user_id: "u".into(),
                status: "online".into(),
            }
            .channel_id(),
            None
        );
        assert_eq!(
            ServerEvent::JoinRequest {
                channel_id: "c1".into(),
                user_id: "u".into(),
                username: "n".into(),
            }
            .channel_id(),
            None
        );
        assert_eq!(
            ServerEvent::Invitation {
                channel_id: "c1".into(),
                channel_name: "n".into(),
                inviter_id: "i".into(),
                inviter_name: "n".into(),
            }
            .channel_id(),
            None
        );
        assert_eq!(
            ServerEvent::ChannelArchived { channel_id: "c1".into() }.channel_id(),
            None
        );
        assert_eq!(
            ServerEvent::ChannelUnarchived { channel_id: "c1".into() }.channel_id(),
            None
        );
        assert_eq!(
            ServerEvent::MemberAdded {
                channel_id: "c1".into(),
                user_id: "u".into(),
                username: "n".into(),
            }
            .channel_id(),
            None
        );
        assert_eq!(
            ServerEvent::MemberRemoved {
                channel_id: "c1".into(),
                user_id: "u".into(),
            }
            .channel_id(),
            None
        );
        assert_eq!(
            ServerEvent::Error {
                code: "x".into(),
                message: "m".into(),
            }
            .channel_id(),
            None
        );
        assert_eq!(
            ServerEvent::DmCreated {
                dm_channel_id: "d1".into(),
                participant_ids: vec![],
            }
            .channel_id(),
            None
        );
        assert_eq!(
            ServerEvent::DmClosed {
                dm_channel_id: "d1".into(),
                participant_ids: vec![],
            }
            .channel_id(),
            None
        );
        assert_eq!(
            ServerEvent::BotMention {
                bot_user_id: "b".into(),
                mention_id: "m".into(),
                channel_id: "c1".into(),
                messages: vec![],
                model: "g".into(),
                system_prompt: "p".into(),
            }
            .channel_id(),
            None
        );
    }
}
