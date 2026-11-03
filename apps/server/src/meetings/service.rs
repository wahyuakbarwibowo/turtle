//! Meeting use cases, independent of HTTP. Room name in LiveKit == meeting code.
//!
//! In-meeting authority comes from the caller's latest `meeting_participants` row
//! (status + role), identified by the participant token. That covers guest hosts too.

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
    auth::{
        User, password,
        service::{hash_blocking, validate_display_name},
        token,
    },
    error::{AppError, AppResult},
};

/// Target capacity; LiveKit enforces it per room.
pub const MAX_PARTICIPANTS: u32 = 100;
/// Only used to connect; LiveKit refreshes tokens on its own for reconnects. Short, so a
/// removed participant can't reuse a saved join token to get back into the room.
const JOIN_TOKEN_TTL: Duration = Duration::from_secs(60);
/// Authenticates in-meeting API calls (chat, moderation, lobby polling).
const PARTICIPANT_TOKEN_TTL: Duration = Duration::from_secs(12 * 3600);
/// Lobby requests not polled within this window are treated as abandoned.
const LOBBY_STALE_SECS: f64 = 30.0;
/// Data topic telling hosts the lobby changed (payload is empty: no PII to non-hosts).
pub const LOBBY_TOPIC: &str = "turtlemeet.lobby";

/// Every meeting query aliases `meetings` as `m`. A macro so SQL stays `&'static str`.
macro_rules! meeting_cols {
    () => {
        "m.id, m.code, m.title, m.host_id, m.started_at, m.ended_at, m.created_at,
         m.lobby_enabled, (m.password_hash IS NOT NULL) AS has_password"
    };
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct Meeting {
    pub id: Uuid,
    pub code: String,
    pub title: String,
    /// `None` for meetings created without an account.
    pub host_id: Option<Uuid>,
    pub started_at: Option<DateTime<Utc>>,
    pub ended_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub lobby_enabled: bool,
    pub has_password: bool,
}

#[derive(Debug, Serialize)]
pub struct CreatedMeeting {
    #[serde(flatten)]
    pub meeting: Meeting,
    /// Only for guest-created meetings, shown once: proves host rights on join.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub host_secret: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct JoinInfo {
    pub token: String,
    /// Bearer token for our own in-meeting endpoints. No media rights.
    pub participant_token: String,
    pub livekit_url: String,
    pub identity: String,
    pub role: &'static str,
    pub meeting: Meeting,
}

#[derive(Debug, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum JoinOutcome {
    Admitted(JoinInfo),
    /// In the lobby; poll with `participant_token` until admitted or denied.
    Waiting {
        identity: String,
        participant_token: String,
    },
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct LobbyEntry {
    pub identity: String,
    pub display_name: String,
    pub requested_at: DateTime<Utc>,
}

pub struct CreateInput {
    pub title: String,
    pub password: Option<String>,
    pub lobby_enabled: bool,
    /// Set when created through the integration API; such meetings have no account host.
    pub integration_id: Option<Uuid>,
    pub external_ref: Option<String>,
}

#[derive(Default)]
pub struct JoinInput<'a> {
    pub guest_name: Option<&'a str>,
    pub password: Option<&'a str>,
    pub host_secret: Option<&'a str>,
    /// Verified embed token minted by an integration: identity, name and role come from it,
    /// and it skips the password and lobby (the integrator already authorized the user).
    pub embed: Option<livekit_client::AppToken>,
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

async fn hash_meeting_password(pw: Option<String>) -> AppResult<Option<String>> {
    match pw {
        None => Ok(None),
        Some(pw) if (6..=128).contains(&pw.chars().count()) => Ok(Some(hash_blocking(pw).await?)),
        Some(_) => Err(AppError::Validation(
            "password must be 6-128 characters".into(),
        )),
    }
}

pub async fn create(
    db: &PgPool,
    host: Option<&User>,
    input: CreateInput,
) -> AppResult<CreatedMeeting> {
    let title = validate_title(&input.title)?;
    let password_hash = hash_meeting_password(input.password).await?;
    let host_secret = (host.is_none() && input.integration_id.is_none()).then(token::generate);

    let mut tx = db.begin().await?;
    let meeting: Meeting = sqlx::query_as(concat!(
        "INSERT INTO meetings AS m (code, title, host_id, host_secret_hash, password_hash, lobby_enabled,
                                    integration_id, external_ref)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8) RETURNING ",
        meeting_cols!()
    ))
    .bind(generate_code())
    .bind(&title)
    .bind(host.map(|u| u.id))
    .bind(host_secret.as_deref().map(token::hash))
    .bind(password_hash)
    .bind(input.lobby_enabled)
    .bind(input.integration_id)
    .bind(input.external_ref)
    .fetch_one(&mut *tx)
    .await?;
    audit::record(
        &mut *tx,
        host.map(|u| u.id),
        "meeting.create",
        Some(("meeting", meeting.id)),
        json!({}),
    )
    .await?;
    tx.commit().await?;
    Ok(CreatedMeeting {
        meeting,
        host_secret,
    })
}

