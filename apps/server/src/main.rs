use std::sync::Arc;

use livekit_client::LiveKit;
use server::{AppState, app, config::Config, integrations::service as integrations};

const USAGE: &str = "usage:
  server                                                  run the API
  server integration create <name> [--webhook-url <url>]  print a new API key (shown once)
  server integration list
  server integration revoke <name>";

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    telemetry::init();
    let config = Config::from_env()?;

    let db = database::connect(&config.database_url, config.db_max_connections).await?;
    database::migrate(&db).await?;

    let args: Vec<String> = std::env::args().skip(1).collect();
    if !args.is_empty() {
        return integration_cli(&db, &args).await;
    }

    let redis =
        redis::aio::ConnectionManager::new(redis::Client::open(config.redis_url.as_str())?).await?;
    let livekit = LiveKit::new(
        &config.livekit_api_url,
        &config.livekit_api_key,
        &config.livekit_api_secret,
    );

    let listener = tokio::net::TcpListener::bind(&config.bind_addr).await?;
    tracing::info!(addr = %config.bind_addr, "turtlemeet server listening");
    let state = AppState {
        db,
        redis,
        config: Arc::new(config),
        livekit: Arc::new(livekit),
        http: reqwest::Client::new(),
    };
    axum::serve(listener, app(state)).await?;
    Ok(())
}

// ponytail: hand-rolled argv parsing for three subcommands; switch to clap if it grows.
async fn integration_cli(
    db: &sqlx::PgPool,
    args: &[String],
) -> Result<(), Box<dyn std::error::Error>> {
    let a: Vec<&str> = args.iter().map(String::as_str).collect();
    match a.as_slice() {
        ["integration", "create", name, rest @ ..] => {
            let webhook = match rest {
                [] => None,
                ["--webhook-url", url] => Some(*url),
                _ => return Err(USAGE.into()),
            };
            let n = integrations::create(db, name, webhook).await?;
            println!("id:             {}", n.id);
            println!("api_key:        {}", n.api_key);
            if let Some(s) = n.webhook_secret {
                println!("webhook_secret: {s}");
            }
            eprintln!("Store these now; the API key cannot be shown again.");
        }
        ["integration", "list"] => {
            for i in integrations::list(db).await? {
                let state = if i.revoked_at.is_some() {
                    "revoked"
                } else {
                    "active"
                };
                println!(
                    "{}\t{state}\t{}",
                    i.name,
                    i.webhook_url.as_deref().unwrap_or("-")
                );
            }
        }
        ["integration", "revoke", name] => {
            if !integrations::revoke(db, name).await? {
                return Err(format!("no active integration named {name}").into());
            }
            println!("revoked {name}");
        }
        _ => return Err(USAGE.into()),
    }
    Ok(())
}
