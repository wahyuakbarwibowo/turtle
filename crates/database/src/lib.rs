//! PostgreSQL pool construction and embedded migrations.

use std::time::Duration;

pub use sqlx::PgPool;
use sqlx::postgres::PgPoolOptions;

/// Migrations embedded from the workspace `migrations/` directory at compile time.
pub static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("../../migrations");

pub async fn connect(url: &str, max_connections: u32) -> Result<PgPool, sqlx::Error> {
    PgPoolOptions::new()
        .max_connections(max_connections)
        .acquire_timeout(Duration::from_secs(5))
        .connect(url)
        .await
}

pub async fn migrate(pool: &PgPool) -> Result<(), sqlx::migrate::MigrateError> {
    MIGRATOR.run(pool).await
}
