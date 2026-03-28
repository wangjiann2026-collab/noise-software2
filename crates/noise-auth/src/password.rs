use argon2::{
    password_hash::{rand_core::OsRng, PasswordHash, PasswordHasher, PasswordVerifier, SaltString},
    Argon2,
};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum PasswordError {
    #[error("Hashing failed: {0}")]
    HashFailed(String),
    #[error("Invalid password")]
    InvalidPassword,
    #[error("Invalid hash format: {0}")]
    InvalidHash(String),
}

/// Hash a plaintext password using Argon2id.
pub fn hash_password(plaintext: &str) -> Result<String, PasswordError> {
    let salt = SaltString::generate(&mut OsRng);
    Argon2::default()
        .hash_password(plaintext.as_bytes(), &salt)
        .map(|h| h.to_string())
        .map_err(|e| PasswordError::HashFailed(e.to_string()))
}

/// Verify a plaintext password against a stored Argon2 hash.
pub fn verify_password(plaintext: &str, hash_str: &str) -> Result<(), PasswordError> {
    let hash = PasswordHash::new(hash_str)
        .map_err(|e| PasswordError::InvalidHash(e.to_string()))?;
    Argon2::default()
        .verify_password(plaintext.as_bytes(), &hash)
        .map_err(|_| PasswordError::InvalidPassword)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_and_verify_roundtrip() {
        let pw = "S3cur3P@ssw0rd!";
        let hash = hash_password(pw).unwrap();
        assert!(verify_password(pw, &hash).is_ok());
    }

    #[test]
    fn wrong_password_fails_verification() {
        let hash = hash_password("correct").unwrap();
        assert!(matches!(verify_password("wrong", &hash), Err(PasswordError::InvalidPassword)));
    }
}
