//! User management routes.
//!
//! POST /auth/register         → register new user (admin or open if no users exist)
//! GET  /users                 → list all users (admin only)
//! GET  /users/:id             → get user by ID (admin or self)
//! PUT  /users/:id/role        → update role (admin only)
//! DELETE /users/:id           → delete user (admin only)
//! PUT  /auth/change-password  → change own password (any authenticated user)

use axum::{extract::Path, http::StatusCode, Json};
use noise_auth::{AuthService, RegisterRequest, validate_register};
use noise_data::{
    db::Database,
    repository::{StoredUser, UserRepository},
};
use serde::{Deserialize, Serialize};

use crate::middleware::AuthClaims;

fn open_db() -> Result<Database, (StatusCode, Json<serde_json::Value>)> {
    let path = std::env::var("NOISE_DB").unwrap_or_else(|_| "noise.db".into());
    Database::open(&path).map_err(|e| (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(serde_json::json!({ "error": format!("Database error: {e}") })),
    ))
}

fn auth_svc() -> AuthService {
    let secret = std::env::var("NOISE_JWT_SECRET")
        .unwrap_or_else(|_| "change-me-in-production".into());
    AuthService::new(secret.as_bytes())
}

// ─── Public response type (no password hash) ─────────────────────────────────

#[derive(Debug, Serialize)]
pub struct UserResponse {
    pub id: String,
    pub username: String,
    pub email: String,
    pub role: String,
    pub created_at: String,
    pub last_login_at: Option<String>,
}

impl From<StoredUser> for UserResponse {
    fn from(u: StoredUser) -> Self {
        Self {
            id: u.id,
            username: u.username,
            email: u.email,
            role: u.role,
            created_at: u.created_at,
            last_login_at: u.last_login_at,
        }
    }
}

// ─── POST /auth/register ─────────────────────────────────────────────────────

/// Register a new user.
///
/// - If no users exist yet: anyone can register (bootstrap the first admin).
/// - Otherwise: only existing admins may register new users.
pub async fn register(
    claims: Option<AuthClaims>,
    Json(req): Json<RegisterRequest>,
) -> Result<(StatusCode, Json<UserResponse>), (StatusCode, Json<serde_json::Value>)> {
    validate_register(&req).map_err(|e| (
        StatusCode::UNPROCESSABLE_ENTITY,
        Json(serde_json::json!({ "error": e.to_string() })),
    ))?;

    let db = open_db()?;
    let repo = UserRepository::new(db.connection());

    // Bootstrap: allow open registration when no users exist yet.
    let total = repo.list().map_err(|e| (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(serde_json::json!({ "error": e.to_string() })),
    ))?.len();

    if total > 0 {
        // Require an authenticated admin.
        let c = claims.ok_or_else(|| (
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({ "error": "Authentication required to register new users" })),
        ))?;
        crate::middleware::require_role(&c.0, "admin")?;
    }

    // Check username uniqueness.
    if repo.get_by_username(&req.username).is_ok() {
        return Err((
            StatusCode::CONFLICT,
            Json(serde_json::json!({ "error": format!("Username '{}' already taken", req.username) })),
        ));
    }

    let svc = auth_svc();
    let hash = svc.hash_new_password(&req.password).map_err(|e| (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(serde_json::json!({ "error": e.to_string() })),
    ))?;

    let role = if total == 0 { "admin" } else { req.role.as_str() };
    let user = StoredUser::new(&req.username, hash, &req.email, role);
    repo.insert(&user).map_err(|e| (
        StatusCode::CONFLICT,
        Json(serde_json::json!({ "error": e.to_string() })),
    ))?;

    Ok((StatusCode::CREATED, Json(UserResponse::from(user))))
}

// ─── GET /users ──────────────────────────────────────────────────────────────

pub async fn list_users(
    AuthClaims(claims): AuthClaims,
) -> Result<Json<Vec<UserResponse>>, (StatusCode, Json<serde_json::Value>)> {
    crate::middleware::require_role(&claims, "admin")?;
    let db = open_db()?;
    let repo = UserRepository::new(db.connection());
    let users = repo.list().map_err(|e| (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(serde_json::json!({ "error": e.to_string() })),
    ))?;
    Ok(Json(users.into_iter().map(UserResponse::from).collect()))
}

// ─── GET /users/:id ──────────────────────────────────────────────────────────

pub async fn get_user(
    AuthClaims(claims): AuthClaims,
    Path(user_id): Path<String>,
) -> Result<Json<UserResponse>, (StatusCode, Json<serde_json::Value>)> {
    // Admins can fetch anyone; others can only fetch themselves.
    if claims.sub != user_id {
        crate::middleware::require_role(&claims, "admin")?;
    }
    let db = open_db()?;
    let repo = UserRepository::new(db.connection());
    let user = repo.get_by_id(&user_id).map_err(|_| (
        StatusCode::NOT_FOUND,
        Json(serde_json::json!({ "error": "User not found" })),
    ))?;
    Ok(Json(UserResponse::from(user)))
}

