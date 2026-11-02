//! In-meeting authentication via the participant token returned on join.
//! Works for guests (no session) and is scoped to a single room.

use axum::{extract::FromRequestParts, http::request::Parts};

use crate::{error::AppError, state::AppState};

/// Extractor: a `Bearer` participant token.
pub struct Participant {
    pub room: String,
    pub identity: String,
}

impl Participant {
    /// The token is scoped to one room; reject use against another meeting's code.
    pub fn check_room(&self, code: &str) -> Result<(), AppError> {
        if self.room != code {
            return Err(AppError::Forbidden);
        }
        Ok(())
    }
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
