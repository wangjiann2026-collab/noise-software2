//! # noise — Command Line Interface
//!
//! Cross-platform CLI for the 3D environmental noise mapping platform.
//!
//! ## Usage
//!
//! ```text
//! noise [COMMAND] [OPTIONS]
//!
//! Commands:
//!   project   Manage projects and scenarios
//!   calc      Run acoustic noise calculations
//!   import    Import scene data (DXF, GIS, ASCII, XML)
//!   export    Export results (noise maps, reports)
//!   server    Start the REST API / MCP server
//!   info      Show system information
//! ```

use clap::{Parser, Subcommand};
use tracing_subscriber::{EnvFilter, fmt};

mod commands;

#[derive(Parser)]
#[command(
    name = "noise",
    about = "3D Environmental Noise Mapping & Acoustic Simulation Platform",
    version,
    author,
    propagate_version = true,
    arg_required_else_help = true,
)]
struct Cli {
    /// Verbosity level (-v, -vv, -vvv).
    #[arg(short, long, action = clap::ArgAction::Count, global = true)]
    verbose: u8,

    /// Output format: text, json.
    #[arg(long, default_value = "text", global = true)]
    output: OutputFormat,

    #[command(subcommand)]
    command: Commands,
}

#[derive(clap::ValueEnum, Clone, Debug)]
enum OutputFormat {
    Text,
    Json,
}

#[derive(Subcommand)]
enum Commands {
    /// Authenticate with a noise platform server (login, logout, whoami, register).
    Auth(commands::auth::AuthArgs),
    /// Manage projects, scenarios, and variants.
    Project(commands::project::ProjectArgs),
    /// Manage scene objects (receivers, sources, buildings, etc.).
    Object(commands::object::ObjectArgs),
    /// Run acoustic noise calculations.
    Calc(commands::calc::CalcArgs),
    /// Import scene data from DXF, Shapefile, GeoJSON, ASCII, or XML.
    Import(commands::import::ImportArgs),
    /// Export calculation results.
    Export(commands::export::ExportArgs),
    /// Start the REST API and MCP server.
    Server(commands::server::ServerArgs),
    /// Show system and platform information.
    Info,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    // Initialize tracing based on verbosity.
    let filter = match cli.verbose {
        0 => "warn",
        1 => "info",
        2 => "debug",
        _ => "trace",
    };
    fmt()
        .with_env_filter(EnvFilter::new(filter))
        .with_target(false)
        .init();

    match cli.command {
        Commands::Auth(args)    => commands::auth::run(args).await,
        Commands::Project(args) => commands::project::run(args).await,
        Commands::Object(args)  => commands::object::run(args).await,
        Commands::Calc(args)    => commands::calc::run(args).await,
        Commands::Import(args)  => commands::import::run(args).await,
        Commands::Export(args)  => commands::export::run(args).await,
        Commands::Server(args)  => commands::server::run(args).await,
        Commands::Info          => commands::info::run(),
    }
}
