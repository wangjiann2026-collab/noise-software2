//! noise-api binary entry-point.
//!
//! All router construction lives in `lib.rs`; this file only handles
//! process-level setup (logging, env-vars, TCP listener).

use tracing_subscriber::{EnvFilter, fmt};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .with_target(false)
        .init();

    let db_path = std::env::var("NOISE_DB").unwrap_or_else(|_| "noise.db".into());
    let state = noise_api::AppState::new(&db_path)?;

    let app = noise_api::build_router(state);
    let addr = "0.0.0.0:8080";
    let listener = tokio::net::TcpListener::bind(addr).await?;
    tracing::info!("noise-api listening on {addr}");
    axum::serve(listener, app).await?;
    Ok(())
}
