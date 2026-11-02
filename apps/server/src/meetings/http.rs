//! HTTP handlers for /api/meetings and the LiveKit webhook.

use axum::{
    Json, Router,
    extract::{Path, State},
    http::{HeaderMap, StatusCode, header},
    routing::{get, post},
};
use serde::Deserialize;

use super::service::{self, JoinInfo, Meeting};
use crate::{
    auth::{CurrentUser, MaybeUser},
    error::{AppError, AppResult},
    rate_limit,
    state::AppState,
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", post(create).get(list))
        .route("/{code}", get(show))
        .route("/{code}/join", post(join))
        .route("/{code}/end", post(end))
        .route("/{code}/participants/{identity}/remove", post(remove))
}

pub fn webhook_router() -> Router<AppState> {
    Router::new().route("/", post(webhook))
}

#[derive(Deserialize)]
struct CreateReq {
    title: String,
}

#[derive(Deserialize, Default)]
struct JoinReq {
    display_name: Option<String>,
}

async fn create(
    State(s): State<AppState>,
    CurrentUser(user): CurrentUser,
    Json(req): Json<CreateReq>,
) -> AppResult<(StatusCode, Json<Meeting>)> {
    rate_limit::check(&s.redis, &format!("meeting_create:{}", user.id), 30, 3600).await?;
    Ok((
        StatusCode::CREATED,
        Json(service::create(&s.db, &user, &req.title).await?),
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
) -> AppResult<Json<JoinInfo>> {
    // Throttles join-code guessing as well as token minting.
    rate_limit::check(&s.redis, &format!("join:{code}"), 300, 60).await?;
    let info = service::join(
        &s.db,
        &s.livekit,
        &s.config.livekit_public_url,
        &code,
        user.as_ref(),
        req.display_name.as_deref(),
    )
    .await?;
    Ok(Json(info))
}

async fn end(
    State(s): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path(code): Path<String>,
) -> AppResult<StatusCode> {
    service::end(&s.db, &s.livekit, &code, &user).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn remove(
    State(s): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path((code, identity)): Path<(String, String)>,
) -> AppResult<StatusCode> {
    service::remove_participant(&s.db, &s.livekit, &code, &user, &identity).await?;
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
