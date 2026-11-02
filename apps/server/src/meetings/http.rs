//! HTTP handlers for /api/meetings and the LiveKit webhook.
//! In-meeting actions authenticate with the participant token (`Authorization: Bearer`).

use axum::{
    Json, Router,
    extract::{Path, State},
    http::{HeaderMap, StatusCode, header},
    routing::{get, patch, post},
};
use serde::{Deserialize, Deserializer};

use super::service::{
    self, CreateInput, CreatedMeeting, JoinInput, JoinOutcome, LobbyEntry, Meeting, SettingsInput,
};
use crate::{
    auth::{CurrentUser, MaybeUser},
    error::{AppError, AppResult},
    participant::Participant,
    rate_limit,
    state::AppState,
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", post(create).get(list))
        .route("/{code}", get(show))
        .route("/{code}/join", post(join))
        .route("/{code}/settings", patch(update_settings))
        .route("/{code}/lobby", get(list_lobby))
        .route("/{code}/lobby/me", get(lobby_status))
        .route("/{code}/lobby/{identity}/admit", post(admit))
        .route("/{code}/lobby/{identity}/deny", post(deny))
        .route("/{code}/hand", post(hand))
        .route("/{code}/mute-all", post(mute_all))
        .route("/{code}/participants/{identity}/mute", post(mute))
        .route("/{code}/participants/{identity}/remove", post(remove))
        .route("/{code}/end", post(end))
}

pub fn webhook_router() -> Router<AppState> {
    Router::new().route("/", post(webhook))
}

#[derive(Deserialize)]
struct CreateReq {
    title: String,
    password: Option<String>,
    #[serde(default)]
    lobby_enabled: bool,
}

#[derive(Deserialize, Default)]
struct JoinReq {
    display_name: Option<String>,
    password: Option<String>,
    host_secret: Option<String>,
}

/// Distinguishes a missing field (leave unchanged) from `null` (clear).
fn double_option<'de, D: Deserializer<'de>>(d: D) -> Result<Option<Option<String>>, D::Error> {
    Option::<String>::deserialize(d).map(Some)
}

#[derive(Deserialize)]
struct SettingsReq {
    lobby_enabled: Option<bool>,
    #[serde(default, deserialize_with = "double_option")]
    password: Option<Option<String>>,
}

#[derive(Deserialize)]
struct HandReq {
    raised: bool,
}

async fn create(
    State(s): State<AppState>,
    MaybeUser(user): MaybeUser,
    Json(req): Json<CreateReq>,
) -> AppResult<(StatusCode, Json<CreatedMeeting>)> {
    let key = match &user {
        Some(u) => format!("meeting_create:{}", u.id),
        None if s.config.allow_guest_create => {
            // ponytail: one global bucket for anonymous creates; per-IP limits belong at the Traefik edge.
            "meeting_create:guest".to_string()
        }
        None => return Err(AppError::Unauthorized),
    };
    let limit = if user.is_some() { 30 } else { 120 };
    rate_limit::check(&s.redis, &key, limit, 3600).await?;
    let input = CreateInput {
        title: req.title,
        password: req.password,
        lobby_enabled: req.lobby_enabled,
    };
    Ok((
        StatusCode::CREATED,
        Json(service::create(&s.db, user.as_ref(), input).await?),
    ))
}

async fn list(
    State(s): State<AppState>,
    CurrentUser(user): CurrentUser,
) -> AppResult<Json<Vec<Meeting>>> {
    Ok(Json(service::list_for_user(&s.db, user.id).await?))
}

async fn show(State(s): State<AppState>, Path(code): Path<String>) -> AppResult<Json<Meeting>> {
    Ok(Json(service::by_code(&s.db, &code).await?))
}

