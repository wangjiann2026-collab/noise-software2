//! # noise-api
//!
//! Axum-based REST API server for the noise mapping platform.
//!
//! ## Routes (stub — full implementation in Phase 6)
//!
//! | Method | Path                            | Description              |
//! |--------|---------------------------------|--------------------------|
//! | POST   | /auth/login                     | Authenticate user        |
//! | GET    | /projects                       | List projects            |
//! | POST   | /projects                       | Create project           |
//! | GET    | /projects/:id/scenarios         | List scenarios           |
//! | POST   | /projects/:id/scenarios         | Create scenario variant  |
//! | POST   | /scenarios/:id/calculate        | Submit calculation job   |
//! | GET    | /jobs/:id                       | Get job status/result    |
//! | POST   | /mcp/v1/tools/list              | MCP tool listing         |
//! | POST   | /mcp/v1/tools/call              | MCP tool invocation      |

use axum::{Json, Router, routing::get};
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

    let app = Router::new()
        .route("/health", get(health))
        .route("/info",   get(info))
        .merge(noise_mcp::server::router())
        .layer(CorsLayer::permissive());

    let addr = "0.0.0.0:8080";
    let listener = tokio::net::TcpListener::bind(addr).await?;
    tracing::info!("noise-api listening on {addr}");
    axum::serve(listener, app).await?;
    Ok(())
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
