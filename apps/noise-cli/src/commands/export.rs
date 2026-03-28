use clap::Args;

#[derive(Args)]
pub struct ExportArgs {
    /// Project file.
    #[arg(short, long)]
    pub project: String,

    /// Calculation result ID to export.
    #[arg(long)]
    pub calc_id: Option<u64>,

    /// Export type: heatmap, report, building-facade.
    #[arg(long, default_value = "heatmap")]
    pub export_type: String,

    /// Output file path.
    #[arg(short = 'f', long = "file")]
    pub file: String,
}

pub async fn run(args: ExportArgs) -> anyhow::Result<()> {
    println!("Export");
    println!("  Type   : {}", args.export_type);
    println!("  Output : {}", args.file);
    println!("  Status : Export not yet implemented — available in Phase 6.");
    Ok(())
}
