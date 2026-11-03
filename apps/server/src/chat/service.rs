use chrono::{DateTime, Utc};
use livekit_client::LiveKit;
use serde::Serialize;
use sqlx::PgPool;
use uuid::Uuid;

use super::CHAT_TOPIC;
use crate::error::{AppError, AppResult};

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct ChatMessage {
    pub id: Uuid,
    pub identity: Option<String>,
    pub display_name: Option<String>,
    pub body: String,
    pub created_at: DateTime<Utc>,
}

const HISTORY_LIMIT: i64 = 200;

fn validate_body(body: &str) -> AppResult<&str> {
    let body = body.trim();
    if !(1..=4000).contains(&body.chars().count()) {
        return Err(AppError::Validation(
            "message must be 1-4000 characters".into(),
        ));
    }
    Ok(body)
}

/// Most recent messages, oldest first.
pub async fn history(db: &PgPool, code: &str) -> AppResult<Vec<ChatMessage>> {
    Ok(sqlx::query_as(
        "SELECT * FROM (
           SELECT c.id, p.identity, p.display_name, c.body, c.created_at
           FROM chat_messages c
           JOIN meetings m ON m.id = c.meeting_id
           LEFT JOIN meeting_participants p ON p.id = c.participant_id
           WHERE m.code = $1
           ORDER BY c.created_at DESC
           LIMIT $2
         ) recent ORDER BY created_at",
    )
    .bind(code)
    .bind(HISTORY_LIMIT)
    .fetch_all(db)
    .await?)
}

pub async fn send(
    db: &PgPool,
    lk: &LiveKit,
    code: &str,
    identity: &str,
    body: &str,
) -> AppResult<ChatMessage> {
    let body = validate_body(body)?;
    // Latest join row for this identity; removed participants and ended meetings can't post.
    let sender: Option<(Uuid, Uuid, String)> = sqlx::query_as(
        "SELECT p.id, p.meeting_id, p.display_name FROM meeting_participants p
         JOIN meetings m ON m.id = p.meeting_id
         WHERE m.code = $1 AND p.identity = $2 AND p.status = 'admitted'
           AND p.removed_at IS NULL AND m.ended_at IS NULL
         ORDER BY p.joined_at DESC LIMIT 1",
    )
    .bind(code)
    .bind(identity)
    .fetch_optional(db)
    .await?;
    let (participant_id, meeting_id, display_name) = sender.ok_or(AppError::Forbidden)?;

    let (id, created_at): (Uuid, DateTime<Utc>) = sqlx::query_as(
        "INSERT INTO chat_messages (meeting_id, participant_id, body) VALUES ($1, $2, $3)
         RETURNING id, created_at",
    )
    .bind(meeting_id)
    .bind(participant_id)
    .bind(body)
    .fetch_one(db)
    .await?;

    let msg = ChatMessage {
        id,
        identity: Some(identity.to_string()),
        display_name: Some(display_name),
        body: body.to_string(),
        created_at,
    };
    let payload = serde_json::to_vec(&msg).map_err(|e| AppError::Internal(e.to_string()))?;
    // Stored already; if live delivery fails, others still see it via history on reload.
    if let Err(e) = lk.broadcast(code, CHAT_TOPIC, payload).await {
        tracing::warn!(error = %e, code, "chat broadcast failed");
    }
    Ok(msg)
}

#[cfg(test)]
mod tests {
    #[test]
    fn body_bounds() {
        assert!(super::validate_body("   ").is_err());
        assert_eq!(super::validate_body(" hi ").ok(), Some("hi"));
        assert!(super::validate_body(&"x".repeat(4001)).is_err());
    }
}
