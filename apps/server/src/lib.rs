//! TurtleMeet API server.

pub mod audit;
pub mod auth;
pub mod chat;
pub mod config;
pub mod error;
pub mod meetings;
pub mod participant;
pub mod rate_limit;
pub mod state;

use axum::{Json, Router, routing::get};
use serde_json::{Value, json};
use tower_http::trace::TraceLayer;

pub use state::AppState;

async fn health() -> Json<Value> {
    Json(json!({ "status": "ok" }))
}

/// Public, non-secret settings the web client needs before sign-in.
async fn public_config(axum::extract::State(s): axum::extract::State<AppState>) -> Json<Value> {
    Json(json!({ "allow_guest_create": s.config.allow_guest_create }))
}

pub fn app(state: AppState) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/api/config", get(public_config))
        .nest("/api/auth", auth::http::router())
        .nest(
            "/api/meetings",
            meetings::http::router().merge(chat::http::router()),
        )
        .nest("/api/livekit/webhook", meetings::http::webhook_router())
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}
