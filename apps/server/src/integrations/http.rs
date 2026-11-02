//! `/api/integrations/*`, authenticated by `Authorization: Bearer tmk_…`.

use axum::{
    Json, Router,
    extract::{Path, State},
    http::StatusCode,
    routing::{get, post},
};
use serde::Deserialize;
use serde_json::{Value, json};

use super::{
    Integration,
    service::{self, EmbedLink, MeetingInput, TokenInput},
    webhook,
};
use crate::{error::AppResult, meetings::service::CreatedMeeting, rate_limit, state::AppState};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/meetings", post(create_meeting))
        .route("/meetings/{code}", get(show_meeting))
        .route("/meetings/{code}/tokens", post(mint_token))
        .route("/meetings/{code}/end", post(end_meeting))
}

#[derive(Deserialize)]
struct CreateReq {
    title: String,
    external_ref: Option<String>,
}

async fn create_meeting(
    State(s): State<AppState>,
    i: Integration,
    Json(req): Json<CreateReq>,
) -> AppResult<(StatusCode, Json<CreatedMeeting>)> {
    rate_limit::check(&s.redis, &format!("int_create:{}", i.id), 600, 60).await?;
    let input = MeetingInput {
        title: req.title,
        external_ref: req.external_ref,
    };
    Ok((
        StatusCode::CREATED,
        Json(service::create_meeting(&s.db, &i, input).await?),
    ))
}

async fn show_meeting(
    State(s): State<AppState>,
    i: Integration,
    Path(code): Path<String>,
) -> AppResult<Json<Value>> {
    let meeting = service::owned_meeting(&s.db, &i, &code).await?;
    let participants = service::participants(&s.db, meeting.id).await?;
    Ok(Json(
        json!({ "meeting": meeting, "participants": participants }),
    ))
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
enum Role {
    Host,
    Participant,
}

#[derive(Deserialize)]
struct TokenReq {
    external_id: String,
    display_name: String,
    role: Role,
    ttl: Option<u64>,
}

async fn mint_token(
    State(s): State<AppState>,
    i: Integration,
    Path(code): Path<String>,
    Json(req): Json<TokenReq>,
) -> AppResult<Json<EmbedLink>> {
    rate_limit::check(&s.redis, &format!("int_token:{}", i.id), 3000, 60).await?;
    let input = TokenInput {
        external_id: req.external_id,
        display_name: req.display_name,
        host: matches!(req.role, Role::Host),
        ttl_secs: req.ttl,
    };
    Ok(Json(
        service::embed_link(
            &s.db,
            &s.livekit,
            &s.config.public_web_url,
            &i,
            &code,
            input,
        )
        .await?,
    ))
}

async fn end_meeting(
    State(s): State<AppState>,
    i: Integration,
    Path(code): Path<String>,
) -> AppResult<StatusCode> {
    service::end_meeting(&s.db, &s.livekit, &i, &code).await?;
    webhook::dispatch(&s.db, &s.http, &code, "meeting_ended", None).await;
    Ok(StatusCode::NO_CONTENT)
}
