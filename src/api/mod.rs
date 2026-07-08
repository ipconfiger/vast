pub mod admin;
pub mod auth;
pub mod channel_members;
pub mod channels;
pub mod dm;
pub mod files;
pub mod invitations;
pub mod messages;
pub mod presence;
pub mod push;
pub mod reactions;
pub mod requests;
pub mod search;
pub mod trains;
pub mod votes;

use axum::{routing::{delete, get, post, put}, Router};
use std::sync::Arc;

use crate::AppState;

pub fn routes() -> Router<Arc<AppState>> {
    Router::new()
        .route("/bots", get(channels::list_public_bots))
        .route("/channels", get(channels::list_channels).post(channels::create_channel))
        .route("/channels/discover", get(channels::discover_channels))
        .route("/channels/{id}", get(channels::get_channel).patch(channels::update_channel))
        .route("/channels/{id}/archive", post(channels::archive_channel))
        .route("/channels/{id}/unarchive", post(channels::unarchive_channel))
        .route(
            "/channels/{id}/archive/download",
            get(channels::download_channel_archive),
        )
        .route("/channels/{channel_id}/bots", post(channels::add_bot_to_channel))
        .route("/channels/{channel_id}/messages", get(messages::get_messages).post(messages::send_message))
        .route(
            "/channels/{channel_id}/messages/{msg_id}/thread",
            get(messages::get_thread),
        )
        .route("/messages/{message_id}", delete(messages::delete_message))
        .route("/trains/{train_id}", get(trains::get_train))
        .route("/trains/{train_id}/join", post(trains::join_train))
        .route("/votes/{vote_id}", get(votes::get_vote))
        .route("/votes/{vote_id}/vote", post(votes::cast_vote))
        .route(
            "/messages/{message_id}/reactions",
            get(reactions::get_reactions).post(reactions::add_reaction),
        )
        .route(
            "/messages/{message_id}/reactions/{emoji}",
            delete(reactions::remove_reaction),
        )
        .route("/search", get(search::search_messages))
        .route(
            "/channels/{id}/join-request",
            post(requests::create_join_request),
        )
        .route("/requests", get(requests::list_join_requests))
        .route("/requests/{id}/approve", put(requests::approve_join_request))
        .route("/requests/{id}/reject", put(requests::reject_join_request))
        .route(
            "/channels/{id}/invitations",
            post(invitations::create_invitation),
        )
        .route("/invitations", get(invitations::list_invitations))
        .route("/invitations/{id}/accept", put(invitations::accept_invitation))
        .route("/invitations/{id}/reject", put(invitations::reject_invitation))
        .nest("/dm", dm::dm_routes())
        .route("/push/subscribe", post(push::subscribe_handler))
        .route("/push/unsubscribe", delete(push::unsubscribe_handler))
        .route("/push/resubscribe", post(push::resubscribe_handler))
}
