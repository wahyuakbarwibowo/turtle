//! Meeting lifecycle against real PostgreSQL, Redis and a local `livekit-server --dev`.

mod common;

use axum::{Router, http::StatusCode};
use common::{new_user, send, send_with, test_app};
use serde_json::{Value, json};

async fn create(app: &Router, cookie: Option<&str>, body: Value) -> Value {
    let (status, _, m) = send(app, "POST", "/api/meetings", cookie, Some(body)).await;
    assert_eq!(status, StatusCode::CREATED, "{m}");
    m
}

async fn join(app: &Router, code: &str, cookie: Option<&str>, body: Value) -> (StatusCode, Value) {
    let (status, _, j) = send(
        app,
        "POST",
        &format!("/api/meetings/{code}/join"),
        cookie,
        Some(body),
    )
    .await;
    (status, j)
}

fn pt(j: &Value) -> String {
    j["participant_token"]
        .as_str()
        .expect("participant_token")
        .to_string()
}

#[tokio::test]
async fn host_moderates_with_participant_token() {
    let app = test_app().await;
    let (host, host_id) = new_user(&app, "host").await;
    let (other, _) = new_user(&app, "other").await;

    let m = create(&app, Some(&host), json!({ "title": "Standup" })).await;
    let code = m["code"].as_str().expect("code").to_string();
    assert!(
        m.get("host_secret").is_none(),
        "account meetings don't get a host secret"
    );

    let (status, h) = join(&app, &code, Some(&host), json!({})).await;
    assert_eq!(status, StatusCode::OK, "{h}");
    assert_eq!(h["status"], "admitted");
    assert_eq!(h["role"], "host");
    assert_eq!(h["identity"], format!("u-{host_id}"));
    let host_pt = pt(&h);

    let (_, o) = join(&app, &code, Some(&other), json!({})).await;
    let other_pt = pt(&o);
    let (status, _) = join(&app, &code, None, json!({})).await;
    assert_eq!(
        status,
        StatusCode::UNPROCESSABLE_ENTITY,
        "guest needs a display name"
    );
    let (status, g) = join(&app, &code, None, json!({ "display_name": "Guest" })).await;
    assert_eq!(status, StatusCode::OK, "{g}");
    let guest = g["identity"].as_str().expect("identity").to_string();

    let remove = format!("/api/meetings/{code}/participants/{guest}/remove");
    let (status, _, _) = send(&app, "POST", &remove, Some(&host), None).await;
    assert_eq!(
        status,
        StatusCode::UNAUTHORIZED,
        "moderation needs the participant token, not the cookie"
    );
    let (status, _, _) = send_with(&app, "POST", &remove, None, Some(&other_pt), None).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "only the host may remove");

    let mute = format!("/api/meetings/{code}/participants/{guest}/mute");
    let (status, _, b) = send_with(&app, "POST", &mute, None, Some(&host_pt), None).await;
    assert_eq!(status, StatusCode::NO_CONTENT, "{b}");
    let (status, _, _) = send_with(
        &app,
        "POST",
        &format!("/api/meetings/{code}/mute-all"),
        None,
        Some(&other_pt),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let (status, _, _) = send_with(
        &app,
        "POST",
        &format!("/api/meetings/{code}/mute-all"),
        None,
        Some(&host_pt),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let host_identity = format!("u-{host_id}");
    let (status, _, _) = send_with(
        &app,
        "POST",
        &format!("/api/meetings/{code}/participants/{host_identity}/remove"),
        None,
        Some(&host_pt),
        None,
    )
    .await;
    assert_eq!(
        status,
        StatusCode::UNPROCESSABLE_ENTITY,
        "hosts can't be removed"
    );

    let (status, _, b) = send_with(&app, "POST", &remove, None, Some(&host_pt), None).await;
    assert_eq!(status, StatusCode::NO_CONTENT, "{b}");

    let (status, _, list) = send(&app, "GET", "/api/meetings", Some(&host), None).await;
    assert_eq!(status, StatusCode::OK);
    assert!(
        list.as_array()
            .is_some_and(|a| a.iter().any(|m| m["code"] == code.as_str()))
    );

    let end = format!("/api/meetings/{code}/end");
    let (status, _, _) = send_with(&app, "POST", &end, None, Some(&other_pt), None).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let (status, _, _) = send_with(&app, "POST", &end, None, Some(&host_pt), None).await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (status, _) = join(&app, &code, Some(&host), json!({})).await;
    assert_eq!(status, StatusCode::GONE, "ended meetings can't be joined");
}

#[tokio::test]
async fn password_protects_non_hosts() {
    let app = test_app().await;
    let (host, _) = new_user(&app, "pwhost").await;
    let m = create(
        &app,
        Some(&host),
        json!({ "title": "Secret", "password": "open-sesame" }),
    )
    .await;
    let code = m["code"].as_str().expect("code").to_string();
    assert_eq!(m["has_password"], true);
    assert!(
        m.get("password_hash").is_none(),
        "hash never leaves the server"
    );

    let (status, h) = join(&app, &code, Some(&host), json!({})).await;
    assert_eq!(status, StatusCode::OK, "host skips the password: {h}");

    let (status, b) = join(&app, &code, None, json!({ "display_name": "G" })).await;
    assert_eq!(
        (status, b["error"]["code"].clone()),
        (StatusCode::UNAUTHORIZED, json!("password_required"))
    );
    let (status, b) = join(
        &app,
        &code,
        None,
        json!({ "display_name": "G", "password": "wrong-one" }),
    )
    .await;
    assert_eq!(
        (status, b["error"]["code"].clone()),
        (StatusCode::UNAUTHORIZED, json!("invalid_password"))
    );
    let (status, _) = join(
        &app,
        &code,
        None,
        json!({ "display_name": "G", "password": "open-sesame" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    // Host clears the password; guests then join without one.
    let settings = format!("/api/meetings/{code}/settings");
    let (status, _, s) = send_with(
        &app,
        "PATCH",
        &settings,
        None,
        Some(&pt(&h)),
        Some(json!({ "password": null })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{s}");
    assert_eq!(s["has_password"], false);
    let (status, _) = join(&app, &code, None, json!({ "display_name": "G2" })).await;
    assert_eq!(status, StatusCode::OK);
}

#[tokio::test]
async fn lobby_admit_and_deny() {
    let app = test_app().await;
    let (host, _) = new_user(&app, "lobbyhost").await;
    let m = create(
        &app,
        Some(&host),
        json!({ "title": "Lobby", "lobby_enabled": true }),
    )
    .await;
    let code = m["code"].as_str().expect("code").to_string();
    let (_, h) = join(&app, &code, Some(&host), json!({})).await;
    assert_eq!(h["status"], "admitted", "hosts bypass the lobby");
    let host_pt = pt(&h);

    let (status, a) = join(&app, &code, None, json!({ "display_name": "Alice" })).await;
    assert_eq!(status, StatusCode::OK, "{a}");
    assert_eq!(a["status"], "waiting");
    assert!(a.get("token").is_none(), "no media token while waiting");
    let (alice, alice_pt) = (a["identity"].as_str().expect("id").to_string(), pt(&a));
    let (_, b) = join(&app, &code, None, json!({ "display_name": "Bob" })).await;
    let (bob, bob_pt) = (b["identity"].as_str().expect("id").to_string(), pt(&b));

    // Waiting participants can't chat or see the lobby.
    let (status, _, _) = send_with(
        &app,
        "POST",
        &format!("/api/meetings/{code}/chat"),
        None,
        Some(&alice_pt),
        Some(json!({ "body": "let me in" })),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let lobby = format!("/api/meetings/{code}/lobby");
    let (status, _, _) = send_with(&app, "GET", &lobby, None, Some(&alice_pt), None).await;
    assert_eq!(status, StatusCode::FORBIDDEN);

    let (status, _, list) = send_with(&app, "GET", &lobby, None, Some(&host_pt), None).await;
    assert_eq!(status, StatusCode::OK);
    let names: Vec<_> = list
        .as_array()
        .expect("array")
        .iter()
        .map(|e| e["display_name"].clone())
        .collect();
    assert_eq!(names, vec![json!("Alice"), json!("Bob")]);

    let me = format!("/api/meetings/{code}/lobby/me");
    let (_, _, s) = send_with(&app, "GET", &me, None, Some(&alice_pt), None).await;
    assert_eq!(s["status"], "waiting");

    let (status, _, _) = send_with(
        &app,
        "POST",
        &format!("{lobby}/{alice}/admit"),
        None,
        Some(&host_pt),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (status, _, _) = send_with(
        &app,
        "POST",
        &format!("{lobby}/{bob}/deny"),
        None,
        Some(&host_pt),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);

    let (status, _, s) = send_with(&app, "GET", &me, None, Some(&alice_pt), None).await;
    assert_eq!(status, StatusCode::OK, "{s}");
    assert_eq!(s["status"], "admitted");
    assert!(
        s["token"]
            .as_str()
            .is_some_and(|t| t.split('.').count() == 3)
    );
    let (status, _, _) = send_with(&app, "GET", &me, None, Some(&bob_pt), None).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "denied");

    let (status, _, _) = send_with(
        &app,
        "POST",
        &format!("/api/meetings/{code}/chat"),
        None,
        Some(&alice_pt),
        Some(json!({ "body": "thanks!" })),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::CREATED,
        "admitted participants can chat"
    );
    let (_, _, list) = send_with(&app, "GET", &lobby, None, Some(&host_pt), None).await;
    assert_eq!(list, json!([]));
}

#[tokio::test]
async fn guest_created_meeting_uses_host_secret() {
    let app = test_app().await;
    let m = create(&app, None, json!({ "title": "Anon" })).await;
    let code = m["code"].as_str().expect("code").to_string();
    assert!(m["host_id"].is_null());
    let secret = m["host_secret"].as_str().expect("host_secret").to_string();

    let (_, plain) = join(&app, &code, None, json!({ "display_name": "Visitor" })).await;
    assert_eq!(plain["role"], "participant");
    let (_, wrong) = join(
        &app,
        &code,
        None,
        json!({ "display_name": "Faker", "host_secret": "nope" }),
    )
    .await;
    assert_eq!(
        wrong["role"], "participant",
        "wrong secret is just a participant"
    );
    let (status, h) = join(
        &app,
        &code,
        None,
        json!({ "display_name": "Creator", "host_secret": secret }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{h}");
    assert_eq!(h["role"], "host");

    let (status, _, _) = send_with(
        &app,
        "PATCH",
        &format!("/api/meetings/{code}/settings"),
        None,
        Some(&pt(&h)),
        Some(json!({ "lobby_enabled": true })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "guest host can moderate");
    let (status, _, _) = send_with(
        &app,
        "PATCH",
        &format!("/api/meetings/{code}/settings"),
        None,
        Some(&pt(&plain)),
        Some(json!({ "lobby_enabled": false })),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn hand_requires_admitted_participant() {
    let app = test_app().await;
    let (host, _) = new_user(&app, "handhost").await;
    let m = create(&app, Some(&host), json!({ "title": "Hands" })).await;
    let code = m["code"].as_str().expect("code").to_string();
    let (_, h) = join(&app, &code, Some(&host), json!({})).await;
    let (_, g) = join(&app, &code, None, json!({ "display_name": "Asker" })).await;

    // Not connected to LiveKit in this test, so updating attributes reports not found;
    // what matters here is authorization. The browser E2E covers the real attribute update.
    let hand = format!("/api/meetings/{code}/hand");
    let (status, _, _) = send(&app, "POST", &hand, None, Some(json!({ "raised": true }))).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    let (status, _, _) = send_with(
        &app,
        "POST",
        &format!("/api/meetings/{}/hand", "zzz-zzzz-zzz"),
        None,
        Some(&pt(&g)),
        Some(json!({ "raised": true })),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "token is room-scoped");
    let _ = h;
}

#[tokio::test]
async fn unknown_meeting_is_404_and_config_is_public() {
    let app = test_app().await;
    let (status, _, _) = send(&app, "GET", "/api/meetings/zzz-zzzz-zzz", None, None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, _, c) = send(&app, "GET", "/api/config", None, None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(c["allow_guest_create"], true);
}
