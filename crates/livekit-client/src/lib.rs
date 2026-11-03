//! Thin wrapper over the official LiveKit server SDK (`livekit-api`).
//! Keeps API key/secret server-side; only short-lived join JWTs reach clients.

use std::time::Duration;

pub use livekit_api::services::room::CreateRoomOptions;
use livekit_api::{
    access_token::{AccessToken, AccessTokenError, TokenVerifier, VideoGrants},
    services::{
        ServerError, ServerErrorCode, ServiceError,
        room::{RoomClient, SendDataOptions, UpdateParticipantOptions},
    },
    webhooks::{WebhookError, WebhookReceiver},
};
pub use livekit_protocol::WebhookEvent;
use livekit_protocol::{ParticipantInfo, TrackSource, data_packet::Kind};

#[derive(Debug, thiserror::Error)]
pub enum LiveKitError {
    #[error("livekit token: {0}")]
    Token(#[from] AccessTokenError),
    #[error("livekit api: {0}")]
    Service(#[from] ServiceError),
    #[error("livekit webhook: {0}")]
    Webhook(#[from] WebhookError),
    #[error("token is not a {0} token")]
    WrongTokenKind(&'static str),
}

/// Attribute marking which of our own token types a JWT is, so one can't stand in for another.
const TOKEN_KIND_ATTR: &str = "tm_token";

/// Verified claims of one of our app tokens (participant or embed).
#[derive(Debug, Clone, PartialEq)]
pub struct AppToken {
    pub room: String,
    pub identity: String,
    pub name: String,
    pub attributes: std::collections::HashMap<String, String>,
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
            // Attributes (role, raised hand) are server-written only; clients must not spoof "host".
            can_update_own_metadata: Some(false),
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

    /// Signs one of our own app tokens. They carry no `room_join` grant, so LiveKit
    /// refuses them for media; only our API accepts them.
    fn app_token(
        &self,
        kind: &'static str,
        room: &str,
        identity: &str,
        name: &str,
        attributes: &[(&str, &str)],
        ttl: Duration,
    ) -> Result<String, LiveKitError> {
        let grants = VideoGrants {
            room_join: false,
            room: room.to_owned(),
            can_publish: Some(false),
            can_subscribe: Some(false),
            can_publish_data: Some(false),
            ..Default::default()
        };
        let attrs = attributes
            .iter()
            .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
            .chain([(TOKEN_KIND_ATTR.to_owned(), kind.to_owned())]);
        Ok(AccessToken::with_api_key(&self.api_key, &self.api_secret)
            .with_identity(identity)
            .with_name(name)
            .with_ttl(ttl)
            .with_attributes(attrs)
            .with_grants(grants)
            .to_jwt()?)
    }

    fn verify_app_token(&self, token: &str, kind: &'static str) -> Result<AppToken, LiveKitError> {
        let claims = TokenVerifier::with_api_key(&self.api_key, &self.api_secret).verify(token)?;
        if claims.attributes.get(TOKEN_KIND_ATTR).map(String::as_str) != Some(kind)
            || claims.video.room_join
        {
            return Err(LiveKitError::WrongTokenKind(kind));
        }
        Ok(AppToken {
            room: claims.video.room,
            identity: claims.sub,
            name: claims.name,
            attributes: claims.attributes,
        })
    }

    /// Long-lived token proving "identity X is in room Y" to *our* API (chat, moderation…).
    pub fn participant_token(
        &self,
        room: &str,
        identity: &str,
        ttl: Duration,
    ) -> Result<String, LiveKitError> {
        self.app_token("participant", room, identity, "", &[], ttl)
    }

    /// Verifies a token from [`Self::participant_token`]; returns `(room, identity)`.
    pub fn verify_participant_token(&self, token: &str) -> Result<(String, String), LiveKitError> {
        let t = self.verify_app_token(token, "participant")?;
        Ok((t.room, t.identity))
    }

    /// Short-lived token an integrator's backend hands to its user; the embed page
    /// exchanges it for a join. `role` is "host" or "participant".
    pub fn embed_token(
        &self,
        room: &str,
        identity: &str,
        name: &str,
        role: &str,
        ttl: Duration,
    ) -> Result<String, LiveKitError> {
        self.app_token("embed", room, identity, name, &[("role", role)], ttl)
    }

    pub fn verify_embed_token(&self, token: &str) -> Result<AppToken, LiveKitError> {
        self.verify_app_token(token, "embed")
    }

    /// Reliable broadcast to every participant in the room on `topic`.
    pub async fn broadcast(
        &self,
        room: &str,
        topic: &str,
        data: Vec<u8>,
    ) -> Result<(), LiveKitError> {
        let opts = SendDataOptions {
            kind: Kind::Reliable,
            topic: Some(topic.to_owned()),
            ..Default::default()
        };
        self.rooms.send_data(room, data, opts).await?;
        Ok(())
    }

    /// Server-side mute of a participant's microphone. Remote unmute is not possible,
    /// so the participant must unmute themselves.
    pub async fn mute_microphone(&self, room: &str, identity: &str) -> Result<(), LiveKitError> {
        match self.rooms.get_participant(room, identity).await {
            Ok(p) => self.mute_mics_of(room, &p).await,
            // Not connected: nothing to mute.
            Err(e) => ignore_not_found(Err(e)),
        }
    }

    /// Mutes every microphone in the room except `except_identity` (the host).
    pub async fn mute_all_microphones(
        &self,
        room: &str,
        except_identity: &str,
    ) -> Result<(), LiveKitError> {
        for p in self.rooms.list_participants(room).await? {
            if p.identity != except_identity {
                self.mute_mics_of(room, &p).await?;
            }
        }
        Ok(())
    }

    async fn mute_mics_of(&self, room: &str, p: &ParticipantInfo) -> Result<(), LiveKitError> {
        for t in p
            .tracks
            .iter()
            .filter(|t| t.source == TrackSource::Microphone as i32 && !t.muted)
        {
            ignore_not_found(
                self.rooms
                    .mute_published_track(room, &p.identity, &t.sid, true)
                    .await
                    .map(|_| ()),
            )?;
        }
        Ok(())
    }

    /// Server-authoritative participant attributes (merged into existing ones).
    pub async fn set_attributes(
        &self,
        room: &str,
        identity: &str,
        attributes: impl IntoIterator<Item = (String, String)>,
    ) -> Result<(), LiveKitError> {
        let opts = UpdateParticipantOptions {
            attributes: attributes.into_iter().collect(),
            ..Default::default()
        };
        self.rooms.update_participant(room, identity, opts).await?;
        Ok(())
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
        assert!(
            !claims.video.can_update_own_metadata(),
            "clients must not set their own role"
        );
        assert_eq!(
            claims.attributes.get("role").map(String::as_str),
            Some("host")
        );
        let pt = lk
            .participant_token("abc-defg-hij", "g-2", Duration::from_secs(60))
            .expect("participant token");
        assert_eq!(
            lk.verify_participant_token(&pt).ok(),
            Some(("abc-defg-hij".to_string(), "g-2".to_string()))
        );
        let pt_claims = TokenVerifier::with_api_key("key", "a-secret-of-at-least-32-characters!")
            .verify(&pt)
            .expect("verify");
        assert!(
            !pt_claims.video.room_join,
            "participant token must not grant media"
        );
        assert!(
            TokenVerifier::with_api_key("key", "wrong-secret-wrong-secret-wrong-secret")
                .verify(&jwt)
                .is_err()
        );
    }
}
