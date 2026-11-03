//! Signed event delivery to an integration's webhook.
//!
//! Signature: `X-TurtleMeet-Signature: v1=<hex HMAC-SHA256(secret, "<timestamp>.<body>")>`
//! with `X-TurtleMeet-Timestamp` (unix seconds). Receivers should reject stale timestamps
//! (e.g. > 5 min) to stop replays, and dedupe on the event `id`.

use std::time::Duration;

use chrono::{DateTime, Utc};
use hmac::{Hmac, Mac};
use serde::Serialize;
use sha2::Sha256;
use sqlx::PgPool;
use uuid::Uuid;

use crate::auth::token;

#[derive(Debug, Clone, Serialize)]
pub struct WebhookParticipant {
    pub identity: String,
    /// The integrator's own user id (what they passed as `external_id`).
    #[serde(default)]
    pub external_id: Option<String>,
    pub name: String,
    pub role: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct WebhookEvent {
    pub id: Uuid,
    /// `participant_joined` | `participant_left` | `meeting_ended`
    #[serde(rename = "type")]
    pub kind: &'static str,
    pub created_at: DateTime<Utc>,
    pub meeting: WebhookMeeting,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub participant: Option<WebhookParticipant>,
}

#[derive(Debug, Clone, Serialize)]
pub struct WebhookMeeting {
    pub code: String,
    pub external_ref: Option<String>,
}

pub fn sign(secret: &str, timestamp: i64, body: &[u8]) -> String {
    // HMAC accepts keys of any length, so this can't fail.
    let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes()).expect("hmac key");
    mac.update(timestamp.to_string().as_bytes());
    mac.update(b".");
    mac.update(body);
    format!("v1={}", token::hex(&mac.finalize().into_bytes()))
}

const RETRY_DELAYS: [Duration; 3] = [
    Duration::from_secs(1),
    Duration::from_secs(5),
    Duration::from_secs(25),
];

/// (webhook_url, webhook_secret, meeting external_ref, integration name)
type Target = (String, String, Option<String>, String);

/// Fire-and-forget delivery for a meeting's integration, if it has a webhook.
// ponytail: in-process retries only; events are lost if the server restarts mid-retry.
// Move to a durable job queue (Apalis) when integrators need guaranteed delivery.
pub async fn dispatch(
    db: &PgPool,
    http: &reqwest::Client,
    meeting_code: &str,
    kind: &'static str,
    mut participant: Option<WebhookParticipant>,
) {
    let target: Result<Option<Target>, _> = sqlx::query_as(
        "SELECT i.webhook_url, i.webhook_secret, m.external_ref, i.name
         FROM meetings m JOIN integrations i ON i.id = m.integration_id
         WHERE m.code = $1 AND i.revoked_at IS NULL AND i.webhook_url IS NOT NULL",
    )
    .bind(meeting_code)
    .fetch_optional(db)
    .await;
    let (url, secret, external_ref, integration) = match target {
        Ok(Some(t)) => t,
        Ok(None) => return,
        Err(e) => return tracing::warn!(error = %e, "webhook lookup failed"),
    };
    if let Some(p) = participant.as_mut() {
        p.external_id = external_id(&integration, &p.identity).map(str::to_string);
    }
    let event = WebhookEvent {
        id: Uuid::new_v4(),
        kind,
        created_at: Utc::now(),
        meeting: WebhookMeeting {
            code: meeting_code.to_string(),
            external_ref,
        },
        participant,
    };
    let http = http.clone();
    tokio::spawn(async move { deliver(&http, &url, &secret, &event).await });
}

async fn deliver(http: &reqwest::Client, url: &str, secret: &str, event: &WebhookEvent) {
    let Ok(body) = serde_json::to_vec(event) else {
        return;
    };
    for (attempt, delay) in std::iter::once(Duration::ZERO)
        .chain(RETRY_DELAYS)
        .enumerate()
    {
        tokio::time::sleep(delay).await;
        let ts = Utc::now().timestamp();
        let res = http
            .post(url)
            .header("content-type", "application/json")
            .header("x-turtlemeet-event", event.kind)
            .header("x-turtlemeet-timestamp", ts)
            .header("x-turtlemeet-signature", sign(secret, ts, &body))
            .timeout(Duration::from_secs(10))
            .body(body.clone())
            .send()
            .await;
        match res {
            Ok(r) if r.status().is_success() => return,
            Ok(r) => tracing::warn!(url, status = %r.status(), attempt, "webhook rejected"),
            Err(e) => tracing::warn!(url, error = %e, attempt, "webhook failed"),
        }
    }
    tracing::error!(url, event = event.kind, id = %event.id, "webhook gave up");
}

/// Embed identities are `x-<integration>-<external_id>`; anyone else (a host who joined
/// from the TurtleMeet UI) has no external id.
pub fn identity_for(integration: &str, external_id: &str) -> String {
    format!("x-{integration}-{external_id}")
}

fn external_id<'a>(integration: &str, identity: &'a str) -> Option<&'a str> {
    identity
        .strip_prefix("x-")?
        .strip_prefix(integration)?
        .strip_prefix('-')
}

#[cfg(test)]
mod tests {
    #[test]
    fn signature_is_stable_hmac_sha256() {
        // Reference: printf '1700000000.{"a":1}' | openssl dgst -sha256 -hmac secret
        let sig = super::sign("secret", 1_700_000_000, br#"{"a":1}"#);
        assert_eq!(
            sig,
            "v1=49f24e537407743fa4a0242bb63b94b9a47ee99cbbe071ccd8a22550ae411686"
        );
        assert_ne!(sig, super::sign("other", 1_700_000_000, br#"{"a":1}"#));
        assert_ne!(sig, super::sign("secret", 1_700_000_001, br#"{"a":1}"#));
    }

    #[test]
    fn external_id_round_trips() {
        let id = super::identity_for("qnext", "user-42");
        assert_eq!(super::external_id("qnext", &id), Some("user-42"));
        assert_eq!(super::external_id("qn", &id), None);
        assert_eq!(super::external_id("qnext", "u-123"), None);
    }
}
