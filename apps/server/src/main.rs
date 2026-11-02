use std::sync::Arc;

use livekit_client::LiveKit;
use server::{AppState, app, config::Config};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    telemetry::init();
    let config = Config::from_env()?;

    let db = database::connect(&config.database_url, config.db_max_connections).await?;
    database::migrate(&db).await?;
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
    };
    axum::serve(listener, app(state)).await?;
    Ok(())
}