async fn join(
    State(s): State<AppState>,
    MaybeUser(user): MaybeUser,
    Path(code): Path<String>,
    Json(req): Json<JoinReq>,
) -> AppResult<Json<JoinOutcome>> {
    // Throttles join-code guessing as well as token minting.
    rate_limit::check(&s.redis, &format!("join:{code}"), 300, 60).await?;
    if req.password.is_some() {
        // ponytail: per-meeting bucket also slows legit users during an attack; add per-IP at the edge.
        rate_limit::check(&s.redis, &format!("join_pw:{code}"), 20, 60).await?;
    }
    let input = JoinInput {
        guest_name: req.display_name.as_deref(),
        password: req.password.as_deref(),
        host_secret: req.host_secret.as_deref(),
    };
    let outcome = service::join(
        &s.db,
        &s.livekit,
        &s.config.livekit_public_url,
        &code,
        user.as_ref(),
        input,
    )
    .await?;
    Ok(Json(outcome))
}

async fn lobby_status(
    State(s): State<AppState>,
    p: Participant,
    Path(code): Path<String>,
) -> AppResult<Json<JoinOutcome>> {
    p.check_room(&code)?;
    Ok(Json(
        service::lobby_status(
            &s.db,
            &s.livekit,
            &s.config.livekit_public_url,
            &code,
            &p.identity,
        )
        .await?,
    ))
}

async fn list_lobby(
    State(s): State<AppState>,
    p: Participant,
    Path(code): Path<String>,
) -> AppResult<Json<Vec<LobbyEntry>>> {
    p.check_room(&code)?;
    Ok(Json(service::list_lobby(&s.db, &code, &p.identity).await?))
}

async fn admit(
    State(s): State<AppState>,
    p: Participant,
    Path((code, identity)): Path<(String, String)>,
) -> AppResult<StatusCode> {
    p.check_room(&code)?;
    service::decide_lobby(&s.db, &code, &p.identity, &identity, true).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn deny(
    State(s): State<AppState>,
    p: Participant,
    Path((code, identity)): Path<(String, String)>,
) -> AppResult<StatusCode> {
    p.check_room(&code)?;
    service::decide_lobby(&s.db, &code, &p.identity, &identity, false).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn update_settings(
    State(s): State<AppState>,
    p: Participant,
    Path(code): Path<String>,
    Json(req): Json<SettingsReq>,
) -> AppResult<Json<Meeting>> {
    p.check_room(&code)?;
    let input = SettingsInput {
        lobby_enabled: req.lobby_enabled,
        password: req.password,
    };
    Ok(Json(
        service::update_settings(&s.db, &code, &p.identity, input).await?,
    ))
}

async fn hand(
    State(s): State<AppState>,
    p: Participant,
    Path(code): Path<String>,
    Json(req): Json<HandReq>,
) -> AppResult<StatusCode> {
    p.check_room(&code)?;
    rate_limit::check(&s.redis, &format!("hand:{code}:{}", p.identity), 20, 60).await?;
    service::set_hand(&s.db, &s.livekit, &code, &p.identity, req.raised).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn mute(
    State(s): State<AppState>,
    p: Participant,
    Path((code, identity)): Path<(String, String)>,
) -> AppResult<StatusCode> {
    p.check_room(&code)?;
    service::mute_participant(&s.db, &s.livekit, &code, &p.identity, &identity).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn mute_all(
    State(s): State<AppState>,
    p: Participant,
    Path(code): Path<String>,
) -> AppResult<StatusCode> {
    p.check_room(&code)?;
    service::mute_all(&s.db, &s.livekit, &code, &p.identity).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn remove(
    State(s): State<AppState>,
    p: Participant,
    Path((code, identity)): Path<(String, String)>,
) -> AppResult<StatusCode> {
    p.check_room(&code)?;
    service::remove_participant(&s.db, &s.livekit, &code, &p.identity, &identity).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn end(
    State(s): State<AppState>,
    p: Participant,
    Path(code): Path<String>,
) -> AppResult<StatusCode> {
    p.check_room(&code)?;
    service::end(&s.db, &s.livekit, &code, &p.identity).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn webhook(
    State(s): State<AppState>,
    headers: HeaderMap,
    body: String,
) -> AppResult<StatusCode> {
    let auth = headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .ok_or(AppError::Unauthorized)?;
    let event = s.livekit.receive_webhook(&body, auth).map_err(|e| {
        tracing::warn!(error = %e, "rejected livekit webhook");
        AppError::Unauthorized
    })?;
    service::apply_webhook(&s.db, &event).await?;
    Ok(StatusCode::OK)
}
