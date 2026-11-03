//! Integration use cases: key management (CLI) and meeting/embed-link creation (API).

use std::time::Duration;

use chrono::{DateTime, Utc};
use livekit_client::LiveKit;
use serde::Serialize;
use sqlx::PgPool;
use uuid::Uuid;

use super::{API_KEY_PREFIX, Integration, webhook};
use crate::{
    auth::{service::validate_display_name, token},
    error::{AppError, AppResult},
    meetings::service::{self as meetings, CreateInput, CreatedMeeting},
};

const DEFAULT_EMBED_TTL: u64 = 300;
const MAX_EMBED_TTL: u64 = 3600;

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct IntegrationRow {
    pub id: Uuid,
    pub name: String,
    pub webhook_url: Option<String>,
    pub created_at: DateTime<Utc>,
    pub revoked_at: Option<DateTime<Utc>>,
}

/// Shown once at creation; only hashes / the signing secret are stored.
pub struct NewIntegration {
    pub id: Uuid,
    pub api_key: String,
    pub webhook_secret: Option<String>,
}

pub async fn create(
    db: &PgPool,
    name: &str,
    webhook_url: Option<&str>,
) -> AppResult<NewIntegration> {
    let api_key = format!("{API_KEY_PREFIX}{}", token::generate());
    let webhook_secret = webhook_url.map(|_| format!("whsec_{}", token::generate()));
    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO integrations (name, api_key_hash, webhook_url, webhook_secret)
         VALUES ($1, $2, $3, $4) RETURNING id",
    )
    .bind(name)
    .bind(token::hash(&api_key))
    .bind(webhook_url)
    .bind(&webhook_secret)
    .fetch_one(db)
    .await
    .map_err(|e| match &e {
        sqlx::Error::Database(d) if d.is_unique_violation() => {
            AppError::Conflict("integration name taken".into())
        }
        sqlx::Error::Database(d) if d.is_check_violation() => AppError::Validation(
            "name must be 2-32 chars of a-z, 0-9, '-'; webhook url must be http(s)".into(),
        ),
        _ => e.into(),
    })?;
    Ok(NewIntegration {
        id,
        api_key,
        webhook_secret,
    })
}

pub async fn list(db: &PgPool) -> AppResult<Vec<IntegrationRow>> {
    Ok(sqlx::query_as(
        "SELECT id, name, webhook_url, created_at, revoked_at FROM integrations ORDER BY created_at",
    )
    .fetch_all(db)
    .await?)
}

/// Returns false if no active integration had that name.
pub async fn revoke(db: &PgPool, name: &str) -> AppResult<bool> {
    let r = sqlx::query(
        "UPDATE integrations SET revoked_at = now() WHERE name = $1 AND revoked_at IS NULL",
    )
    .bind(name)
    .execute(db)
    .await?;
    Ok(r.rows_affected() > 0)
}

pub struct MeetingInput {
    pub title: String,
    pub external_ref: Option<String>,
}

pub async fn create_meeting(
    db: &PgPool,
    i: &Integration,
    input: MeetingInput,
) -> AppResult<CreatedMeeting> {
    if input.external_ref.as_ref().is_some_and(|r| r.len() > 200) {
        return Err(AppError::Validation(
            "external_ref must be at most 200 bytes".into(),
        ));
    }
    meetings::create(
        db,
        None,
        CreateInput {
            title: input.title,
            password: None,
            lobby_enabled: false,
            integration_id: Some(i.id),
            external_ref: input.external_ref,
        },
    )
    .await
}

/// The meeting, only if it belongs to this integration (404 otherwise, so codes don't leak).
pub async fn owned_meeting(
    db: &PgPool,
    i: &Integration,
    code: &str,
) -> AppResult<meetings::Meeting> {
    let owned: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM meetings WHERE code = $1 AND integration_id = $2)",
    )
    .bind(code)
    .bind(i.id)
    .fetch_one(db)
    .await?;
    if !owned {
        return Err(AppError::NotFound);
    }
    meetings::by_code(db, code).await
}

