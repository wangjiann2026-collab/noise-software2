//! Axum middleware layers for the noise-api.
pub mod auth;
pub mod rate_limit;
pub use auth::{AuthClaims, auth_layer, require_role};
pub use rate_limit::{RateLimitConfig, RateLimitState, new_rate_limit_state, rate_limit_layer};
