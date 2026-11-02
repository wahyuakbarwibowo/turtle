//! Distributed fixed-window rate limiting backed by Redis.

use redis::aio::ConnectionManager;

use crate::error::{AppError, AppResult};

/// Allow at most `limit` hits per `window_secs` for `key`.
// ponytail: fixed window allows up to 2x burst at window edges; switch to sliding log if that matters.
pub async fn check(
    redis: &ConnectionManager,
    key: &str,
    limit: u64,
    window_secs: i64,
) -> AppResult<()> {
    let key = format!("turtlemeet:rl:{key}");
    let mut conn = redis.clone();
    let (count,): (u64,) = redis::pipe()
        .atomic()
        .incr(&key, 1)
        .expire(&key, window_secs)
        .ignore()
        .query_async(&mut conn)
        .await?;
    if count > limit {
        return Err(AppError::RateLimited);
    }
    Ok(())
}
