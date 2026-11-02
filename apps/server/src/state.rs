use std::sync::Arc;

use database::PgPool;
use redis::aio::ConnectionManager;

use crate::config::Config;

#[derive(Clone)]
pub struct AppState {
    pub db: PgPool,
    pub redis: ConnectionManager,
    pub config: Arc<Config>,
}
