//! Authentication use cases, independent of HTTP.

use chrono::{DateTime, Duration, Utc};
use serde::Serialize;
use serde_json::json;
use sqlx::PgPool;
use uuid::Uuid;

use super::{password, token};
use crate::{
    audit,
    error::{AppError, AppResult},
};

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct User {
    pub id: Uuid,
    pub email: String,
    pub display_name: String,
    pub role: String,
    pub created_at: DateTime<Utc>,
}

impl User {
    pub fn is_admin(&self) -> bool {
        self.role == "admin"
    }
}

/// Column list for `User`; every user query aliases `users` as `u`.
/// A macro (not a const) so queries stay `&'static str` via `concat!`, as sqlx 0.9 requires.
macro_rules! user_cols {
    () => {
        "u.id, u.email::text AS email, u.display_name, u.role::text AS role, u.created_at"
    };
}

pub struct Credentials {
    pub email: String,
    pub password: String,
}

fn normalize_email(email: &str) -> AppResult<String> {
    let email = email.trim().to_lowercase();
    let valid = email.len() <= 254
        && email
            .split_once('@')
            .is_some_and(|(local, domain)| !local.is_empty() && domain.contains('.'));
    if !valid {
        return Err(AppError::Validation("invalid email".into()));
    }
    Ok(email)
}

fn validate_password(pw: &str) -> AppResult<()> {
    if !(8..=128).contains(&pw.chars().count()) {
        return Err(AppError::Validation(
            "password must be 8-128 characters".into(),
        ));
    }
    Ok(())
}

pub fn validate_display_name(name: &str) -> AppResult<String> {
    let name = name.trim();
    if !(1..=80).contains(&name.chars().count()) {
        return Err(AppError::Validation(
            "display name must be 1-80 characters".into(),
        ));
    }
    Ok(name.to_string())
}

async fn hash_blocking(pw: String) -> AppResult<String> {
    tokio::task::spawn_blocking(move || password::hash(&pw))
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?
}

pub async fn register(db: &PgPool, creds: Credentials, display_name: &str) -> AppResult<User> {
    let email = normalize_email(&creds.email)?;
    validate_password(&creds.password)?;
    let display_name = validate_display_name(display_name)?;
    let hash = hash_blocking(creds.password).await?;

    let mut tx = db.begin().await?;
    let user: User = sqlx::query_as(concat!(
        "INSERT INTO users AS u (email, display_name, password_hash) VALUES ($1, $2, $3)
         RETURNING ",
        user_cols!()
    ))
    .bind(&email)
    .bind(&display_name)
    .bind(&hash)
    .fetch_one(&mut *tx)
    .await
    .map_err(|e| match &e {
        sqlx::Error::Database(d) if d.is_unique_violation() => {
            AppError::Conflict("email already registered".into())
        }
        _ => e.into(),
    })?;
    audit::record(
        &mut *tx,
        Some(user.id),
        "user.register",
        Some(("user", user.id)),
        json!({}),
    )
    .await?;
    tx.commit().await?;
    Ok(user)
}

/// Returns the user and a fresh raw session token.
pub async fn login(db: &PgPool, creds: Credentials, ttl_hours: i64) -> AppResult<(User, String)> {
    let email = normalize_email(&creds.email).map_err(|_| AppError::InvalidCredentials)?;
    let row: Option<(Uuid, String, bool)> = sqlx::query_as(
        "SELECT id, password_hash, disabled_at IS NOT NULL FROM users WHERE email = $1",
    )
    .bind(&email)
    .fetch_optional(db)
    .await?;

    let stored = row.as_ref().map(|r| r.1.clone());
    let pw = creds.password;
    let ok = tokio::task::spawn_blocking(move || password::verify(&pw, stored.as_deref()))
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?;

    let user_id = match row {
        Some((id, _, false)) if ok => id,
        _ => {
            audit::record(
                db,
                None,
                "auth.login_failed",
                None,
                json!({ "email": email }),
            )
            .await?;
            return Err(AppError::InvalidCredentials);
        }
    };

    let raw = token::generate();
    let mut tx = db.begin().await?;
    sqlx::query("INSERT INTO sessions (user_id, token_hash, expires_at) VALUES ($1, $2, $3)")
        .bind(user_id)
        .bind(token::hash(&raw))
        .bind(Utc::now() + Duration::hours(ttl_hours))
        .execute(&mut *tx)
        .await?;
    audit::record(&mut *tx, Some(user_id), "auth.login", None, json!({})).await?;
    let user: User = sqlx::query_as(concat!(
        "SELECT ",
        user_cols!(),
        " FROM users u WHERE u.id = $1"
    ))
    .bind(user_id)
    .fetch_one(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok((user, raw))
}

pub async fn user_by_session(db: &PgPool, raw_token: &str) -> AppResult<Option<User>> {
    Ok(sqlx::query_as(concat!(
        "SELECT ",
        user_cols!(),
        " FROM sessions s JOIN users u ON u.id = s.user_id
         WHERE s.token_hash = $1 AND s.revoked_at IS NULL AND s.expires_at > now()
           AND u.disabled_at IS NULL"
    ))
    .bind(token::hash(raw_token))
    .fetch_optional(db)
    .await?)
}

pub async fn logout(db: &PgPool, raw_token: &str) -> AppResult<()> {
    let mut tx = db.begin().await?;
    let user_id: Option<Uuid> = sqlx::query_scalar(
        "UPDATE sessions SET revoked_at = now()
         WHERE token_hash = $1 AND revoked_at IS NULL RETURNING user_id",
    )
    .bind(token::hash(raw_token))
    .fetch_optional(&mut *tx)
    .await?;
    if let Some(id) = user_id {
        audit::record(&mut *tx, Some(id), "auth.logout", None, json!({})).await?;
    }
    tx.commit().await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn email_normalization() {
        assert_eq!(
            normalize_email("  A@Example.COM ").ok().as_deref(),
            Some("a@example.com")
        );
        assert!(normalize_email("no-at-sign").is_err());
        assert!(normalize_email("@example.com").is_err());
        assert!(normalize_email("a@localhost").is_err());
    }

    #[test]
    fn password_and_name_bounds() {
        assert!(validate_password("short").is_err());
        assert!(validate_password("long enough").is_ok());
        assert!(validate_display_name("   ").is_err());
        assert_eq!(validate_display_name(" Ana ").ok().as_deref(), Some("Ana"));
    }
}
