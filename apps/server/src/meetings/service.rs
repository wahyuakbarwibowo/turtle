//! Meeting use cases, independent of HTTP. Room name in LiveKit == meeting code.

use std::time::Duration;

use argon2::password_hash::rand_core::{OsRng, RngCore};
use chrono::{DateTime, Utc};
use livekit_client::{JoinGrant, LiveKit, WebhookEvent};
use serde::Serialize;
use serde_json::json;
use sqlx::PgPool;
use uuid::Uuid;

use crate::{
    audit,
    auth::{User, service::validate_display_name},
    error::{AppError, AppResult},
};

/// Target capacity; LiveKit enforces it per room.
pub const MAX_PARTICIPANTS: u32 = 100;
/// Only used to connect; LiveKit refreshes tokens on its own for reconnects.
const JOIN_TOKEN_TTL: Duration = Duration::from_secs(15 * 60);
/// Authenticates in-meeting API calls (chat); outlives any realistic meeting.
const PARTICIPANT_TOKEN_TTL: Duration = Duration::from_secs(12 * 3600);

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct Meeting {
    pub id: Uuid,
    pub code: String,
    pub title: String,
    pub host_id: Uuid,
    pub started_at: Option<DateTime<Utc>>,
    pub ended_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Serialize)]
pub struct JoinInfo {
    pub token: String,
    /// Bearer token for our own in-meeting endpoints (chat). No media rights.
    pub participant_token: String,
    pub livekit_url: String,
    pub identity: String,
    pub role: &'static str,
    pub meeting: Meeting,
}

/// `abc-defg-hij`: 10 random lowercase letters (~47 bits), unguessable enough for invite links.
pub fn generate_code() -> String {
    let mut bytes = [0u8; 10];
    OsRng.fill_bytes(&mut bytes);
    let letters: String = bytes.iter().map(|b| (b'a' + b % 26) as char).collect();
    format!("{}-{}-{}", &letters[..3], &letters[3..7], &letters[7..])
}

fn validate_title(title: &str) -> AppResult<String> {
    let t = title.trim();
    if !(1..=120).contains(&t.chars().count()) {
        return Err(AppError::Validation(
            "title must be 1-120 characters".into(),
        ));
    }
    Ok(t.to_string())
}

pub async fn create(db: &PgPool, host: &User, title: &str) -> AppResult<Meeting> {
    let title = validate_title(title)?;
    let mut tx = db.begin().await?;
    let m: Meeting = sqlx::query_as(
        "INSERT INTO meetings (code, title, host_id) VALUES ($1, $2, $3)
         RETURNING id, code, title, host_id, started_at, ended_at, created_at",
    )
    .bind(generate_code())
    .bind(&title)
    .bind(host.id)
    .fetch_one(&mut *tx)
    .await?;
    audit::record(
        &mut *tx,
        Some(host.id),
        "meeting.create",
        Some(("meeting", m.id)),
        json!({}),
    )
    .await?;
    tx.commit().await?;
    Ok(m)
}

pub async fn by_code(db: &PgPool, code: &str) -> AppResult<Meeting> {
    sqlx::query_as(
        "SELECT id, code, title, host_id, started_at, ended_at, created_at FROM meetings WHERE code = $1",
    )
    .bind(code)
    .fetch_optional(db)
    .await?
    .ok_or(AppError::NotFound)
}

/// Meetings the user hosted or joined, newest first.
pub async fn list_for_user(db: &PgPool, user_id: Uuid) -> AppResult<Vec<Meeting>> {
    Ok(sqlx::query_as(
        "SELECT id, code, title, host_id, started_at, ended_at, created_at FROM meetings m
         WHERE m.host_id = $1
            OR EXISTS (SELECT 1 FROM meeting_participants p WHERE p.meeting_id = m.id AND p.user_id = $1)
         ORDER BY m.created_at DESC LIMIT 50",
    )
    .bind(user_id)
    .fetch_all(db)
    .await?)
}

pub async fn join(
    db: &PgPool,
    lk: &LiveKit,
    livekit_url: &str,
    code: &str,
    user: Option<&User>,
    guest_name: Option<&str>,
) -> AppResult<JoinInfo> {
    let meeting = by_code(db, code).await?;
    if meeting.ended_at.is_some() {
        return Err(AppError::Gone("meeting has ended".into()));
    }

    let (identity, name, role) = match user {
        Some(u) => {
            let removed: bool = sqlx::query_scalar(
                "SELECT EXISTS (SELECT 1 FROM meeting_participants
                 WHERE meeting_id = $1 AND user_id = $2 AND removed_at IS NOT NULL)",
            )
            .bind(meeting.id)
            .bind(u.id)
            .fetch_one(db)
            .await?;
            if removed {
                return Err(AppError::Forbidden);
            }
            let role = if u.id == meeting.host_id {
                "host"
            } else {
                "participant"
            };
            // Stable identity: LiveKit replaces a stale connection on rejoin/reconnect.
            (format!("u-{}", u.id), u.display_name.clone(), role)
        }
        // ponytail: removed guests can rejoin under a new identity; add host-approval lobby if abused.
        None => {
            let name = validate_display_name(guest_name.unwrap_or_default())?;
            (format!("g-{}", Uuid::new_v4()), name, "participant")
        }
    };

    lk.ensure_room(&meeting.code, MAX_PARTICIPANTS).await?;
    let token = lk.join_token(JoinGrant {
        room: &meeting.code,
        identity: &identity,
        name: &name,
        role,
        ttl: JOIN_TOKEN_TTL,
    })?;
    let participant_token =
        lk.participant_token(&meeting.code, &identity, PARTICIPANT_TOKEN_TTL)?;

    let mut tx = db.begin().await?;
    sqlx::query(
        "INSERT INTO meeting_participants (meeting_id, user_id, identity, display_name, role)
         VALUES ($1, $2, $3, $4, $5::participant_role)",
    )
    .bind(meeting.id)
    .bind(user.map(|u| u.id))
    .bind(&identity)
    .bind(&name)
    .bind(role)
    .execute(&mut *tx)
    .await?;
    let meeting: Meeting = sqlx::query_as(
        "UPDATE meetings SET started_at = COALESCE(started_at, now()) WHERE id = $1
         RETURNING id, code, title, host_id, started_at, ended_at, created_at",
    )
    .bind(meeting.id)
    .fetch_one(&mut *tx)
    .await?;
    audit::record(
        &mut *tx,
        user.map(|u| u.id),
        "meeting.join",
        Some(("meeting", meeting.id)),
        json!({ "identity": identity }),
    )
    .await?;
    tx.commit().await?;

    Ok(JoinInfo {
        token,
        participant_token,
        livekit_url: livekit_url.to_string(),
        identity,
        role,
        meeting,
    })
}

