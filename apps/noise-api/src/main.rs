//! # noise-api
//!
//! Axum-based REST API server for the noise mapping platform.
//!
//! ## Routes
//!
//! | Method | Path                            | Auth required | Description              |
//! |--------|---------------------------------|---------------|--------------------------|
//! | GET    | /health                         | —             | Health check             |
//! | GET    | /info                           | —             | Platform info            |
//! | POST   | /auth/login                     | —             | Obtain JWT token         |
//! | POST   | /auth/verify                    | —             | Verify JWT token         |
//! | POST   | /auth/register                  | admin*        | Register new user        |
//! | PUT    | /auth/change-password           | any           | Change own password      |
//! | GET    | /users                          | admin         | List all users           |
//! | GET    | /users/:id                      | admin/self    | Get user                 |
//! | PUT    | /users/:id/role                 | admin         | Update user role         |
//! | DELETE | /users/:id                      | admin         | Delete user              |
//! | GET    | /projects                       | any           | List projects            |
//! | POST   | /projects                       | analyst+      | Create project           |
//! | GET    | /projects/:id                   | any           | Get project              |
//! | GET    | /projects/:id/scenarios         | any           | List scenarios           |
//! | POST   | /scenarios/:id/calculate        | analyst+      | Submit calculation       |
//! | GET    | /jobs/:id                       | any           | Get job status           |
//! | POST   | /mcp/v1/tools/list              | —             | MCP tool listing         |
//! | POST   | /mcp/v1/tools/call              | —             | MCP tool invocation      |
//!
//! *First registration is open (bootstraps the initial admin).

use axum::{Json, Router, routing::{delete, get, post, put}};
use axum::middleware as axum_mw;
use serde_json::{Value, json};
use tower_http::cors::CorsLayer;
use tracing_subscriber::{EnvFilter, fmt};

mod middleware;
mod routes;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .with_target(false)
        .init();

    let app = build_router();
    let addr = "0.0.0.0:8080";
    let listener = tokio::net::TcpListener::bind(addr).await?;
    tracing::info!("noise-api listening on {addr}");
    axum::serve(listener, app).await?;
    Ok(())
}

/// Build the full application router.
pub fn build_router() -> Router {
    // Routes that require a valid JWT (any role).
    let authenticated = Router::new()
        .route("/auth/change-password", put(routes::users::change_password))
        .route("/users/:id",            get(routes::users::get_user))
        .route("/projects",             get(routes::projects::list_projects))
        .route("/projects/:id",         get(routes::projects::get_project))
        .route("/projects/:id/scenarios", get(routes::projects::list_scenarios))
        .route("/jobs/:id",             get(routes::calculate::get_job))
        .layer(axum_mw::from_fn(middleware::auth::auth_layer));

    // Routes that require analyst or admin.
    let analyst_routes = Router::new()
        .route("/projects",              post(routes::projects::create_project))
        .route("/scenarios/:id/calculate", post(routes::calculate::submit_calculate))
        .layer(axum_mw::from_fn(middleware::auth::auth_layer));

    // Routes that require admin.
    let admin_routes = Router::new()
        .route("/users",         get(routes::users::list_users))
        .route("/users/:id/role", put(routes::users::update_role))
        .route("/users/:id",     delete(routes::users::delete_user))
        .layer(axum_mw::from_fn(middleware::auth::auth_layer));

    Router::new()
        // Public
        .route("/health",        get(health))
        .route("/info",          get(info))
        .route("/auth/login",    post(routes::auth::login))
        .route("/auth/verify",   post(routes::auth::verify))
        .route("/auth/register", post(routes::users::register))
        // Protected groups
        .merge(authenticated)
        .merge(analyst_routes)
        .merge(admin_routes)
        // MCP (public — AI agents authenticate via a separate mechanism)
        .merge(noise_mcp::server::router())
        .layer(CorsLayer::permissive())
}

async fn health() -> Json<Value> {
    Json(json!({ "status": "ok", "service": "noise-api" }))
}

async fn info() -> Json<Value> {
    Json(json!({
        "version": env!("CARGO_PKG_VERSION"),
        "engine": "noise-core",
        "propagation_models": ["ISO 9613-2", "CNOSSOS-EU"],
        "max_reflection_order": noise_core::engine::ray_tracer::MAX_REFLECTION_ORDER,
    }))
}