pub async fn by_code(db: &PgPool, code: &str) -> AppResult<Meeting> {
    sqlx::query_as(concat!(
        "SELECT ",
        meeting_cols!(),
        " FROM meetings m WHERE m.code = $1"
    ))
    .bind(code)
    .fetch_optional(db)
    .await?
    .ok_or(AppError::NotFound)
}

/// Meetings the user hosted or joined, newest first.
pub async fn list_for_user(db: &PgPool, user_id: Uuid) -> AppResult<Vec<Meeting>> {
    Ok(sqlx::query_as(concat!(
        "SELECT ",
        meeting_cols!(),
        " FROM meetings m
         WHERE m.host_id = $1
            OR EXISTS (SELECT 1 FROM meeting_participants p WHERE p.meeting_id = m.id AND p.user_id = $1)
         ORDER BY m.created_at DESC LIMIT 50"
    ))
    .bind(user_id)
    .fetch_all(db)
    .await?)
}

/// Mint LiveKit + participant tokens for an admitted participant.
async fn admit_tokens(
    lk: &LiveKit,
    livekit_url: &str,
    meeting: Meeting,
    identity: String,
    name: &str,
    role: &'static str,
) -> AppResult<JoinInfo> {
    lk.ensure_room(&meeting.code, MAX_PARTICIPANTS).await?;
    let token = lk.join_token(JoinGrant {
        room: &meeting.code,
        identity: &identity,
        name,
        role,
        ttl: JOIN_TOKEN_TTL,
    })?;
    let participant_token =
        lk.participant_token(&meeting.code, &identity, PARTICIPANT_TOKEN_TTL)?;
    Ok(JoinInfo {
        token,
        participant_token,
        livekit_url: livekit_url.to_string(),
        identity,
        role,
        meeting,
    })
}

/// Tell connected hosts to refresh their lobby list. Best effort: the room may not exist yet.
async fn notify_lobby(lk: &LiveKit, code: &str) {
    if let Err(e) = lk.broadcast(code, LOBBY_TOPIC, b"{}".to_vec()).await {
        tracing::debug!(error = %e, code, "lobby notify skipped");
    }
}

