//! Server-to-server API for third-party platforms (queue systems, LMSs, CRMs…) that
//! create meetings and embed TurtleMeet in their own UI, like Jitsi's External API.
//!
//! Flow: the platform's backend calls us with its API key to create a meeting and mint
//! short-lived embed links for its users; its frontend loads the link in an iframe via
//! `turtlemeet-embed.js`. We POST signed events back to the platform's webhook.

pub mod http;
pub mod service;
pub mod webhook;

use axum::{extract::FromRequestParts, http::request::Parts};
use uuid::Uuid;

use crate::{auth::token, error::AppError, state::AppState};

/// API keys look like `tmk_<64 hex>` so they're recognizable in logs and secret scanners.
pub const API_KEY_PREFIX: &str = "tmk_";

/// Extractor: an integration authenticated by `Authorization: Bearer tmk_…`.
#[derive(Debug, Clone)]
pub struct Integration {
    pub id: Uuid,
    pub name: String,
}

impl FromRequestParts<AppState> for Integration {
    type Rejection = AppError;

    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, AppError> {
        let key = parts
            .headers
            .get(axum::http::header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.strip_prefix("Bearer "))
            .filter(|k| k.starts_with(API_KEY_PREFIX))
            .ok_or(AppError::Unauthorized)?;
        let row: Option<(Uuid, String)> = sqlx::query_as(
            "SELECT id, name FROM integrations WHERE api_key_hash = $1 AND revoked_at IS NULL",
        )
        .bind(token::hash(key))
        .fetch_optional(&state.db)
        .await?;
        let (id, name) = row.ok_or(AppError::Unauthorized)?;
        Ok(Integration { id, name })
    }
}
