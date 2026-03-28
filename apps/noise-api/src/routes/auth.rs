//! Authentication routes.
//!
//! POST /auth/login  — issue JWT token
//! POST /auth/verify — verify and decode a JWT token

use axum::{Json, http::StatusCode};
use serde::{Deserialize, Serialize};
use noise_auth::{TokenService, User, Role};
use uuid::Uuid;

#[derive(Debug, Deserialize)]
pub struct LoginRequest {
    pub username: String,
    pub password: String,
}

#[derive(Debug, Serialize)]
pub struct LoginResponse {
    pub token: String,
    pub user_id: String,
    pub username: String,
    pub role: String,
    pub expires_in_seconds: u64,
}

#[derive(Debug, Deserialize)]
pub struct VerifyRequest {
    pub token: String,
}

#[derive(Debug, Serialize)]
pub struct VerifyResponse {
    pub valid: bool,
    pub username: Option<String>,
    pub role: Option<String>,
    pub error: Option<String>,
}

/// POST /auth/login
///
/// Accepts username + password and returns a signed JWT.
/// For demonstration purposes a fixed admin user is accepted without
/// a real database; a production implementation would look up the user
/// in the DB and verify the Argon2 hash.
pub async fn login(
    Json(body): Json<LoginRequest>,
) -> Result<Json<LoginResponse>, (StatusCode, Json<serde_json::Value>)> {
    // Demo credential check — replace with DB lookup + argon2 verify.
    let (user_id, role_str) = match (body.username.as_str(), body.password.as_str()) {
        ("admin",   "admin123")   => (Uuid::new_v4(), "admin"),
        ("analyst", "analyst123") => (Uuid::new_v4(), "analyst"),
        ("viewer",  "viewer123")  => (Uuid::new_v4(), "viewer"),
        _ => {
            return Err((
                StatusCode::UNAUTHORIZED,
                Json(serde_json::json!({ "error": "Invalid credentials" })),
            ));
        }
    };

    let jwt_secret = std::env::var("NOISE_JWT_SECRET")
        .unwrap_or_else(|_| "change-me-in-production".into());
    let svc = TokenService::new(jwt_secret.as_bytes());
    let token = svc.issue(user_id, &body.username, role_str)
        .map_err(|e| (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": format!("Token issue failed: {e}") })),
        ))?;

    Ok(Json(LoginResponse {
        token,
        user_id: user_id.to_string(),
        username: body.username,
        role: role_str.into(),
        expires_in_seconds: svc.ttl_seconds,
    }))
}

/// POST /auth/verify
pub async fn verify(
    Json(body): Json<VerifyRequest>,
) -> Json<VerifyResponse> {
    let jwt_secret = std::env::var("NOISE_JWT_SECRET")
        .unwrap_or_else(|_| "change-me-in-production".into());
    let svc = TokenService::new(jwt_secret.as_bytes());
    match svc.verify(&body.token) {
        Ok(claims) => Json(VerifyResponse {
            valid: true,
            username: Some(claims.username),
            role: Some(claims.role),
            error: None,
        }),
        Err(e) => Json(VerifyResponse {
            valid: false,
            username: None,
            role: None,
            error: Some(e.to_string()),
        }),
    }
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn login_admin_returns_token() {
        let req = LoginRequest {
            username: "admin".into(),
            password: "admin123".into(),
        };
        let resp = login(Json(req)).await.unwrap();
        assert!(!resp.0.token.is_empty());
        assert_eq!(resp.0.role, "admin");
        assert_eq!(resp.0.expires_in_seconds, 86400);
    }

    #[tokio::test]
    async fn login_wrong_password_returns_401() {
        let req = LoginRequest {
            username: "admin".into(),
            password: "wrong".into(),
        };
        let result = login(Json(req)).await;
        assert!(result.is_err());
        assert_eq!(result.err().unwrap().0, StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn verify_valid_token() {
        let req = LoginRequest { username: "analyst".into(), password: "analyst123".into() };
        let login_resp = login(Json(req)).await.unwrap().0;
        let verify_resp = verify(Json(VerifyRequest { token: login_resp.token })).await.0;
        assert!(verify_resp.valid);
        assert_eq!(verify_resp.username.as_deref(), Some("analyst"));
    }

    #[tokio::test]
    async fn verify_invalid_token_returns_not_valid() {
        let resp = verify(Json(VerifyRequest { token: "garbage.token.here".into() })).await.0;
        assert!(!resp.valid);
        assert!(resp.error.is_some());
    }
}