fn require_host(meeting: &Meeting, user: &User) -> AppResult<()> {
    if meeting.host_id != user.id && !user.is_admin() {
        return Err(AppError::Forbidden);
    }
    Ok(())
}

pub async fn remove_participant(
    db: &PgPool,
    lk: &LiveKit,
    code: &str,
    by: &User,
    identity: &str,
) -> AppResult<()> {
    let meeting = by_code(db, code).await?;
    require_host(&meeting, by)?;
    if identity == format!("u-{}", meeting.host_id) {
        return Err(AppError::Validation("host cannot remove themselves".into()));
    }
    lk.remove_participant(&meeting.code, identity).await?;

    let mut tx = db.begin().await?;
    sqlx::query(
        "UPDATE meeting_participants SET removed_at = now(), left_at = COALESCE(left_at, now())
         WHERE meeting_id = $1 AND identity = $2 AND removed_at IS NULL",
    )
    .bind(meeting.id)
    .bind(identity)
    .execute(&mut *tx)
    .await?;
    audit::record(
        &mut *tx,
        Some(by.id),
        "meeting.remove_participant",
        Some(("meeting", meeting.id)),
        json!({ "identity": identity }),
    )
    .await?;
    tx.commit().await?;
    Ok(())
}

pub async fn end(db: &PgPool, lk: &LiveKit, code: &str, by: &User) -> AppResult<()> {
    let meeting = by_code(db, code).await?;
    require_host(&meeting, by)?;
    if meeting.ended_at.is_some() {
        return Ok(());
    }
    // Disconnects everyone; already-gone rooms (empty timeout) are fine.
    lk.delete_room(&meeting.code).await?;
    let mut tx = db.begin().await?;
    sqlx::query("UPDATE meetings SET ended_at = now(), started_at = COALESCE(started_at, now()) WHERE id = $1")
        .bind(meeting.id)
        .execute(&mut *tx)
        .await?;
    sqlx::query(
        "UPDATE meeting_participants SET left_at = now() WHERE meeting_id = $1 AND left_at IS NULL",
    )
    .bind(meeting.id)
    .execute(&mut *tx)
    .await?;
    audit::record(
        &mut *tx,
        Some(by.id),
        "meeting.end",
        Some(("meeting", meeting.id)),
        json!({}),
    )
    .await?;
    tx.commit().await?;
    Ok(())
}

/// Keep `left_at` accurate from LiveKit's view of the room.
pub async fn apply_webhook(db: &PgPool, ev: &WebhookEvent) -> AppResult<()> {
    let Some(room) = ev.room.as_ref() else {
        return Ok(());
    };
    match (ev.event.as_str(), ev.participant.as_ref()) {
        // connection_aborted = joined signaling but never connected media; also gone.
        ("participant_left" | "participant_connection_aborted", Some(p)) => {
            sqlx::query(
                "UPDATE meeting_participants mp SET left_at = now() FROM meetings m
                 WHERE mp.meeting_id = m.id AND m.code = $1 AND mp.identity = $2 AND mp.left_at IS NULL",
            )
            .bind(&room.name)
            .bind(&p.identity)
            .execute(db)
            .await?;
        }
        ("room_finished", _) => {
            sqlx::query(
                "UPDATE meeting_participants mp SET left_at = now() FROM meetings m
                 WHERE mp.meeting_id = m.id AND m.code = $1 AND mp.left_at IS NULL",
            )
            .bind(&room.name)
            .execute(db)
            .await?;
        }
        _ => {}
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn code_format_matches_db_constraint() {
        for _ in 0..100 {
            let c = generate_code();
            assert_eq!(c.len(), 12);
            assert!(c.chars().all(|ch| ch.is_ascii_lowercase() || ch == '-'));
            assert_eq!(c.matches('-').count(), 2);
        }
        assert_ne!(generate_code(), generate_code());
    }

    #[test]
    fn title_bounds() {
        assert!(validate_title("  ").is_err());
        assert_eq!(validate_title(" Standup ").ok().as_deref(), Some("Standup"));
    }
}
