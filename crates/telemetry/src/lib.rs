//! Structured logging setup shared by binaries.

use tracing_subscriber::{EnvFilter, fmt};

/// Initialise JSON structured logging. Level comes from `RUST_LOG` (default `info`).
pub fn init() {
    fmt()
        .json()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();
}
