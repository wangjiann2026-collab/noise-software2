//! Axum middleware layers for the noise-api.
pub mod auth;
pub use auth::{AuthClaims, auth_layer, require_role};
