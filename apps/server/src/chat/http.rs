use axum::{
    Json, Router,
    extract::{Path, State},
    http::StatusCode,
    routing::get,
};
use serde::Deserialize;

use super::{
    Participant,
    service::{self, ChatMessage},
};
use crate::{
    error::{AppError, AppResult},
    rate_limit,
    state::AppState,
};

/// Mounted under /api/meetings/{code}/chat.
pub fn router() -> Router<AppState> {
    Router::new().route("/{code}/chat", get(history).post(send))
}

#[derive(Deserialize)]
struct SendReq {
    body: String,
}

/// The token is scoped to one room; reject use against another meeting's code.
fn check_room(p: &Participant, code: &str) -> AppResult<()> {
    if p.room != code {
        return Err(AppError::Forbidden);
    }
    Ok(())
}

async fn history(
    State(s): State<AppState>,
    p: Participant,
    Path(code): Path<String>,
) -> AppResult<Json<Vec<ChatMessage>>> {
    check_room(&p, &code)?;
    Ok(Json(service::history(&s.db, &code).await?))
}

async fn send(
    State(s): State<AppState>,
    p: Participant,
    Path(code): Path<String>,
    Json(req): Json<SendReq>,
) -> AppResult<(StatusCode, Json<ChatMessage>)> {
    check_room(&p, &code)?;
    rate_limit::check(&s.redis, &format!("chat:{code}:{}", p.identity), 20, 10).await?;
    let msg = service::send(&s.db, &s.livekit, &code, &p.identity, &req.body).await?;
    Ok((StatusCode::CREATED, Json(msg)))
}
