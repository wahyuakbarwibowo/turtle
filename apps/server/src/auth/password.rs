//! Argon2id password hashing. CPU-heavy, so callers run it via `spawn_blocking`.

use std::sync::LazyLock;

use argon2::{
    Argon2, PasswordHash, PasswordHasher, PasswordVerifier,
    password_hash::{SaltString, rand_core::OsRng},
};

use crate::error::{AppError, AppResult};

/// Hash verified when the account doesn't exist, so login timing doesn't reveal valid emails.
static DUMMY_HASH: LazyLock<String> =
    LazyLock::new(|| hash("dummy-password-for-timing").unwrap_or_default());

pub fn hash(password: &str) -> AppResult<String> {
    let salt = SaltString::generate(&mut OsRng);
    Argon2::default() // Argon2id v19 with OWASP-recommended defaults
        .hash_password(password.as_bytes(), &salt)
        .map(|h| h.to_string())
        .map_err(|e| AppError::Internal(format!("hash password: {e}")))
}

pub fn verify(password: &str, stored: Option<&str>) -> bool {
    let stored = stored.unwrap_or(&DUMMY_HASH);
    PasswordHash::new(stored)
        .map(|parsed| {
            Argon2::default()
                .verify_password(password.as_bytes(), &parsed)
                .is_ok()
        })
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_roundtrip() {
        let h = hash("correct horse").expect("hash");
        assert!(h.starts_with("$argon2id$"));
        assert!(verify("correct horse", Some(&h)));
        assert!(!verify("wrong", Some(&h)));
        assert!(!verify("anything", None));
    }
}
