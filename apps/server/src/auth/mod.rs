//! Authentication: Argon2id passwords + opaque server-side sessions in HTTP-only cookies.

pub mod http;
pub mod password;
pub mod service;
pub mod token;

use axum::{extract::FromRequestParts, http::request::Parts};
use axum_extra::extract::CookieJar;

use crate::{error::AppError, state::AppState};
pub use service::User;

pub const SESSION_COOKIE: &str = "tm_session";

/// Extractor: rejects with 401 unless a valid session cookie is present.
pub struct CurrentUser(pub User);

impl FromRequestParts<AppState> for CurrentUser {
    type Rejection = AppError;

    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, AppError> {
        let jar = CookieJar::from_headers(&parts.headers);
        let token = jar.get(SESSION_COOKIE).ok_or(AppError::Unauthorized)?;
        service::user_by_session(&state.db, token.value())
            .await?
            .map(CurrentUser)
            .ok_or(AppError::Unauthorized)
    }
}

/// Extractor: like `CurrentUser` but requires the admin role (RBAC).
pub struct AdminUser(pub User);

impl FromRequestParts<AppState> for AdminUser {
    type Rejection = AppError;

    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, AppError> {
        let CurrentUser(user) = CurrentUser::from_request_parts(parts, state).await?;
        if !user.is_admin() {
            return Err(AppError::Forbidden);
        }
        Ok(AdminUser(user))
    }
}
