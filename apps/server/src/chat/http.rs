use axum::{
    Json, Router,
    extract::{Path, State},
    http::StatusCode,
    routing::get,
};
use serde::Deserialize;

use super::service::{self, ChatMessage};
use crate::{error::AppResult, participant::Participant, rate_limit, state::AppState};

/// Mounted under /api/meetings/{code}/chat.
pub fn router() -> Router<AppState> {
    Router::new().route("/{code}/chat", get(history).post(send))
}

#[derive(Deserialize)]
struct SendReq {
    body: String,
}

async fn history(
    State(s): State<AppState>,
    p: Participant,
    Path(code): Path<String>,
) -> AppResult<Json<Vec<ChatMessage>>> {
    p.check_room(&code)?;
    Ok(Json(service::history(&s.db, &code, &p.identity).await?))
}

async fn send(
    State(s): State<AppState>,
    p: Participant,
    Path(code): Path<String>,
    Json(req): Json<SendReq>,
) -> AppResult<(StatusCode, Json<ChatMessage>)> {
    p.check_room(&code)?;
    rate_limit::check(&s.redis, &format!("chat:{code}:{}", p.identity), 20, 10).await?;
    let msg = service::send(&s.db, &s.livekit, &code, &p.identity, &req.body).await?;
    Ok((StatusCode::CREATED, Json(msg)))
}
