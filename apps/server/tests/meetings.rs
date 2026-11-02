//! Meeting lifecycle against real PostgreSQL, Redis and a local `livekit-server --dev`.

mod common;

use axum::http::StatusCode;
use common::{new_user, send, test_app};
use serde_json::json;

#[tokio::test]
async fn host_creates_guest_joins_host_moderates() {
    let app = test_app().await;
    let (host, host_id) = new_user(&app, "host").await;
    let (other, _) = new_user(&app, "other").await;

    let (status, _, m) = send(
        &app,
        "POST",
        "/api/meetings",
        None,
        Some(json!({ "title": "x" })),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED, "{m}");

    let (status, _, m) = send(
        &app,
        "POST",
        "/api/meetings",
        Some(&host),
        Some(json!({ "title": "Standup" })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{m}");
    let code = m["code"].as_str().expect("code").to_string();

    let (status, _, j) = send(
        &app,
        "POST",
        &format!("/api/meetings/{code}/join"),
        Some(&host),
        Some(json!({})),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{j}");
    assert_eq!(j["role"], "host");
    assert_eq!(j["identity"], format!("u-{host_id}"));
    assert!(
        j["token"]
            .as_str()
            .is_some_and(|t| t.split('.').count() == 3)
    );

    let (status, _, _) = send(
        &app,
        "POST",
        &format!("/api/meetings/{code}/join"),
        None,
        Some(json!({})),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::UNPROCESSABLE_ENTITY,
        "guest needs a display name"
    );
    let (status, _, g) = send(
        &app,
        "POST",
        &format!("/api/meetings/{code}/join"),
        None,
        Some(json!({ "display_name": "Guest" })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{g}");
    assert_eq!(g["role"], "participant");
    let guest_identity = g["identity"].as_str().expect("identity").to_string();

    let remove = format!("/api/meetings/{code}/participants/{guest_identity}/remove");
    let (status, _, _) = send(&app, "POST", &remove, Some(&other), None).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "only the host may remove");
    let (status, _, body) = send(&app, "POST", &remove, Some(&host), None).await;
    assert_eq!(status, StatusCode::NO_CONTENT, "{body}");

    let (status, _, list) = send(&app, "GET", "/api/meetings", Some(&host), None).await;
    assert_eq!(status, StatusCode::OK);
    assert!(
        list.as_array()
            .is_some_and(|a| a.iter().any(|m| m["code"] == code.as_str()))
    );

    let (status, _, _) = send(
        &app,
        "POST",
        &format!("/api/meetings/{code}/end"),
        Some(&other),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let (status, _, _) = send(
        &app,
        "POST",
        &format!("/api/meetings/{code}/end"),
        Some(&host),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (status, _, _) = send(
        &app,
        "POST",
        &format!("/api/meetings/{code}/join"),
        Some(&host),
        Some(json!({})),
    )
    .await;
    assert_eq!(status, StatusCode::GONE, "ended meetings can't be joined");
}

#[tokio::test]
async fn unknown_meeting_is_404() {
    let app = test_app().await;
    let (status, _, _) = send(&app, "GET", "/api/meetings/zzz-zzzz-zzz", None, None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}
