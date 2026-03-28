use clap::Args;

#[derive(Args)]
pub struct ImportArgs {
    /// Input file path.
    #[arg(short, long)]
    pub input: String,

    /// File format (auto-detected from extension if omitted).
    #[arg(long)]
    pub format: Option<String>,

    /// Target project file.
    #[arg(short, long)]
    pub project: String,

    /// Target scenario ID (defaults to base scenario).
    #[arg(long)]
    pub scenario: Option<String>,
}

pub async fn run(args: ImportArgs) -> anyhow::Result<()> {
    use noise_io::import::detect_format;

    let fmt = args.format
        .as_deref()
        .or_else(|| detect_format(&args.input))
        .unwrap_or("unknown");

    println!("Import");
    println!("  File   : {}", args.input);
    println!("  Format : {}", fmt);
    println!("  Project: {}", args.project);
    println!("  Status : Parser not yet implemented — available in Phase 6.");
    Ok(())
}
