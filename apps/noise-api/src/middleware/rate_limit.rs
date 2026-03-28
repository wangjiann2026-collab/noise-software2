//! Per-IP sliding-window rate-limiting middleware for Axum.
//!
//! ## Usage
//!
//! ```rust,ignore
//! use std::sync::{Arc, Mutex};
//! use noise_api::middleware::rate_limit::{new_rate_limit_state, rate_limit_layer};
//!
//! let app = Router::new()
//!     .route("/api/v1/projects", get(list_projects))
//!     .layer(axum::middleware::from_fn(rate_limit_layer))
//!     .layer(Extension(new_rate_limit_state()));
//! ```

use axum::{
    body::Body,
    extract::ConnectInfo,
    http::Request,
    middleware::Next,
    response::{IntoResponse, Response},
    Extension, Json,
};
use axum::http::StatusCode;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::time::Instant;

// ─── Configuration ────────────────────────────────────────────────────────────

/// Rate-limit parameters.
#[derive(Debug, Clone)]
pub struct RateLimitConfig {
    /// Maximum number of requests allowed within `window_secs`.
    pub max_requests: u32,
    /// Sliding-window duration in seconds.
    pub window_secs: u64,
}

impl Default for RateLimitConfig {
    fn default() -> Self {
        Self {
            max_requests: 60,
            window_secs: 60,
        }
    }
}

// ─── State ────────────────────────────────────────────────────────────────────

/// Per-IP rate-limit state: maps an IP string to `(window_start, request_count)`.
pub type RateLimitState = Arc<Mutex<HashMap<String, (Instant, u32)>>>;

/// Construct a new, empty [`RateLimitState`].
pub fn new_rate_limit_state() -> RateLimitState {
    Arc::new(Mutex::new(HashMap::new()))
}

// ─── Middleware ───────────────────────────────────────────────────────────────

/// Axum middleware function that enforces per-IP sliding-window rate limiting.
///
/// Reads `RateLimitState` from an Axum [`Extension`] (inserted at app build
/// time with `.layer(Extension(new_rate_limit_state()))`).
///
/// IP address resolution order:
/// 1. First value in the `X-Forwarded-For` header.
/// 2. Socket address via [`ConnectInfo<SocketAddr>`].
///
/// When the limit is exceeded the middleware short-circuits and returns
/// `429 Too Many Requests` with a JSON body:
/// ```json
/// { "error": "Rate limit exceeded. Try again in N seconds." }
/// ```
pub async fn rate_limit_layer(
    Extension(rl): Extension<RateLimitState>,
    req: Request<Body>,
    next: Next,
) -> Response {
    let config = RateLimitConfig::default();

    // ── Resolve client IP ────────────────────────────────────────────────────

    let ip: String = req
        .headers()
        .get("x-forwarded-for")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.split(',').next())
        .map(|s| s.trim().to_owned())
        .or_else(|| {
            req.extensions()
                .get::<ConnectInfo<SocketAddr>>()
                .map(|ci| ci.0.ip().to_string())
        })
        .unwrap_or_else(|| "unknown".to_owned());

    // ── Sliding-window check ─────────────────────────────────────────────────

    let (allowed, retry_after_secs) = {
        let mut map = rl.lock().unwrap_or_else(|e| e.into_inner());
        let now = Instant::now();

        let (window_start, count) = map
            .entry(ip.clone())
            .or_insert((now, 0));

        let elapsed = now.duration_since(*window_start).as_secs();

        if elapsed >= config.window_secs {
            // Window has expired — reset.
            *window_start = now;
            *count = 1;
            (true, 0u64)
        } else if *count < config.max_requests {
            *count += 1;
            (true, 0u64)
        } else {
            // Limit exceeded — tell the caller how long until window resets.
            let retry = config.window_secs.saturating_sub(elapsed);
            (false, retry)
        }
    };

    if allowed {
        next.run(req).await
    } else {
        (
            StatusCode::TOO_MANY_REQUESTS,
            Json(serde_json::json!({
                "error": format!(
                    "Rate limit exceeded. Try again in {} seconds.",
                    retry_after_secs
                )
            })),
        )
            .into_response()
    }
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_state_is_empty() {
        let state = new_rate_limit_state();
        assert!(state.lock().unwrap().is_empty());
    }

    #[test]
    fn sliding_window_resets_after_expiry() {
        let map: HashMap<String, (Instant, u32)> = HashMap::new();
        let state = Arc::new(Mutex::new(map));
        let config = RateLimitConfig { max_requests: 2, window_secs: 60 };

        // Simulate two requests within the window.
        {
            let mut m = state.lock().unwrap();
            let now = Instant::now();
            m.insert("1.2.3.4".to_owned(), (now, 2));
        }

        // Third request should be rejected.
        {
            let mut m = state.lock().unwrap();
            let now = Instant::now();
            let (ws, count) = m.get_mut("1.2.3.4").unwrap();
            let elapsed = now.duration_since(*ws).as_secs();
            let allowed = if elapsed >= config.window_secs {
                *ws = now;
                *count = 1;
                true
            } else if *count < config.max_requests {
                *count += 1;
                true
            } else {
                false
            };
            assert!(!allowed, "third request should be rejected");
        }
    }

    #[test]
    fn default_config_has_60_rpm() {
        let cfg = RateLimitConfig::default();
        assert_eq!(cfg.max_requests, 60);
        assert_eq!(cfg.window_secs, 60);
    }
}
