//! Opaque session tokens. The cookie holds the raw token; the DB stores only its SHA-256.

use argon2::password_hash::rand_core::{OsRng, RngCore};
use sha2::{Digest, Sha256};

pub fn generate() -> String {
    let mut bytes = [0u8; 32];
    OsRng.fill_bytes(&mut bytes);
    hex(&bytes)
}

pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

pub fn hash(token: &str) -> Vec<u8> {
    Sha256::digest(token.as_bytes()).to_vec()
}

#[cfg(test)]
mod tests {
    #[test]
    fn tokens_are_unique_and_256_bit() {
        let (a, b) = (super::generate(), super::generate());
        assert_eq!(a.len(), 64);
        assert_ne!(a, b);
        assert_eq!(super::hash(&a).len(), 32);
    }
}
