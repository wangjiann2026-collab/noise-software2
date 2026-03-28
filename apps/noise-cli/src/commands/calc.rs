use clap::Args;

#[derive(Args)]
pub struct CalcArgs {
    /// Project file path.
    #[arg(short, long)]
    pub project: String,

    /// Scenario ID (UUID). Defaults to base scenario.
    #[arg(long)]
    pub scenario: Option<String>,

    /// Grid type: horizontal, vertical, facade.
    #[arg(long, default_value = "horizontal")]
    pub grid: GridType,

    /// Noise metric: Ld, Le, Ln, Lden, Ldn, L10, L1hmax, custom.
    #[arg(long, default_value = "Lden")]
    pub metric: String,

    /// Custom formula (when --metric=custom). Variables: Ld, Le, Ln, Leq.
    #[arg(long)]
    pub formula: Option<String>,

    /// Grid resolution in metres.
    #[arg(long, default_value_t = 10.0)]
    pub resolution: f64,

    /// Number of CPU threads (0 = all available).
    #[arg(long, default_value_t = 0)]
    pub threads: usize,

    /// Output file for results (JSON).
    #[arg(short = 'f', long = "file")]
    pub file: Option<String>,
}

#[derive(clap::ValueEnum, Clone, Debug)]
pub enum GridType {
    Horizontal,
    Vertical,
    Facade,
}

pub async fn run(args: CalcArgs) -> anyhow::Result<()> {
    use noise_core::parallel::{ParallelScheduler, SchedulerConfig};

    println!("Noise calculation");
    println!("  Project  : {}", args.project);
    println!("  Grid     : {:?}", args.grid);
    println!("  Metric   : {}", args.metric);
    println!("  Resolution: {} m", args.resolution);

    let sched = ParallelScheduler::new(SchedulerConfig {
        num_threads: args.threads,
        chunk_size: 64,
    });
    println!("  Threads  : {}", sched.num_threads());
    println!("  Status   : Engine initialised — full calculation in Phase 4.");
    Ok(())
}
