//! HTTP handlers for /api/auth.

use axum::{
    Json, Router,
    extract::State,
    http::StatusCode,
    routing::{get, post},
};
use axum_extra::extract::{
    CookieJar,
    cookie::{Cookie, SameSite},
};
use serde::Deserialize;

use super::{CurrentUser, SESSION_COOKIE, User, service};
use crate::{error::AppResult, rate_limit, state::AppState};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/register", post(register))
        .route("/login", post(login))
        .route("/logout", post(logout))
        .route("/me", get(me))
}

#[derive(Deserialize)]
struct RegisterReq {
    email: String,
    password: String,
    display_name: String,
}

#[derive(Deserialize)]
struct LoginReq {
    email: String,
    password: String,
}

// SameSite=Lax + JSON-only bodies blocks cross-site form CSRF: browsers can't send
// `application/json` cross-origin without a CORS preflight, which we don't allow.
fn session_cookie(state: &AppState, token: String) -> Cookie<'static> {
    Cookie::build((SESSION_COOKIE, token))
        .http_only(true)
        .secure(state.config.cookie_secure)
        .same_site(SameSite::Lax)
        .path("/")
        .max_age(cookie::time::Duration::hours(
            state.config.session_ttl_hours,
        ))
        .build()
}

async fn start_session(
    state: &AppState,
    jar: CookieJar,
    email: String,
    password: String,
) -> AppResult<(CookieJar, Json<User>)> {
    let creds = service::Credentials { email, password };
    let (user, token) = service::login(&state.db, creds, state.config.session_ttl_hours).await?;
    Ok((jar.add(session_cookie(state, token)), Json(user)))
}

async fn register(
    State(state): State<AppState>,
    jar: CookieJar,
    Json(req): Json<RegisterReq>,
) -> AppResult<(StatusCode, CookieJar, Json<User>)> {
    let key = format!("register:{}", req.email.trim().to_lowercase());
    rate_limit::check(&state.redis, &key, 5, 3600).await?;
    let creds = service::Credentials {
        email: req.email.clone(),
        password: req.password.clone(),
    };
    service::register(&state.db, creds, &req.display_name).await?;
    let (jar, user) = start_session(&state, jar, req.email, req.password).await?;
    Ok((StatusCode::CREATED, jar, user))
}

async fn login(
    State(state): State<AppState>,
    jar: CookieJar,
    Json(req): Json<LoginReq>,
) -> AppResult<(CookieJar, Json<User>)> {
    // ponytail: keyed per email (brute-force guard); per-IP limits belong at the Traefik edge.
    let key = format!("login:{}", req.email.trim().to_lowercase());
    rate_limit::check(&state.redis, &key, 10, 900).await?;
    start_session(&state, jar, req.email, req.password).await
}

async fn logout(
    State(state): State<AppState>,
    jar: CookieJar,
) -> AppResult<(StatusCode, CookieJar)> {
    if let Some(c) = jar.get(SESSION_COOKIE) {
        service::logout(&state.db, c.value()).await?;
    }
    Ok((
        StatusCode::NO_CONTENT,
        jar.remove(Cookie::build(SESSION_COOKIE).path("/")),
    ))
}

async fn me(CurrentUser(user): CurrentUser) -> Json<User> {
    Json(user)
}