pub async fn join(
    db: &PgPool,
    lk: &LiveKit,
    livekit_url: &str,
    code: &str,
    user: Option<&User>,
    input: JoinInput<'_>,
) -> AppResult<JoinOutcome> {
    let meeting = by_code(db, code).await?;
    if meeting.ended_at.is_some() {
        return Err(AppError::Gone("meeting has ended".into()));
    }
    let (password_hash, host_secret_hash, is_integration): (Option<String>, Option<Vec<u8>>, bool) =
        sqlx::query_as(
            "SELECT password_hash, host_secret_hash, integration_id IS NOT NULL FROM meetings WHERE id = $1",
        )
        .bind(meeting.id)
        .fetch_one(db)
        .await?;

    // Integration meetings are only reachable through the integrator's embed links, so a
    // leaked meeting code alone doesn't let anyone in.
    if is_integration != input.embed.is_some() {
        return Err(AppError::Forbidden);
    }
    if let Some(e) = &input.embed {
        if e.room != meeting.code {
            return Err(AppError::Forbidden);
        }
        let removed: bool = sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM meeting_participants
                            WHERE meeting_id = $1 AND identity = $2 AND removed_at IS NOT NULL)",
        )
        .bind(meeting.id)
        .bind(&e.identity)
        .fetch_one(db)
        .await?;
        if removed {
            return Err(AppError::Forbidden);
        }
    }

    let is_host = match &input.embed {
        Some(e) => e.attributes.get("role").is_some_and(|r| r == "host"),
        None => {
            user.is_some_and(|u| Some(u.id) == meeting.host_id)
                || matches!((input.host_secret, &host_secret_hash), (Some(s), Some(h)) if token::hash(s) == *h)
        }
    };

    let mut previously_admitted = input.embed.is_some();
    if !is_host && input.embed.is_none() {
        if let Some(u) = user {
            let (removed, admitted): (bool, bool) = sqlx::query_as(
                "SELECT COALESCE(bool_or(removed_at IS NOT NULL), false),
                        COALESCE(bool_or(status = 'admitted'), false)
                 FROM meeting_participants WHERE meeting_id = $1 AND user_id = $2",
            )
            .bind(meeting.id)
            .bind(u.id)
            .fetch_one(db)
            .await?;
            if removed {
                return Err(AppError::Forbidden);
            }
            previously_admitted = admitted;
        }
        if let Some(hash) = password_hash {
            let given = input.password.ok_or(AppError::PasswordRequired)?.to_owned();
            let ok = password::blocking(move || password::verify(&given, Some(&hash))).await?;
            if !ok {
                return Err(AppError::InvalidPassword);
            }
        }
    }

    // Signed-in users keep a stable identity so LiveKit replaces stale connections on rejoin.
    let (identity, name) = match (&input.embed, user) {
        (Some(e), _) => (e.identity.clone(), e.name.clone()),
        (None, Some(u)) => (format!("u-{}", u.id), u.display_name.clone()),
        (None, None) => (
            format!("g-{}", Uuid::new_v4()),
            validate_display_name(input.guest_name.unwrap_or_default())?,
        ),
    };
    let role = if is_host { "host" } else { "participant" };
    let waiting = !is_host && meeting.lobby_enabled && !previously_admitted;

    let mut tx = db.begin().await?;
    sqlx::query(
        "INSERT INTO meeting_participants (meeting_id, user_id, identity, display_name, role, status)
         VALUES ($1, $2, $3, $4, $5::participant_role, $6::participant_status)",
    )
    .bind(meeting.id)
    .bind(user.filter(|_| input.embed.is_none()).map(|u| u.id))
    .bind(&identity)
    .bind(&name)
    .bind(role)
    .bind(if waiting { "waiting" } else { "admitted" })
    .execute(&mut *tx)
    .await?;
    let action = if waiting {
        "meeting.lobby_request"
    } else {
        "meeting.join"
    };
    audit::record(
        &mut *tx,
        user.map(|u| u.id),
        action,
        Some(("meeting", meeting.id)),
        json!({ "identity": identity }),
    )
    .await?;
    tx.commit().await?;

    if waiting {
        notify_lobby(lk, &meeting.code).await;
        let participant_token =
            lk.participant_token(&meeting.code, &identity, PARTICIPANT_TOKEN_TTL)?;
        return Ok(JoinOutcome::Waiting {
            identity,
            participant_token,
        });
    }
    let meeting: Meeting = sqlx::query_as(concat!(
        "UPDATE meetings m SET started_at = COALESCE(started_at, now()) WHERE m.id = $1 RETURNING ",
        meeting_cols!()
    ))
    .bind(meeting.id)
    .fetch_one(db)
    .await?;
    Ok(JoinOutcome::Admitted(
        admit_tokens(lk, livekit_url, meeting, identity, &name, role).await?,
    ))
}

/// Polled by a waiting participant. Also a heartbeat, so abandoned requests drop off the host's list.
pub async fn lobby_status(
    db: &PgPool,
    lk: &LiveKit,
    livekit_url: &str,
    code: &str,
    identity: &str,
) -> AppResult<JoinOutcome> {
    let meeting = by_code(db, code).await?;
    if meeting.ended_at.is_some() {
        return Err(AppError::Gone("meeting has ended".into()));
    }
    let row: Option<(Uuid, String, String, bool)> = sqlx::query_as(
        "UPDATE meeting_participants SET joined_at = CASE WHEN status = 'waiting' THEN now() ELSE joined_at END
         WHERE id = (SELECT id FROM meeting_participants WHERE meeting_id = $1 AND identity = $2
                     ORDER BY joined_at DESC LIMIT 1)
         RETURNING id, status::text, display_name, removed_at IS NOT NULL",
    )
    .bind(meeting.id)
    .bind(identity)
    .fetch_optional(db)
    .await?;
    match row {
        Some((_, status, name, false)) if status == "admitted" => Ok(JoinOutcome::Admitted(
            admit_tokens(
                lk,
                livekit_url,
                meeting,
                identity.to_string(),
                &name,
                "participant",
            )
            .await?,
        )),
        Some((_, status, _, false)) if status == "waiting" => Ok(JoinOutcome::Waiting {
            identity: identity.to_string(),
            participant_token: lk.participant_token(code, identity, PARTICIPANT_TOKEN_TTL)?,
        }),
        Some(_) => Err(AppError::Forbidden), // denied or removed
        None => Err(AppError::NotFound),
    }
}

