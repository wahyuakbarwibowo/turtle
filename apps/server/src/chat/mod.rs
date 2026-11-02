//! Meeting chat: persisted in PostgreSQL, delivered live over LiveKit's data channel.
//! Sending goes through the API so the sender identity is authenticated and every message is stored.

pub mod http;
pub mod service;

use axum::{extract::FromRequestParts, http::request::Parts};

use crate::{error::AppError, state::AppState};

/// Topic clients subscribe to with `useDataChannel`.
pub const CHAT_TOPIC: &str = "turtlemeet.chat";

/// Extractor: a `Bearer` participant token (from the join response). Works for guests too.
pub struct Participant {
    pub room: String,
    pub identity: String,
}

impl FromRequestParts<AppState> for Participant {
    type Rejection = AppError;

    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, AppError> {
        let token = parts
            .headers
            .get(axum::http::header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.strip_prefix("Bearer "))
            .ok_or(AppError::Unauthorized)?;
        let (room, identity) = state
            .livekit
            .verify_participant_token(token)
            .map_err(|_| AppError::Unauthorized)?;
        Ok(Participant { room, identity })
    }
}
