mod common;

use axum::{Router, body::Bytes, http::HeaderMap, routing::post};
use common::{send_with, test_app};
use serde_json::{Value, json};
use server::integrations::{service as integrations, webhook};
use tokio::sync::mpsc;

/// A local webhook receiver; yields (headers, body) per delivery.
async fn webhook_sink() -> (String, mpsc::UnboundedReceiver<(HeaderMap, Bytes)>) {
    let (tx, rx) = mpsc::unbounded_channel();
    let app = Router::new().route(
        "/hook",
        post(move |h: HeaderMap, b: Bytes| {
            let tx = tx.clone();
            async move {
                let _ = tx.send((h, b));
            }
        }),
    );
    let l = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind");
    let url = format!("http://{}/hook", l.local_addr().expect("addr"));
    tokio::spawn(async move { axum::serve(l, app).await });
    (url, rx)
}

async fn new_integration(webhook_url: Option<&str>) -> (String, integrations::NewIntegration) {
    let db = database::connect(&std::env::var("TEST_DATABASE_URL").expect("db"), 2)
        .await
        .expect("connect");
    // Names are 2-32 chars; a uuid prefix keeps runs independent.
    let name = format!("t-{}", &uuid::Uuid::new_v4().simple().to_string()[..12]);
    let n = integrations::create(&db, &name, webhook_url)
        .await
        .expect("create integration");
    (name, n)
}

#[tokio::test]
async fn integration_creates_meeting_mints_embed_links_and_signs_webhooks() {
    let app = test_app().await;
    let (url, mut hooks) = webhook_sink().await;
    let (name, int) = new_integration(Some(&url)).await;
    let key = int.api_key.as_str();

    // No / wrong key.
    let (st, _, _) = send_with(
        &app,
        "POST",
        "/api/integrations/meetings",
        None,
        None,
        Some(json!({"title":"x"})),
    )
    .await;
    assert_eq!(st, 401);
    let (st, _, _) = send_with(
        &app,
        "POST",
        "/api/integrations/meetings",
        None,
        Some("tmk_nope"),
        Some(json!({"title":"x"})),
    )
    .await;
    assert_eq!(st, 401);

    let (st, _, m) = send_with(
        &app,
        "POST",
        "/api/integrations/meetings",
        None,
        Some(key),
        Some(json!({ "title": "Consultation", "external_ref": "ticket-7" })),
    )
    .await;
    assert_eq!(st, 201, "{m}");
    assert!(
        m.get("host_secret").is_none(),
        "integration meetings have no host secret"
    );
    let code = m["code"].as_str().expect("code").to_string();

    let mint = |ext: &str, role: &str| {
        let (app, code, key) = (app.clone(), code.clone(), key.to_string());
        let body =
            json!({ "external_id": ext, "display_name": format!("User {ext}"), "role": role });
        async move {
            send_with(
                &app,
                "POST",
                &format!("/api/integrations/meetings/{code}/tokens"),
                None,
                Some(&key),
                Some(body),
            )
            .await
        }
    };
    let (st, _, host) = mint("officer-1", "host").await;
    assert_eq!(st, 200, "{host}");
    assert_eq!(host["identity"], format!("x-{name}-officer-1"));
    assert!(
        host["embed_url"]
            .as_str()
            .expect("url")
            .contains(&format!("/embed/{code}#t="))
    );
    let (st, _, _) = mint("bad id!", "participant").await;
    assert_eq!(st, 422);

    // The plain join route is closed for integration meetings.
    let (st, _, _) = send_with(
        &app,
        "POST",
        &format!("/api/meetings/{code}/join"),
        None,
        None,
        Some(json!({"display_name":"Eve"})),
    )
    .await;
    assert_eq!(st, 403);

    // Role and identity come from the embed token.
    let join = |token: &Value| {
        let (app, code) = (app.clone(), code.clone());
        let body = json!({ "embed_token": token["embed_token"] });
        async move {
            send_with(
                &app,
                "POST",
                &format!("/api/meetings/{code}/join"),
                None,
                None,
                Some(body),
            )
            .await
        }
    };
    let (st, _, j) = join(&host).await;
    assert_eq!(st, 200, "{j}");
    assert_eq!(j["status"], "admitted");
    assert_eq!(j["role"], "host");
    assert_eq!(j["identity"], format!("x-{name}-officer-1"));
    let (_, _, applicant) = mint("applicant-9", "participant").await;
    let (st, _, j2) = join(&applicant).await;
    assert_eq!(st, 200);
    assert_eq!(j2["role"], "participant");

    // An embed token can't be used as a participant (in-meeting) token, nor for another meeting.
    let (st, _, _) = send_with(
        &app,
        "GET",
        &format!("/api/meetings/{code}/lobby"),
        None,
        host["embed_token"].as_str(),
        None,
    )
    .await;
    assert_eq!(st, 401);

    // Another integration can't see or mint for this meeting.
    let (_, other) = new_integration(None).await;
    let (st, _, _) = send_with(
        &app,
        "GET",
        &format!("/api/integrations/meetings/{code}"),
        None,
        Some(&other.api_key),
        None,
    )
    .await;
    assert_eq!(st, 404);

    let (st, _, info) = send_with(
        &app,
        "GET",
        &format!("/api/integrations/meetings/{code}"),
        None,
        Some(key),
        None,
    )
    .await;
    assert_eq!(st, 200);
    assert_eq!(info["participants"].as_array().expect("list").len(), 2);

    // End → signed meeting_ended webhook.
    let (st, _, _) = send_with(
        &app,
        "POST",
        &format!("/api/integrations/meetings/{code}/end"),
        None,
        Some(key),
        None,
    )
    .await;
    assert_eq!(st, 204);
    let (h, body) = tokio::time::timeout(std::time::Duration::from_secs(5), hooks.recv())
        .await
        .expect("webhook in time")
        .expect("delivery");
    let ts: i64 = h["x-turtlemeet-timestamp"]
        .to_str()
        .expect("ts")
        .parse()
        .expect("int");
    let secret = int.webhook_secret.as_deref().expect("secret");
    assert_eq!(
        h["x-turtlemeet-signature"],
        webhook::sign(secret, ts, &body).as_str()
    );
    let ev: Value = serde_json::from_slice(&body).expect("json");
    assert_eq!(ev["type"], "meeting_ended");
    assert_eq!(ev["meeting"]["external_ref"], "ticket-7");

    // Ended meetings mint nothing.
    let (st, _, _) = mint("late", "participant").await;
    assert_eq!(st, 410);
}

#[tokio::test]
async fn revoked_key_is_rejected() {
    let app = test_app().await;
    let (name, int) = new_integration(None).await;
    let db = database::connect(&std::env::var("TEST_DATABASE_URL").expect("db"), 1)
        .await
        .expect("db");
    assert!(integrations::revoke(&db, &name).await.expect("revoke"));
    let (st, _, _) = send_with(
        &app,
        "POST",
        "/api/integrations/meetings",
        None,
        Some(&int.api_key),
        Some(json!({"title":"x"})),
    )
    .await;
    assert_eq!(st, 401);
}
