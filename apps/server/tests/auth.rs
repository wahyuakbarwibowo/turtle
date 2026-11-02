//! End-to-end auth flow against real PostgreSQL + Redis.
//! Requires TEST_DATABASE_URL and REDIS_URL. Uses unique emails, so no cleanup is needed.

use std::sync::Arc;

use axum::{
    Router,
    body::Body,
    http::{Request, StatusCode, header},
};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use server::{AppState, app, config::Config};
use tower::ServiceExt;

async fn test_app() -> Router {
    let db_url = std::env::var("TEST_DATABASE_URL").expect("set TEST_DATABASE_URL");
    let redis_url = std::env::var("REDIS_URL").unwrap_or_else(|_| "redis://127.0.0.1:6379".into());
    let db = database::connect(&db_url, 5).await.expect("connect db");
    database::migrate(&db).await.expect("migrate");
    let redis = redis::aio::ConnectionManager::new(
        redis::Client::open(redis_url.as_str()).expect("redis url"),
    )
    .await
    .expect("connect redis");
    let config = Config {
        bind_addr: String::new(),
        database_url: db_url,
        db_max_connections: 5,
        redis_url,
        cookie_secure: false,
        session_ttl_hours: 1,
    };
    app(AppState {
        db,
        redis,
        config: Arc::new(config),
    })
}

async fn send(
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
    let json = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    (status, set_cookie, json)
}

#[tokio::test]
async fn register_login_me_logout() {
    let app = test_app().await;
    let email = format!("user-{}@example.test", uuid::Uuid::new_v4());
    let pw = "s3cure-passphrase";

    let (status, cookie, body) = send(
        &app,
        "POST",
        "/api/auth/register",
        None,
        Some(json!({ "email": email, "password": pw, "display_name": "Tester" })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    assert_eq!(body["email"], email);
    let cookie = cookie.expect("session cookie");

    let (status, _, body) = send(&app, "GET", "/api/auth/me", Some(&cookie), None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["display_name"], "Tester");

    let (status, _, _) = send(
        &app,
        "POST",
        "/api/auth/register",
        None,
        Some(json!({ "email": email.to_uppercase(), "password": pw, "display_name": "Dup" })),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "emails are case-insensitive");

    let (status, _, _) = send(
        &app,
        "POST",
        "/api/auth/login",
        None,
        Some(json!({ "email": email, "password": "wrong-password" })),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);

    let (status, _, _) = send(&app, "POST", "/api/auth/logout", Some(&cookie), None).await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (status, _, _) = send(&app, "GET", "/api/auth/me", Some(&cookie), None).await;
    assert_eq!(
        status,
        StatusCode::UNAUTHORIZED,
        "revoked session must not authenticate"
    );

    let (status, cookie, _) = send(
        &app,
        "POST",
        "/api/auth/login",
        None,
        Some(json!({ "email": email, "password": pw })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(cookie.is_some());
}

#[tokio::test]
async fn rejects_invalid_input_and_anonymous() {
    let app = test_app().await;
    let (status, _, _) = send(&app, "GET", "/api/auth/me", None, None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);

    let (status, _, body) = send(
        &app,
        "POST",
        "/api/auth/register",
        None,
        Some(json!({ "email": "bad", "password": "x", "display_name": "A" })),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(body["error"]["code"], "validation_error");
}