pub struct TokenInput {
    pub external_id: String,
    pub display_name: String,
    pub host: bool,
    pub ttl_secs: Option<u64>,
}

#[derive(Debug, Serialize)]
pub struct EmbedLink {
    pub embed_url: String,
    pub embed_token: String,
    pub identity: String,
    pub expires_in: u64,
}

pub async fn embed_link(
    db: &PgPool,
    lk: &LiveKit,
    public_web_url: &str,
    i: &Integration,
    code: &str,
    input: TokenInput,
) -> AppResult<EmbedLink> {
    let meeting = owned_meeting(db, i, code).await?;
    if meeting.ended_at.is_some() {
        return Err(AppError::Gone("meeting has ended".into()));
    }
    let ok_id = (1..=64).contains(&input.external_id.len())
        && input
            .external_id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-');
    if !ok_id {
        return Err(AppError::Validation(
            "external_id must be 1-64 chars of A-Z, a-z, 0-9, '_', '-'".into(),
        ));
    }
    let name = validate_display_name(&input.display_name)?;
    let ttl = input.ttl_secs.unwrap_or(DEFAULT_EMBED_TTL);
    if !(30..=MAX_EMBED_TTL).contains(&ttl) {
        return Err(AppError::Validation(format!(
            "ttl must be 30-{MAX_EMBED_TTL} seconds"
        )));
    }
    // Same external_id → same identity, so LiveKit kicks an older session (one tab per user).
    let identity = webhook::identity_for(&i.name, &input.external_id);
    let role = if input.host { "host" } else { "participant" };
    let embed_token = lk.embed_token(
        &meeting.code,
        &identity,
        &name,
        role,
        Duration::from_secs(ttl),
    )?;
    Ok(EmbedLink {
        // Fragment, not query: never sent to servers or logged by proxies.
        embed_url: format!(
            "{}/embed/{}#t={embed_token}",
            public_web_url.trim_end_matches('/'),
            meeting.code
        ),
        embed_token,
        identity,
        expires_in: ttl,
    })
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct ParticipantRow {
    pub identity: String,
    pub display_name: String,
    pub role: String,
    pub status: String,
    pub joined_at: DateTime<Utc>,
    pub left_at: Option<DateTime<Utc>>,
}

/// Latest row per identity: who is (or was) in the call.
pub async fn participants(db: &PgPool, meeting_id: Uuid) -> AppResult<Vec<ParticipantRow>> {
    Ok(sqlx::query_as(
        "SELECT DISTINCT ON (identity) identity, display_name, role::text AS role, status::text AS status,
                joined_at, left_at
         FROM meeting_participants WHERE meeting_id = $1
         ORDER BY identity, joined_at DESC",
    )
    .bind(meeting_id)
    .fetch_all(db)
    .await?)
}

/// End a meeting as its integration (no host identity needed).
pub async fn end_meeting(db: &PgPool, lk: &LiveKit, i: &Integration, code: &str) -> AppResult<()> {
    let meeting = owned_meeting(db, i, code).await?;
    lk.delete_room(&meeting.code).await?;
    let mut tx = db.begin().await?;
    sqlx::query("UPDATE meetings SET ended_at = COALESCE(ended_at, now()), started_at = COALESCE(started_at, now()) WHERE id = $1")
        .bind(meeting.id)
        .execute(&mut *tx)
        .await?;
    sqlx::query(
        "UPDATE meeting_participants SET left_at = now() WHERE meeting_id = $1 AND left_at IS NULL",
    )
    .bind(meeting.id)
    .execute(&mut *tx)
    .await?;
    crate::audit::record(
        &mut *tx,
        None,
        "meeting.end",
        Some(("meeting", meeting.id)),
        serde_json::json!({ "integration": i.name }),
    )
    .await?;
    tx.commit().await?;
    Ok(())
}
