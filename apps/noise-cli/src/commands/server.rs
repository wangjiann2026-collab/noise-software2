use clap::Args;

#[derive(Args)]
pub struct ServerArgs {
    /// Bind address.
    #[arg(long, default_value = "0.0.0.0")]
    pub host: String,

    /// REST API port.
    #[arg(long, default_value_t = 8080)]
    pub port: u16,

    /// JWT secret key (use env var NOISE_JWT_SECRET in production).
    #[arg(long, env = "NOISE_JWT_SECRET", default_value = "change-me-in-production")]
    pub jwt_secret: String,

    /// Database file path.
    #[arg(long, default_value = "noise.db")]
    pub db: String,
}

pub async fn run(args: ServerArgs) -> anyhow::Result<()> {
    use axum::{Json, Router, routing::get};
    use serde_json::{json, Value};
    use tower_http::cors::CorsLayer;

    let addr = format!("{}:{}", args.host, args.port);

    println!("Starting Noise Platform Server");
    println!("  REST API : http://{addr}");
    println!("  MCP      : http://{addr}/mcp/v1");
    println!("  Database : {}", args.db);

    let app = Router::new()
        .route("/health", get(health))
        .route("/info",   get(info))
        .merge(noise_mcp::server::router())
        .layer(CorsLayer::permissive());

    let listener = tokio::net::TcpListener::bind(&addr).await?;
    tracing::info!("noise server listening on {addr}");
    axum::serve(listener, app).await?;
    Ok(())
}

async fn health() -> axum::Json<serde_json::Value> {
    axum::Json(serde_json::json!({ "status": "ok", "service": "noise-platform" }))
}

async fn info() -> axum::Json<serde_json::Value> {
    axum::Json(serde_json::json!({
        "version": env!("CARGO_PKG_VERSION"),
        "engine": "noise-core",
        "propagation_models": ["ISO 9613-2", "CNOSSOS-EU"],
    }))
}
