//! Append-only audit log.

use serde_json::Value;
use sqlx::PgExecutor;
use uuid::Uuid;

pub async fn record<'e>(
    db: impl PgExecutor<'e>,
    actor_id: Option<Uuid>,
    action: &str,
    target: Option<(&str, Uuid)>,
    metadata: Value,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO audit_logs (actor_id, action, target_type, target_id, metadata)
         VALUES ($1, $2, $3, $4, $5)",
    )
    .bind(actor_id)
    .bind(action)
    .bind(target.map(|t| t.0))
    .bind(target.map(|t| t.1))
    .bind(metadata)
    .execute(db)
    .await?;
    Ok(())
}
