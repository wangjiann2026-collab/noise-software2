//! High-level authentication service.
//!
//! `AuthService` combines `TokenService` (JWT) + `password` (Argon2) into a
//! single facade used by both the REST API and the CLI.  It is intentionally
//! decoupled from the database: callers provide user-lookup closures so that
//! the auth crate does not depend on `noise-data`.

use crate::{
    jwt::{Claims, JwtError, TokenService},
    password::{hash_password, verify_password, PasswordError},
    user::Role,
};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

// ─── Errors ──────────────────────────────────────────────────────────────────

#[derive(Debug, Error)]
pub enum AuthError {
    #[error("Invalid credentials")]
    InvalidCredentials,
    #[error("User not found: {0}")]
    UserNotFound(String),
    #[error("Username already taken: {0}")]
    UsernameTaken(String),
    #[error("Email already registered: {0}")]
    EmailTaken(String),
    #[error("Insufficient permissions: requires {required}, caller has {caller}")]
    Forbidden { required: String, caller: String },
    #[error("Token error: {0}")]
    Token(#[from] JwtError),
    #[error("Password error: {0}")]
    Password(#[from] PasswordError),
}

// ─── AuthService ─────────────────────────────────────────────────────────────

/// Combines JWT issuance and Argon2 password verification.
pub struct AuthService {
    pub tokens: TokenService,
}

impl AuthService {
    /// Create a new `AuthService` with the given JWT secret.
    pub fn new(jwt_secret: &[u8]) -> Self {
        Self { tokens: TokenService::new(jwt_secret) }
    }

    /// Attempt to log in.
    ///
    /// # Arguments
    /// * `username`       — supplied username
    /// * `password`       — plaintext password to verify
    /// * `stored_hash`    — Argon2 hash stored in the DB
    /// * `user_id`        — UUID of the matching user
    /// * `role`           — role string ("admin" / "analyst" / "viewer")
    ///
    /// Returns a signed JWT on success.
    pub fn login(
        &self,
        username: &str,
        password: &str,
        stored_hash: &str,
        user_id: Uuid,
        role: &str,
    ) -> Result<String, AuthError> {
        verify_password(password, stored_hash)?;
        let token = self.tokens.issue(user_id, username, role)?;
        Ok(token)
    }

    /// Hash a new password for storage during registration.
    pub fn hash_new_password(&self, password: &str) -> Result<String, AuthError> {
        Ok(hash_password(password)?)
    }

    /// Verify a JWT and return its claims.
    pub fn verify_token(&self, token: &str) -> Result<Claims, AuthError> {
        Ok(self.tokens.verify(token)?)
    }

    /// Check that `claims.role` satisfies the minimum required role level.
    ///
    /// Role hierarchy: admin > analyst > viewer
    pub fn require_role(claims: &Claims, required: &str) -> Result<(), AuthError> {
        if role_level(&claims.role) >= role_level(required) {
            Ok(())
        } else {
            Err(AuthError::Forbidden {
                required: required.into(),
                caller: claims.role.clone(),
            })
        }
    }
}

/// Numeric level for role comparison (higher = more permissions).
pub fn role_level(role: &str) -> u8 {
    match role {
        "admin"   => 3,
        "analyst" => 2,
        "viewer"  => 1,
        _         => 0,
    }
}

// ─── RegisterRequest (for REST + CLI use) ────────────────────────────────────

/// Parameters for a new user registration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegisterRequest {
    pub username: String,
    pub email: String,
    pub password: String,
    /// Role to assign — only admins should be able to set this.
    #[serde(default = "default_role")]
    pub role: String,
}

fn default_role() -> String { "viewer".into() }

/// Validate a registration request (field lengths, allowed roles).
pub fn validate_register(req: &RegisterRequest) -> Result<(), AuthError> {
    if req.username.len() < 3 || req.username.len() > 32 {
        return Err(AuthError::UserNotFound(
            "username must be 3–32 characters".into()
        ));
    }
    if !req.email.contains('@') {
        return Err(AuthError::UserNotFound("invalid email address".into()));
    }
    if req.password.len() < 8 {
        return Err(AuthError::InvalidCredentials);
    }
    if !["admin", "analyst", "viewer"].contains(&req.role.as_str()) {
        return Err(AuthError::Forbidden {
            required: "admin|analyst|viewer".into(),
            caller: req.role.clone(),
        });
    }
    Ok(())
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    const SECRET: &[u8] = b"phase8-test-secret-at-least-32-bytes!!";

    fn svc() -> AuthService { AuthService::new(SECRET) }

    #[test]
    fn login_correct_password_returns_token() {
        let svc = svc();
        let hash = svc.hash_new_password("hunter2").unwrap();
        let uid  = Uuid::new_v4();
        let tok  = svc.login("alice", "hunter2", &hash, uid, "analyst").unwrap();
        assert!(!tok.is_empty());
    }

    #[test]
    fn login_wrong_password_returns_error() {
        let svc = svc();
        let hash = svc.hash_new_password("correct").unwrap();
        let result = svc.login("alice", "wrong", &hash, Uuid::new_v4(), "analyst");
        assert!(matches!(result, Err(AuthError::InvalidCredentials)
                              | Err(AuthError::Password(_))));
    }

    #[test]
    fn verify_token_roundtrip() {
        let svc = svc();
        let hash = svc.hash_new_password("pw").unwrap();
        let uid  = Uuid::new_v4();
        let tok  = svc.login("bob", "pw", &hash, uid, "viewer").unwrap();
        let claims = svc.verify_token(&tok).unwrap();
        assert_eq!(claims.username, "bob");
        assert_eq!(claims.role, "viewer");
        assert_eq!(claims.sub, uid.to_string());
    }

    #[test]
    fn require_role_admin_passes_all() {
        let claims = Claims {
            sub: "x".into(), username: "x".into(), role: "admin".into(),
            iat: 0, exp: u64::MAX,
        };
        assert!(AuthService::require_role(&claims, "admin").is_ok());
        assert!(AuthService::require_role(&claims, "analyst").is_ok());
        assert!(AuthService::require_role(&claims, "viewer").is_ok());
    }

    #[test]
    fn require_role_viewer_fails_analyst() {
        let claims = Claims {
            sub: "x".into(), username: "x".into(), role: "viewer".into(),
            iat: 0, exp: u64::MAX,
        };
        assert!(AuthService::require_role(&claims, "analyst").is_err());
    }

    #[test]
    fn validate_register_ok() {
        let req = RegisterRequest {
            username: "charlie".into(),
            email: "c@example.com".into(),
            password: "12345678".into(),
            role: "analyst".into(),
        };
        assert!(validate_register(&req).is_ok());
    }

    #[test]
    fn validate_register_short_password() {
        let req = RegisterRequest {
            username: "charlie".into(),
            email: "c@example.com".into(),
            password: "short".into(),
            role: "viewer".into(),
        };
        assert!(matches!(validate_register(&req), Err(AuthError::InvalidCredentials)));
    }

    #[test]
    fn validate_register_invalid_role() {
        let req = RegisterRequest {
            username: "charlie".into(),
            email: "c@example.com".into(),
            password: "longpassword".into(),
            role: "superuser".into(),
        };
        assert!(matches!(validate_register(&req), Err(AuthError::Forbidden { .. })));
    }
}
