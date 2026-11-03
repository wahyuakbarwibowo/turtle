use std::sync::Arc;

use database::PgPool;
use livekit_client::LiveKit;
use redis::aio::ConnectionManager;

use crate::config::Config;

#[derive(Clone)]
pub struct AppState {
    pub db: PgPool,
    pub redis: ConnectionManager,
    pub config: Arc<Config>,
    pub livekit: Arc<LiveKit>,
    /// Outbound HTTP (integration webhooks). Cheap to clone; shares a connection pool.
    pub http: reqwest::Client,
}
