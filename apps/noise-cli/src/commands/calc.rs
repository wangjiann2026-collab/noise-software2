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

    /// Evening source-power offset (dB) relative to daytime; used for Lden/Ldn.
    /// Negative = quieter evening.  Default: 0.
    #[arg(long, default_value_t = 0.0)]
    pub evening_offset_db: f64,

    /// Night source-power offset (dB) relative to daytime; used for Lden/Ldn.
    /// Negative = reduced night traffic (e.g. −3 for road, −6 for rail).
    #[arg(long, default_value_t = 0.0)]
    pub night_offset_db: f64,

    /// Number of CPU threads (0 = all available).
    #[arg(long, default_value_t = 0)]
    pub threads: usize,

    /// Database file path (defaults to <project>.db).
    #[arg(long)]
    pub db: Option<String>,

    /// Grid extent: xmin,ymin,xmax,ymax (defaults to 0,0,100,100).
    #[arg(long)]
    pub extent: Option<String>,
}

#[derive(clap::ValueEnum, Clone, Debug)]
pub enum GridType {
    Horizontal,
    Vertical,
    Facade,
}

pub async fn run(args: CalcArgs) -> anyhow::Result<()> {
    use noise_core::grid::{GridCalculator, MultiPeriodConfig, MultiPeriodGridCalculator,
                           SourceSpec, CalculatorConfig, HorizontalGrid};
    use noise_core::engine::PropagationConfig;
    use noise_core::metrics::{compute_exposure, EU_END_THRESHOLDS};
    use noise_core::parallel::{ParallelScheduler, SchedulerConfig};
    use noise_data::db::Database;
    use noise_data::repository::{CalculationRepository, ProjectRepository, SceneObjectRepository};
    use noise_data::scenario::Project;
    use noise_data::entities::ObjectType;
    use nalgebra::Point3;
    use std::sync::Arc;

    println!("Noise Calculation");
    println!("  Project   : {}", args.project);
    println!("  Grid      : {:?}", args.grid);
    println!("  Metric    : {}", args.metric);
    println!("  Resolution: {} m", args.resolution);

    let sched = ParallelScheduler::new(SchedulerConfig {
        num_threads: args.threads,
        chunk_size: 64,
    });
    println!("  Threads   : {}", sched.num_threads());

    // Load project.
    let proj: Project = serde_json::from_str(&std::fs::read_to_string(&args.project)?)?;
    let sid = args.scenario
        .unwrap_or_else(|| proj.base_scenario.id.to_string());

    // Parse grid extent.
    let (xmin, ymin, xmax, ymax) = if let Some(ext) = &args.extent {
        let parts: Vec<f64> = ext.split(',').filter_map(|s| s.trim().parse().ok()).collect();
        if parts.len() != 4 {
            anyhow::bail!("--extent must be 'xmin,ymin,xmax,ymax'");
        }
        (parts[0], parts[1], parts[2], parts[3])
    } else {
        (0.0, 0.0, 100.0, 100.0)
    };

    let nx = ((xmax - xmin) / args.resolution).ceil() as usize;
    let ny = ((ymax - ymin) / args.resolution).ceil() as usize;
    println!("  Grid size : {nx}×{ny} cells");

    // Load sources from DB (if available), else use a demo source.
    let db_path = args.db.unwrap_or_else(|| args.project.replace(".nsp", ".db"));
    let sources: Vec<SourceSpec> = if std::path::Path::new(&db_path).exists() {
        let db = Database::open(&db_path)?;
        ProjectRepository::new(db.connection()).insert(&proj)?;
        let repo = SceneObjectRepository::new(db.connection());
        let objs = repo.list(&sid, Some(ObjectType::RoadSource))?;
        objs.iter().enumerate().map(|(i, (_, _obj))| {
            // Use centroid at scene origin for now; a full implementation would
            // read geometry from the object's stored properties.
            SourceSpec {
                id: i as u64 + 1,
                position: Point3::new(0.0, 0.0, 0.5),
                lw_db: [80.0; 8],
                g_source: 0.0,
            }
        }).collect()
    } else {
        // Demo: single point source at scene centre.
        vec![SourceSpec {
            id: 1,
            position: Point3::new(
                (xmin + xmax) / 2.0,
                (ymin + ymax) / 2.0,
                0.5,
            ),
            lw_db: [80.0; 8],
            g_source: 0.0,
        }]
    };
    println!("  Sources   : {}", sources.len());

    // Build grid and run calculation.
    let mut grid = HorizontalGrid::new(
        1,
        "calc_grid",
        Point3::new(xmin, ymin, 0.0),
        args.resolution,
        args.resolution,
        nx,
        ny,
        4.0,
    );

    let cfg = CalculatorConfig {
        propagation: PropagationConfig::default(),
        g_receiver: 0.0,
        g_middle: 0.0,
        max_source_range_m: None,
    };

    let is_multi_period = matches!(args.metric.as_str(), "Lden" | "Ldn");
    let peak: f32;
    if is_multi_period {
        println!("  Mode      : multi-period ({}) — EU 2002/49/EC", args.metric);
        let period_cfg = MultiPeriodConfig {
            evening_source_offset_db: args.evening_offset_db,
            night_source_offset_db:   args.night_offset_db,
            ..Default::default()
        };
        let mp = MultiPeriodGridCalculator::new(cfg, period_cfg);
        if args.metric == "Lden" {
            mp.calculate_lden(&mut grid, &sources, &[]);
        } else {
            mp.calculate_ldn(&mut grid, &sources, &[]);
        }
        peak = grid.results.iter().copied()
            .filter(|v| v.is_finite())
            .fold(f32::NEG_INFINITY, f32::max);
    } else {
        let total_cells = nx * ny;
        let progress: Option<Arc<dyn Fn(usize, usize) + Send + Sync>> = Some(Arc::new(
            move |done: usize, total: usize| {
                if total > 0 && done % (total / 10).max(1) == 0 {
                    let pct = done * 100 / total;
                    eprint!("\r  Progress : {pct:3}%");
                }
            }
        ));
        peak = GridCalculator::new(cfg).calculate(&mut grid, &sources, &[], progress) as f32;
        eprintln!("\r  Progress : 100%");
        let _ = total_cells;
    }
    println!("  Peak level: {peak:.1} dBA");
    let cells_calculated = grid.results.len();
    println!("  Calculated: {cells_calculated} cells");

    // Compute mean of finite cells.
    let finite: Vec<f64> = grid.results.iter()
        .filter(|&&v| v > 0.0)
        .map(|&v| v as f64)
        .collect();
    let mean = if finite.is_empty() { 0.0 }
               else { finite.iter().sum::<f64>() / finite.len() as f64 };
    println!("  Mean level: {mean:.1} dBA");

    // Exposure statistics.
    let exposure = compute_exposure(&grid.results, &EU_END_THRESHOLDS);
    println!("  Exposure  : {} valid receivers", exposure.valid_receivers);
    for exc in &exposure.above_thresholds {
        println!("    > {:.0} dBA : {} ({:.1}%)",
            exc.threshold_db, exc.count_above, exc.pct_above);
    }

    // Store in DB.
    let levels_json: Vec<f32> = grid.results.clone();
    let data = serde_json::json!({ "grid": levels_json, "nx": nx, "ny": ny });
    let db = Database::open(&db_path)?;
    ProjectRepository::new(db.connection()).insert(&proj)?;
    let repo = CalculationRepository::new(db.connection());
    let calc_id = repo.insert(&sid,
        &format!("{:?}", args.grid).to_lowercase(),
        &args.metric,
        &data)?;
    println!("  Stored    : calculation_id={calc_id} in {db_path}");
    println!("  Status    : Calculation complete.");
    Ok(())
}
