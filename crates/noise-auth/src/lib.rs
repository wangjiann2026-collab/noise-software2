//! # noise-auth
//!
//! Authentication and authorization for the noise platform.
//!
//! - Password hashing: Argon2id
//! - Session tokens: JWT (HS256)
//! - Roles: admin, analyst, viewer

pub mod jwt;
pub mod password;
pub mod user;

pub use jwt::{Claims, JwtError, TokenService};
pub use password::{hash_password, verify_password, PasswordError};
pub use user::{Role, User};
