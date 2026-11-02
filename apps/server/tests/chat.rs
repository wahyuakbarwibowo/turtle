//! Persisted chat against real PostgreSQL, Redis and a local `livekit-server --dev`.

mod common;

use axum::http::StatusCode;
use common::{new_user, send, send_with, test_app};
use serde_json::json;

#[tokio::test]
async fn participants_chat_and_removed_guest_is_blocked() {
    let app = test_app().await;
    let (host, _) = new_user(&app, "chathost").await;
    let (_, _, m) = send(
        &app,
        "POST",
        "/api/meetings",
        Some(&host),
        Some(json!({ "title": "Chat" })),
    )
    .await;
    let code = m["code"]
        .as_str()
        .unwrap_or_else(|| panic!("create failed: {m}"))
        .to_string();
    let chat = format!("/api/meetings/{code}/chat");

    let (_, _, h) = send(
        &app,
        "POST",
        &format!("/api/meetings/{code}/join"),
        Some(&host),
        Some(json!({})),
    )
    .await;
    let host_pt = h["participant_token"]
        .as_str()
        .expect("participant token")
        .to_string();
    let (_, _, g) = send(
        &app,
        "POST",
        &format!("/api/meetings/{code}/join"),
        None,
        Some(json!({ "display_name": "Chatty Guest" })),
    )
    .await;
    let guest_pt = g["participant_token"]
        .as_str()
        .expect("participant token")
        .to_string();

    let (status, _, _) = send(&app, "POST", &chat, None, Some(json!({ "body": "hi" }))).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED, "no token");
    let (status, _, _) = send_with(
        &app,
        "POST",
        &chat,
        None,
        Some("garbage"),
        Some(json!({ "body": "hi" })),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED, "bad token");

    let (status, _, msg) = send_with(
        &app,
        "POST",
        &chat,
        None,
        Some(&guest_pt),
        Some(json!({ "body": "  hello all  " })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{msg}");
    assert_eq!(msg["body"], "hello all");
    assert_eq!(msg["display_name"], "Chatty Guest");
    let (status, _, _) = send_with(
        &app,
        "POST",
        &chat,
        None,
        Some(&host_pt),
        Some(json!({ "body": "welcome" })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);

    let (status, _, hist) = send_with(&app, "GET", &chat, None, Some(&host_pt), None).await;
    assert_eq!(status, StatusCode::OK);
    let bodies: Vec<_> = hist
        .as_array()
        .expect("array")
        .iter()
        .map(|m| m["body"].clone())
        .collect();
    assert_eq!(
        bodies,
        vec![json!("hello all"), json!("welcome")],
        "oldest first"
    );

    // A token for one meeting must not work on another.
    let (_, _, other) = send(
        &app,
        "POST",
        "/api/meetings",
        Some(&host),
        Some(json!({ "title": "Other" })),
    )
    .await;
    let other_chat = format!(
        "/api/meetings/{}/chat",
        other["code"].as_str().expect("code")
    );
    let (status, _, _) = send_with(&app, "GET", &other_chat, None, Some(&guest_pt), None).await;
    assert_eq!(status, StatusCode::FORBIDDEN);

    let guest_identity = g["identity"].as_str().expect("identity");
    let (status, _, _) = send_with(
        &app,
        "POST",
        &format!("/api/meetings/{code}/participants/{guest_identity}/remove"),
        None,
        Some(&host_pt),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (status, _, _) = send_with(
        &app,
        "POST",
        &chat,
        None,
        Some(&guest_pt),
        Some(json!({ "body": "still here?" })),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::FORBIDDEN,
        "removed participants can't post"
    );
}