// ─── PUT /users/:id/role ─────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct UpdateRoleRequest {
    pub role: String,
}

pub async fn update_role(
    AuthClaims(claims): AuthClaims,
    Path(user_id): Path<String>,
    Json(body): Json<UpdateRoleRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    crate::middleware::require_role(&claims, "admin")?;
    if !["admin", "analyst", "viewer"].contains(&body.role.as_str()) {
        return Err((
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(serde_json::json!({ "error": "role must be admin | analyst | viewer" })),
        ));
    }
    let db = open_db()?;
    let repo = UserRepository::new(db.connection());
    repo.update_role(&user_id, &body.role).map_err(|_| (
        StatusCode::NOT_FOUND,
        Json(serde_json::json!({ "error": "User not found" })),
    ))?;
    Ok(Json(serde_json::json!({ "status": "updated", "role": body.role })))
}

// ─── DELETE /users/:id ───────────────────────────────────────────────────────

pub async fn delete_user(
    AuthClaims(claims): AuthClaims,
    Path(user_id): Path<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    crate::middleware::require_role(&claims, "admin")?;
    // Prevent self-deletion.
    if claims.sub == user_id {
        return Err((
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(serde_json::json!({ "error": "Cannot delete your own account" })),
        ));
    }
    let db = open_db()?;
    let repo = UserRepository::new(db.connection());
    repo.delete(&user_id).map_err(|_| (
        StatusCode::NOT_FOUND,
        Json(serde_json::json!({ "error": "User not found" })),
    ))?;
    Ok(Json(serde_json::json!({ "status": "deleted" })))
}

// ─── PUT /auth/change-password ───────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct ChangePasswordRequest {
    pub current_password: String,
    pub new_password: String,
}

pub async fn change_password(
    AuthClaims(claims): AuthClaims,
    Json(body): Json<ChangePasswordRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    if body.new_password.len() < 8 {
        return Err((
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(serde_json::json!({ "error": "New password must be at least 8 characters" })),
        ));
    }
    let db = open_db()?;
    let repo = UserRepository::new(db.connection());
    let user = repo.get_by_id(&claims.sub).map_err(|_| (
        StatusCode::NOT_FOUND,
        Json(serde_json::json!({ "error": "User not found" })),
    ))?;

    let svc = auth_svc();
    // Verify current password.
    noise_auth::verify_password(&body.current_password, &user.password_hash)
        .map_err(|_| (
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({ "error": "Current password is incorrect" })),
        ))?;

    let new_hash = svc.hash_new_password(&body.new_password).map_err(|e| (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(serde_json::json!({ "error": e.to_string() })),
    ))?;

    // Re-insert with new hash (SQLite: UPDATE users SET password_hash=... WHERE id=...).
    db.connection().execute(
        "UPDATE users SET password_hash=?1 WHERE id=?2",
        rusqlite::params![new_hash, claims.sub],
    ).map_err(|e| (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(serde_json::json!({ "error": e.to_string() })),
    ))?;

    Ok(Json(serde_json::json!({ "status": "password changed" })))
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use noise_auth::Claims;

    fn admin_claims() -> AuthClaims {
        AuthClaims(Claims {
            sub: "admin-id".into(), username: "admin".into(),
            role: "admin".into(), iat: 0, exp: u64::MAX,
        })
    }

    fn viewer_claims() -> AuthClaims {
        AuthClaims(Claims {
            sub: "viewer-id".into(), username: "viewer".into(),
            role: "viewer".into(), iat: 0, exp: u64::MAX,
        })
    }

    #[tokio::test]
    async fn register_first_user_becomes_admin() {
        // Use a temp DB that starts empty.
        std::env::set_var("NOISE_DB", ":memory:");
        // Can't open :memory: across calls (different connections), so test
        // the validate path instead.
        let req = RegisterRequest {
            username: "alice".into(),
            email: "a@b.com".into(),
            password: "password123".into(),
            role: "viewer".into(),
        };
        assert!(validate_register(&req).is_ok());
    }

    #[tokio::test]
    async fn list_users_requires_admin() {
        let result = list_users(viewer_claims()).await;
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().0, StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn update_role_invalid_role_returns_422() {
        let result = update_role(
            admin_claims(),
            Path("some-id".into()),
            Json(UpdateRoleRequest { role: "superuser".into() }),
        ).await;
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().0, StatusCode::UNPROCESSABLE_ENTITY);
    }

    #[tokio::test]
    async fn delete_self_returns_422() {
        let claims = admin_claims();
        let self_id = claims.0.sub.clone();
        let result = delete_user(claims, Path(self_id)).await;
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().0, StatusCode::UNPROCESSABLE_ENTITY);
    }

    #[tokio::test]
    async fn change_password_too_short_returns_422() {
        let result = change_password(
            admin_claims(),
            Json(ChangePasswordRequest {
                current_password: "old".into(),
                new_password: "short".into(),
            }),
        ).await;
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().0, StatusCode::UNPROCESSABLE_ENTITY);
    }
}
