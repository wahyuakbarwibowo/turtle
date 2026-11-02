//! End-to-end auth flow against real PostgreSQL + Redis.
//! Requires TEST_DATABASE_URL and REDIS_URL. Uses unique emails, so no cleanup is needed.

mod common;

use axum::http::StatusCode;
use common::{send, test_app};
use serde_json::json;

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
