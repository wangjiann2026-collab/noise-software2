//! # noise-api
//!
//! Axum-based REST API server for the noise mapping platform.
//!
//! ## Routes
//!
//! | Method | Path                            | Description              |
//! |--------|---------------------------------|--------------------------|
//! | POST   | /auth/login                     | Authenticate user        |
//! | POST   | /auth/verify                    | Verify JWT token         |
//! | GET    | /projects                       | List projects            |
//! | POST   | /projects                       | Create project           |
//! | GET    | /projects/:id                   | Get project info         |
//! | GET    | /projects/:id/scenarios         | List scenarios           |
//! | POST   | /scenarios/:id/calculate        | Submit calculation job   |
//! | GET    | /jobs/:id                       | Get job status/result    |
//! | POST   | /mcp/v1/tools/list              | MCP tool listing         |
//! | POST   | /mcp/v1/tools/call              | MCP tool invocation      |

use axum::{Json, Router, routing::{get, post}};
use serde_json::{Value, json};
use tower_http::cors::CorsLayer;
use tracing_subscriber::{EnvFilter, fmt};

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

/// Build the full application router (extracted for testability).
pub fn build_router() -> Router {
    Router::new()
        // System
        .route("/health", get(health))
        .route("/info",   get(info))
        // Auth
        .route("/auth/login",  post(routes::auth::login))
        .route("/auth/verify", post(routes::auth::verify))
        // Projects
        .route("/projects",     get(routes::projects::list_projects)
                               .post(routes::projects::create_project))
        .route("/projects/:id", get(routes::projects::get_project))
        .route("/projects/:id/scenarios", get(routes::projects::list_scenarios))
        // Calculations
        .route("/scenarios/:id/calculate", post(routes::calculate::submit_calculate))
        .route("/jobs/:id",                get(routes::calculate::get_job))
        // MCP
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
