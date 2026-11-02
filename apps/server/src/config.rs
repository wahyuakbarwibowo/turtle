//! Runtime configuration from environment variables.

use std::env;

#[derive(Clone, Debug)]
pub struct Config {
    pub bind_addr: String,
    pub database_url: String,
    pub db_max_connections: u32,
    pub redis_url: String,
    /// Set `Secure` on cookies. Must be true behind HTTPS in production.
    pub cookie_secure: bool,
    pub session_ttl_hours: i64,
    /// Server-to-server LiveKit API URL (e.g. http://livekit:7880).
    pub livekit_api_url: String,
    /// WebSocket URL handed to browsers (e.g. wss://rtc.example.com).
    pub livekit_public_url: String,
    pub livekit_api_key: String,
    pub livekit_api_secret: String,
    /// Let visitors create meetings without an account (Jitsi-style).
    pub allow_guest_create: bool,
}

#[derive(Debug, thiserror::Error)]
#[error("missing or invalid env var {0}")]
pub struct ConfigError(&'static str);

fn var(key: &'static str) -> Result<String, ConfigError> {
    env::var(key).map_err(|_| ConfigError(key))
}

fn var_or<T: std::str::FromStr>(key: &'static str, default: T) -> Result<T, ConfigError> {
    match env::var(key) {
        Ok(v) => v.parse().map_err(|_| ConfigError(key)),
        Err(_) => Ok(default),
    }
}

impl Config {
    pub fn from_env() -> Result<Self, ConfigError> {
        Ok(Self {
            bind_addr: var_or("BIND_ADDR", "0.0.0.0:8080".to_string())?,
            database_url: var("DATABASE_URL")?,
            db_max_connections: var_or("DB_MAX_CONNECTIONS", 20)?,
            redis_url: var("REDIS_URL")?,
            cookie_secure: var_or("COOKIE_SECURE", true)?,
            session_ttl_hours: var_or("SESSION_TTL_HOURS", 24 * 7)?,
            livekit_api_url: var("LIVEKIT_API_URL")?,
            livekit_public_url: var("LIVEKIT_PUBLIC_URL")?,
            livekit_api_key: var("LIVEKIT_API_KEY")?,
            livekit_api_secret: var("LIVEKIT_API_SECRET")?,
            allow_guest_create: var_or("ALLOW_GUEST_CREATE", false)?,
        })
    }
}
