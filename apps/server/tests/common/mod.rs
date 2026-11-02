//! Shared harness: real PostgreSQL (TEST_DATABASE_URL), Redis and LiveKit (`livekit-server --dev`).
#![allow(dead_code)]

use std::sync::Arc;

use axum::{
    Router,
    body::Body,
    http::{Request, StatusCode, header},
};
use http_body_util::BodyExt;
use livekit_client::LiveKit;
use serde_json::{Value, json};
use server::{AppState, app, config::Config};
use tower::ServiceExt;

fn env_or(key: &str, default: &str) -> String {
    std::env::var(key).unwrap_or_else(|_| default.into())
}

pub async fn test_app() -> Router {
    let db_url = std::env::var("TEST_DATABASE_URL").expect("set TEST_DATABASE_URL");
    let redis_url = env_or("REDIS_URL", "redis://127.0.0.1:6379");
    let db = database::connect(&db_url, 5).await.expect("connect db");
    database::migrate(&db).await.expect("migrate");
    let redis = redis::aio::ConnectionManager::new(
        redis::Client::open(redis_url.as_str()).expect("redis url"),
    )
    .await
    .expect("connect redis");
    // Defaults match `livekit-server --dev`.
    let config = Config {
        bind_addr: String::new(),
        database_url: db_url,
        db_max_connections: 5,
        redis_url,
        cookie_secure: false,
        session_ttl_hours: 1,
        livekit_api_url: env_or("LIVEKIT_API_URL", "http://127.0.0.1:7880"),
        livekit_public_url: env_or("LIVEKIT_PUBLIC_URL", "ws://127.0.0.1:7880"),
        livekit_api_key: env_or("LIVEKIT_API_KEY", "devkey"),
        livekit_api_secret: env_or("LIVEKIT_API_SECRET", "secret"),
    };
    let livekit = LiveKit::new(
        &config.livekit_api_url,
        &config.livekit_api_key,
        &config.livekit_api_secret,
    );
    app(AppState {
        db,
        redis,
        config: Arc::new(config),
        livekit: Arc::new(livekit),
    })
}

/// Returns (status, `name=value` of Set-Cookie if any, JSON body or Null).
pub async fn send(
    app: &Router,
    method: &str,
    uri: &str,
    cookie: Option<&str>,
    body: Option<Value>,
) -> (StatusCode, Option<String>, Value) {
    let mut req = Request::builder().method(method).uri(uri);
    if let Some(c) = cookie {
        req = req.header(header::COOKIE, c);
    }
    let body = match body {
        Some(b) => {
            req = req.header(header::CONTENT_TYPE, "application/json");
            Body::from(b.to_string())
        }
        None => Body::empty(),
    };
    let res = app
        .clone()
        .oneshot(req.body(body).expect("request"))
        .await
        .expect("response");
    let status = res.status();
    let set_cookie = res
        .headers()
        .get(header::SET_COOKIE)
        .and_then(|v| v.to_str().ok())
        .map(|v| v.split(';').next().unwrap_or_default().to_string());
    let bytes = res.into_body().collect().await.expect("body").to_bytes();
    (
        status,
        set_cookie,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}

/// Registers a fresh user and returns (session cookie, user id).
pub async fn new_user(app: &Router, name: &str) -> (String, String) {
    let email = format!("{name}-{}@example.test", uuid::Uuid::new_v4());
    let (status, cookie, body) = send(
        app,
        "POST",
        "/api/auth/register",
        None,
        Some(json!({ "email": email, "password": "s3cure-passphrase", "display_name": name })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    (
        cookie.expect("cookie"),
        body["id"].as_str().expect("id").to_string(),
    )
}
