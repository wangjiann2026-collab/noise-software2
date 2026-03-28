//! JWT Bearer-token authentication middleware for Axum.
//!
//! ## Usage
//!
//! ```rust,ignore
//! // Protect a group of routes:
//! let protected = Router::new()
//!     .route("/projects", get(list_projects))
//!     .layer(axum::middleware::from_fn(auth_layer));
//! ```
//!
//! Handlers that need the caller's identity extract `AuthClaims`:
//! ```rust,ignore
//! async fn list_projects(claims: AuthClaims, ...) -> ...
//! ```

use axum::{
    extract::{FromRequestParts, Request},
    http::{StatusCode, header::AUTHORIZATION},
    middleware::Next,
    response::{IntoResponse, Response},
    Json,
};
use noise_auth::{Claims, AuthService, role_level};
use std::sync::Arc;

/// The validated JWT claims, injected into request extensions by `auth_layer`.
#[derive(Debug, Clone)]
pub struct AuthClaims(pub Claims);

/// Axum `FromRequestParts` extractor — pulls `AuthClaims` out of extensions
/// that were inserted by `auth_layer`.  Returns 401 if the middleware was not
/// applied (i.e., the route was accidentally left unprotected).
impl<S> FromRequestParts<S> for AuthClaims
where
    S: Send + Sync,
{
    type Rejection = (StatusCode, Json<serde_json::Value>);

    fn from_request_parts<'life0, 'life1, 'async_trait>(
        parts: &'life0 mut axum::http::request::Parts,
        _state: &'life1 S,
    ) -> ::core::pin::Pin<
        Box<
            dyn ::core::future::Future<Output = Result<Self, Self::Rejection>>
                + ::core::marker::Send
                + 'async_trait,
        >,
    >
    where
        'life0: 'async_trait,
        'life1: 'async_trait,
        Self: 'async_trait,
    {
        let result = parts
            .extensions
            .get::<AuthClaims>()
            .cloned()
            .ok_or_else(|| (
                StatusCode::UNAUTHORIZED,
                Json(serde_json::json!({ "error": "Authentication required" })),
            ));
        Box::pin(async move { result })
    }
}

/// Middleware function: validates `Authorization: Bearer <token>` and inserts
/// `AuthClaims` into request extensions on success, returns 401 otherwise.
pub async fn auth_layer(mut req: Request, next: Next) -> Response {
    let jwt_secret = std::env::var("NOISE_JWT_SECRET")
        .unwrap_or_else(|_| "change-me-in-production".into());
    let svc = AuthService::new(jwt_secret.as_bytes());

    let token = req
        .headers()
        .get(AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "));

    match token {
        None => (
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({ "error": "Missing Authorization: Bearer <token> header" })),
        ).into_response(),
        Some(tok) => match svc.verify_token(tok) {
            Ok(claims) => {
                req.extensions_mut().insert(AuthClaims(claims));
                next.run(req).await
            }
            Err(e) => (
                StatusCode::UNAUTHORIZED,
                Json(serde_json::json!({ "error": format!("Invalid token: {e}") })),
            ).into_response(),
        },
    }
}

/// Return 403 if the caller's role is below `required_role`.
///
/// Call after `auth_layer` has already validated the token.
pub fn require_role(
    claims: &Claims,
    required_role: &str,
) -> Result<(), (StatusCode, Json<serde_json::Value>)> {
    if role_level(&claims.role) >= role_level(required_role) {
        Ok(())
    } else {
        Err((
            StatusCode::FORBIDDEN,
            Json(serde_json::json!({
                "error": format!(
                    "Role '{}' is insufficient; '{}' or above required",
                    claims.role, required_role
                )
            })),
        ))
    }
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use noise_auth::Claims;

    fn analyst_claims() -> Claims {
        Claims { sub: "u1".into(), username: "alice".into(),
                 role: "analyst".into(), iat: 0, exp: u64::MAX }
    }

    fn viewer_claims() -> Claims {
        Claims { sub: "u2".into(), username: "bob".into(),
                 role: "viewer".into(), iat: 0, exp: u64::MAX }
    }

    fn admin_claims() -> Claims {
        Claims { sub: "u3".into(), username: "carol".into(),
                 role: "admin".into(), iat: 0, exp: u64::MAX }
    }

    #[test]
    fn require_role_analyst_passes_viewer_requirement() {
        assert!(require_role(&analyst_claims(), "viewer").is_ok());
    }

    #[test]
    fn require_role_analyst_passes_analyst_requirement() {
        assert!(require_role(&analyst_claims(), "analyst").is_ok());
    }

    #[test]
    fn require_role_analyst_fails_admin_requirement() {
        assert!(require_role(&analyst_claims(), "admin").is_err());
    }

    #[test]
    fn require_role_viewer_fails_analyst_requirement() {
        let result = require_role(&viewer_claims(), "analyst");
        assert!(result.is_err());
        let (status, _) = result.unwrap_err();
        assert_eq!(status, StatusCode::FORBIDDEN);
    }

    #[test]
    fn require_role_admin_passes_everything() {
        assert!(require_role(&admin_claims(), "admin").is_ok());
        assert!(require_role(&admin_claims(), "analyst").is_ok());
        assert!(require_role(&admin_claims(), "viewer").is_ok());
    }
}