/// Resolves the caller as an admitted, not-removed host of a live meeting.
/// Returns the meeting and the host's user id (None for guest hosts) for audit logs.
async fn require_host(
    db: &PgPool,
    code: &str,
    identity: &str,
) -> AppResult<(Meeting, Option<Uuid>)> {
    let meeting = by_code(db, code).await?;
    if meeting.ended_at.is_some() {
        return Err(AppError::Gone("meeting has ended".into()));
    }
    let row: Option<(String, Option<Uuid>)> = sqlx::query_as(
        "SELECT role::text, user_id FROM meeting_participants
         WHERE meeting_id = $1 AND identity = $2 AND status = 'admitted' AND removed_at IS NULL
         ORDER BY joined_at DESC LIMIT 1",
    )
    .bind(meeting.id)
    .bind(identity)
    .fetch_optional(db)
    .await?;
    match row {
        Some((role, user_id)) if role == "host" => Ok((meeting, user_id)),
        _ => Err(AppError::Forbidden),
    }
}

/// The target must be in the meeting and not a host (hosts can't moderate each other).
async fn require_moderatable(db: &PgPool, meeting: &Meeting, identity: &str) -> AppResult<()> {
    let role: Option<String> = sqlx::query_scalar(
        "SELECT role::text FROM meeting_participants WHERE meeting_id = $1 AND identity = $2
         ORDER BY joined_at DESC LIMIT 1",
    )
    .bind(meeting.id)
    .bind(identity)
    .fetch_optional(db)
    .await?;
    match role.as_deref() {
        None => Err(AppError::NotFound),
        Some("host") => Err(AppError::Validation("hosts cannot be moderated".into())),
        Some(_) => Ok(()),
    }
}

pub async fn list_lobby(
    db: &PgPool,
    code: &str,
    host_identity: &str,
) -> AppResult<Vec<LobbyEntry>> {
    let (meeting, _) = require_host(db, code, host_identity).await?;
    Ok(sqlx::query_as(
        "SELECT identity, display_name, joined_at AS requested_at FROM meeting_participants
         WHERE meeting_id = $1 AND status = 'waiting'
           AND joined_at > now() - make_interval(secs => $2)
         ORDER BY joined_at",
    )
    .bind(meeting.id)
    .bind(LOBBY_STALE_SECS)
    .fetch_all(db)
    .await?)
}

pub async fn decide_lobby(
    db: &PgPool,
    code: &str,
    host_identity: &str,
    identity: &str,
    admit: bool,
) -> AppResult<()> {
    let (meeting, actor) = require_host(db, code, host_identity).await?;
    let mut tx = db.begin().await?;
    let updated = sqlx::query(
        "UPDATE meeting_participants SET status = $3::participant_status, joined_at = now()
         WHERE meeting_id = $1 AND identity = $2 AND status = 'waiting'",
    )
    .bind(meeting.id)
    .bind(identity)
    .bind(if admit { "admitted" } else { "denied" })
    .execute(&mut *tx)
    .await?
    .rows_affected();
    if updated == 0 {
        return Err(AppError::NotFound);
    }
    let action = if admit {
        "meeting.lobby_admit"
    } else {
        "meeting.lobby_deny"
    };
    audit::record(
        &mut *tx,
        actor,
        action,
        Some(("meeting", meeting.id)),
        json!({ "identity": identity }),
    )
    .await?;
    tx.commit().await?;
    Ok(())
}

pub struct SettingsInput {
    pub lobby_enabled: Option<bool>,
    /// `Some(None)` clears the password.
    pub password: Option<Option<String>>,
}

