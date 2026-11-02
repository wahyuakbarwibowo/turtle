//! Thin wrapper over the official LiveKit server SDK (`livekit-api`).
//! Keeps API key/secret server-side; only short-lived join JWTs reach clients.

use std::time::Duration;

pub use livekit_api::services::room::CreateRoomOptions;
use livekit_api::{
    access_token::{AccessToken, AccessTokenError, TokenVerifier, VideoGrants},
    services::{ServerError, ServerErrorCode, ServiceError, room::RoomClient},
    webhooks::{WebhookError, WebhookReceiver},
};
pub use livekit_protocol::WebhookEvent;

#[derive(Debug, thiserror::Error)]
pub enum LiveKitError {
    #[error("livekit token: {0}")]
    Token(#[from] AccessTokenError),
    #[error("livekit api: {0}")]
    Service(#[from] ServiceError),
    #[error("livekit webhook: {0}")]
    Webhook(#[from] WebhookError),
}

/// Treat "already gone" as success so removal/deletion is idempotent.
fn ignore_not_found(r: Result<(), ServiceError>) -> Result<(), LiveKitError> {
    match r {
        Err(ServiceError::Twirp(ServerError::Twirp(ref c)))
            if c.code == ServerErrorCode::NOT_FOUND =>
        {
            Ok(())
        }
        other => Ok(other?),
    }
}

pub struct LiveKit {
    rooms: RoomClient,
    webhooks: WebhookReceiver,
    api_key: String,
    api_secret: String,
}

/// Who is joining and what they may do in the room.
pub struct JoinGrant<'a> {
    pub room: &'a str,
    pub identity: &'a str,
    pub name: &'a str,
    /// Exposed to other clients as the `role` participant attribute (e.g. "host").
    pub role: &'a str,
    pub ttl: Duration,
}

impl LiveKit {
    /// `api_url` is the server-to-server URL, e.g. `http://localhost:7880`.
    pub fn new(api_url: &str, api_key: &str, api_secret: &str) -> Self {
        Self {
            rooms: RoomClient::with_api_key(api_url, api_key, api_secret),
            webhooks: WebhookReceiver::new(TokenVerifier::with_api_key(api_key, api_secret)),
            api_key: api_key.to_owned(),
            api_secret: api_secret.to_owned(),
        }
    }

    /// Join token scoped to a single room. No room-admin rights: moderation goes through our API.
    pub fn join_token(&self, g: JoinGrant<'_>) -> Result<String, LiveKitError> {
        let grants = VideoGrants {
            room_join: true,
            room: g.room.to_owned(),
            can_publish: Some(true),
            can_subscribe: Some(true),
            can_publish_data: Some(true),
            ..Default::default()
        };
        Ok(AccessToken::with_api_key(&self.api_key, &self.api_secret)
            .with_identity(g.identity)
            .with_name(g.name)
            .with_attributes([("role", g.role)])
            .with_ttl(g.ttl)
            .with_grants(grants)
            .to_jwt()?)
    }

    /// Idempotent: LiveKit returns the existing room if it already exists.
    pub async fn ensure_room(&self, name: &str, max_participants: u32) -> Result<(), LiveKitError> {
        let opts = CreateRoomOptions {
            empty_timeout: 300,
            departure_timeout: 20,
            max_participants,
            ..Default::default()
        };
        self.rooms.create_room(name, opts).await?;
        Ok(())
    }

    pub async fn remove_participant(&self, room: &str, identity: &str) -> Result<(), LiveKitError> {
        ignore_not_found(self.rooms.remove_participant(room, identity).await)
    }

    pub async fn delete_room(&self, room: &str) -> Result<(), LiveKitError> {
        ignore_not_found(self.rooms.delete_room(room).await)
    }

    /// Verify the webhook JWT signature + body hash, then decode the event.
    pub fn receive_webhook(
        &self,
        body: &str,
        auth_header: &str,
    ) -> Result<WebhookEvent, LiveKitError> {
        Ok(self.webhooks.receive(body, auth_header)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn join_token_is_room_scoped() {
        let lk = LiveKit::new(
            "http://localhost:7880",
            "key",
            "a-secret-of-at-least-32-characters!",
        );
        let jwt = lk
            .join_token(JoinGrant {
                room: "abc-defg-hij",
                identity: "u-1",
                role: "host",
                name: "Ana",
                ttl: Duration::from_secs(60),
            })
            .expect("token");
        let claims = TokenVerifier::with_api_key("key", "a-secret-of-at-least-32-characters!")
            .verify(&jwt)
            .expect("verify");
        assert_eq!(claims.sub, "u-1");
        assert_eq!(claims.video.room, "abc-defg-hij");
        assert!(claims.video.room_join && !claims.video.room_admin);
        assert_eq!(
            claims.attributes.get("role").map(String::as_str),
            Some("host")
        );
        assert!(
            TokenVerifier::with_api_key("key", "wrong-secret-wrong-secret-wrong-secret")
                .verify(&jwt)
                .is_err()
        );
    }
}
