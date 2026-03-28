//! noise-api library crate.
//!
//! Exposes [`build_router`] and [`AppState`] so integration tests
//! (in `tests/`) can construct the full Axum router without binding a TCP port.

use axum::{Json, Router, routing::{delete, get, post, put}};
use axum::middleware as axum_mw;
use serde_json::{Value, json};
use tower_http::cors::CorsLayer;

pub mod middleware;
pub mod routes;
pub mod state;

pub use state::AppState;

/// Build the full application router with shared state.
pub fn build_router(state: AppState) -> Router {
    // Routes that require a valid JWT (any role).
    let authenticated = Router::new()
        .route("/auth/change-password",  put(routes::users::change_password))
        .route("/users/:id",             get(routes::users::get_user))
        .route("/projects",              get(routes::projects::list_projects))
        .route("/projects/:id",          get(routes::projects::get_project))
        .route("/projects/:id/scenarios", get(routes::projects::list_scenarios))
        .route("/jobs/:id",              get(routes::calculate::get_job))
        // Scene object routes.
        .route(
            "/projects/:pid/scenarios/:sid/objects",
            get(routes::objects::list_objects),
        )
        .route(
            "/projects/:pid/scenarios/:sid/objects/:oid",
            get(routes::objects::get_object),
        )
        // Render endpoints (no extra auth — JWT is already required by this layer).
        .route(
            "/projects/:pid/scenarios/:sid/render/png",
            get(routes::render::render_png),
        )
        .route(
            "/projects/:pid/scenarios/:sid/render/svg",
            get(routes::render::render_svg),
        )
        .route(
            "/projects/:pid/scenarios/:sid/render/stats",
            get(routes::render::render_stats),
        )
        .layer(axum_mw::from_fn(middleware::auth::auth_layer));

    // Routes that require analyst or admin.
    let analyst_routes = Router::new()
        .route("/projects",              post(routes::projects::create_project))
        .route("/scenarios/:id/calculate", post(routes::calculate::submit_calculate))
        // Scene object mutation routes.
        .route(
            "/projects/:pid/scenarios/:sid/objects",
            post(routes::objects::create_object),
        )
        .route(
            "/projects/:pid/scenarios/:sid/objects/:oid",
            put(routes::objects::update_object)
                .delete(routes::objects::delete_object),
        )
        .layer(axum_mw::from_fn(middleware::auth::auth_layer));

    // Routes that require admin.
    let admin_routes = Router::new()
        .route("/users",          get(routes::users::list_users))
        .route("/users/:id/role", put(routes::users::update_role))
        .route("/users/:id",      delete(routes::users::delete_user))
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
        // MCP (public — AI agents authenticate via separate mechanism).
        // Convert stateless Router<()> to Router<AppState> for merging.
        .merge(noise_mcp::server::router().with_state(()))
        .layer(CorsLayer::permissive())
        .with_state(state)
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