pub async fn update_settings(
    db: &PgPool,
    code: &str,
    host_identity: &str,
    input: SettingsInput,
) -> AppResult<Meeting> {
    let (meeting, actor) = require_host(db, code, host_identity).await?;
    let (set_password, password_hash) = match input.password {
        None => (false, None),
        Some(pw) => (true, hash_meeting_password(pw).await?),
    };
    let mut tx = db.begin().await?;
    let updated: Meeting = sqlx::query_as(concat!(
        "UPDATE meetings m SET
           lobby_enabled = COALESCE($2, m.lobby_enabled),
           password_hash = CASE WHEN $3 THEN $4 ELSE m.password_hash END
         WHERE m.id = $1 RETURNING ",
        meeting_cols!()
    ))
    .bind(meeting.id)
    .bind(input.lobby_enabled)
    .bind(set_password)
    .bind(password_hash)
    .fetch_one(&mut *tx)
    .await?;
    audit::record(
        &mut *tx,
        actor,
        "meeting.update_settings",
        Some(("meeting", meeting.id)),
        json!({ "lobby_enabled": updated.lobby_enabled, "has_password": updated.has_password }),
    )
    .await?;
    tx.commit().await?;
    Ok(updated)
}

pub async fn remove_participant(
    db: &PgPool,
    lk: &LiveKit,
    code: &str,
    host_identity: &str,
    identity: &str,
) -> AppResult<()> {
    let (meeting, actor) = require_host(db, code, host_identity).await?;
    require_moderatable(db, &meeting, identity).await?;
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
        actor,
        "meeting.remove_participant",
        Some(("meeting", meeting.id)),
        json!({ "identity": identity }),
    )
    .await?;
    tx.commit().await?;
    Ok(())
}

pub async fn mute_participant(
    db: &PgPool,
    lk: &LiveKit,
    code: &str,
    host_identity: &str,
    identity: &str,
) -> AppResult<()> {
    let (meeting, actor) = require_host(db, code, host_identity).await?;
    require_moderatable(db, &meeting, identity).await?;
    lk.mute_microphone(&meeting.code, identity).await?;
    audit::record(
        db,
        actor,
        "meeting.mute",
        Some(("meeting", meeting.id)),
        json!({ "identity": identity }),
    )
    .await?;
    Ok(())
}

pub async fn mute_all(db: &PgPool, lk: &LiveKit, code: &str, host_identity: &str) -> AppResult<()> {
    let (meeting, actor) = require_host(db, code, host_identity).await?;
    lk.mute_all_microphones(&meeting.code, host_identity)
        .await?;
    audit::record(
        db,
        actor,
        "meeting.mute_all",
        Some(("meeting", meeting.id)),
        json!({}),
    )
    .await?;
    Ok(())
}

/// Raised hand is a server-written attribute so it can't be forged by other clients.
pub async fn set_hand(
    db: &PgPool,
    lk: &LiveKit,
    code: &str,
    identity: &str,
    raised: bool,
) -> AppResult<()> {
    let admitted: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM meeting_participants p JOIN meetings m ON m.id = p.meeting_id
         WHERE m.code = $1 AND p.identity = $2 AND p.status = 'admitted'
           AND p.removed_at IS NULL AND m.ended_at IS NULL)",
    )
    .bind(code)
    .bind(identity)
    .fetch_one(db)
    .await?;
    if !admitted {
        return Err(AppError::Forbidden);
    }
    // LiveKit deletes an attribute set to the empty string.
    let value = if raised { "1" } else { "" };
    lk.set_attributes(code, identity, [("hand".to_string(), value.to_string())])
        .await?;
    Ok(())
}

pub async fn end(db: &PgPool, lk: &LiveKit, code: &str, host_identity: &str) -> AppResult<()> {
    let (meeting, actor) = require_host(db, code, host_identity).await?;
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
        actor,
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
                 WHERE mp.meeting_id = m.id AND m.code = $1 AND mp.identity = $2
                   AND mp.status = 'admitted' AND mp.left_at IS NULL",
            )
            .bind(&room.name)
            .bind(&p.identity)
            .execute(db)
            .await?;
        }
        ("room_finished", _) => {
            sqlx::query(
                "UPDATE meeting_participants mp SET left_at = now() FROM meetings m
                 WHERE mp.meeting_id = m.id AND m.code = $1 AND mp.status = 'admitted' AND mp.left_at IS NULL",
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

    #[test]
    fn join_outcome_is_tagged_by_status() {
        let w = JoinOutcome::Waiting {
            identity: "g-1".into(),
            participant_token: "t".into(),
        };
        let v = serde_json::to_value(&w).expect("json");
        assert_eq!(v["status"], "waiting");
        assert_eq!(v["identity"], "g-1");
    }
}
