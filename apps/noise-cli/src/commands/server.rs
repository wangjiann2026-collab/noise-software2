use clap::Args;

#[derive(Args)]
pub struct ServerArgs {
    /// Bind address.
    #[arg(long, default_value = "0.0.0.0")]
    pub host: String,

    /// REST API port.
    #[arg(long, default_value_t = 8080)]
    pub port: u16,

    /// Enable MCP server on a separate port.
    #[arg(long, default_value_t = 8081)]
    pub mcp_port: u16,

    /// JWT secret key (use env var NOISE_JWT_SECRET in production).
    #[arg(long, env = "NOISE_JWT_SECRET", default_value = "change-me-in-production")]
    pub jwt_secret: String,

    /// Database file path.
    #[arg(long, default_value = "noise.db")]
    pub db: String,
}

pub async fn run(args: ServerArgs) -> anyhow::Result<()> {
    println!("Starting Noise Platform Server");
    println!("  REST API : http://{}:{}", args.host, args.port);
    println!("  MCP      : http://{}:{}", args.host, args.mcp_port);
    println!("  Database : {}", args.db);
    println!("  Status   : Full server implementation in Phase 6.");
    // Phase 6: axum Router with auth middleware, REST routes, MCP routes.
    Ok(())
}
