//! # noise-auth
//!
//! Authentication and authorization for the noise platform.
//!
//! - Password hashing: Argon2id
//! - Session tokens: JWT (HS256)
//! - Roles: admin, analyst, viewer

pub mod jwt;
pub mod password;
pub mod service;
pub mod user;

pub use jwt::{Claims, JwtError, TokenService};
pub use password::{hash_password, verify_password, PasswordError};
pub use service::{AuthError, AuthService, RegisterRequest, role_level, validate_register};
pub use user::{Role, User};
