use clap::Args;

#[derive(Args)]
pub struct ExportArgs {
    /// Project file.
    #[arg(short, long)]
    pub project: String,

    /// Calculation result ID to export.
    #[arg(long)]
    pub calc_id: Option<u64>,

    /// Export type: ascii, geojson, csv, report-md, report-txt.
    #[arg(long, default_value = "ascii")]
    pub export_type: String,

    /// Output file path.
    #[arg(short = 'f', long = "file")]
    pub file: String,

    /// Database file path (defaults to <project>.db).
    #[arg(long)]
    pub db: Option<String>,

    /// Grid cell size in metres (for synthetic export without DB).
    #[arg(long, default_value_t = 10.0)]
    pub cellsize: f64,

    /// Grid origin X (xllcorner).
    #[arg(long, default_value_t = 0.0)]
    pub xllcorner: f64,

    /// Grid origin Y (yllcorner).
    #[arg(long, default_value_t = 0.0)]
    pub yllcorner: f64,
}

pub async fn run(args: ExportArgs) -> anyhow::Result<()> {
    use noise_data::db::Database;
    use noise_data::repository::{CalculationRepository, ProjectRepository};
    use noise_data::scenario::Project;
    use noise_export::{GridView, export_asc, export_csv, export_geojson};
    use noise_io::export::report::{GridStats, NoiseReport, SourceReport};

    let db_path = args.db.unwrap_or_else(|| args.project.replace(".nsp", ".db"));
    let proj: Project = serde_json::from_str(&std::fs::read_to_string(&args.project)?)?;

    println!("Export");
    println!("  Type   : {}", args.export_type);
    println!("  Output : {}", args.file);

    // If calc_id provided, load from DB; otherwise create a trivial demo grid.
    let (levels, nx, ny): (Vec<f32>, usize, usize) = if let Some(cid) = args.calc_id {
        let db = Database::open(&db_path)?;
        ProjectRepository::new(db.connection()).insert(&proj)?;
        let repo = CalculationRepository::new(db.connection());
        let result = repo.get(cid as i64)
            .map_err(|e| anyhow::anyhow!("Calculation {cid} not found: {e}"))?;
        println!("  Calc ID: {} ({})", result.id, result.metric);
        let raw: Vec<f32> = serde_json::from_value(result.data)?;
        let n = (raw.len() as f64).sqrt().ceil() as usize;
        let nx = n.max(1);
        let ny = (raw.len() + nx - 1) / nx;
        (raw, nx, ny)
    } else {
        // No calc_id — generate a 4×4 demo grid
        println!("  Note   : No --calc-id provided; using demo 4×4 grid.");
        let demo: Vec<f32> = (0u32..16).map(|i| 50.0 + i as f32 * 1.5).collect();
        (demo, 4, 4)
    };

    // Build a GridView for the noise-export crate functions.
    let view = GridView {
        levels: levels.clone(),
        nx,
        ny,
        xllcorner: args.xllcorner,
        yllcorner: args.yllcorner,
        cellsize: args.cellsize,
    };

    match args.export_type.as_str() {
        "ascii" => {
            let content = export_asc(&view);
            std::fs::write(&args.file, content)?;
            println!("  Cells  : {nx}×{ny}");
            println!("  Status : ESRI ASCII grid written.");
        }
        "geojson" => {
            let value = export_geojson(&view, noise_export::geojson::DEFAULT_LEVELS);
            let content = serde_json::to_string_pretty(&value)?;
            std::fs::write(&args.file, content)?;
            println!("  Cells  : {nx}×{ny}");
            println!("  Status : GeoJSON FeatureCollection written.");
        }
        "csv" => {
            let content = export_csv(&view);
            std::fs::write(&args.file, content)?;
            println!("  Cells  : {nx}×{ny}");
            println!("  Status : CSV written.");
        }
        "report-md" | "report-txt" => {
            let stats = GridStats::from_grid(&levels)
                .map_err(|e| anyhow::anyhow!("{e}"))?;
            let report = NoiseReport {
                project_name: proj.name.clone(),
                scenario_name: proj.base_scenario.name.clone(),
                metric: "Lden".into(),
                grid_stats: stats,
                sources: vec![
                    SourceReport { id: 1, name: "Main Road".into(), lw_dba: 75.0 },
                ],
                thresholds: vec![55.0, 65.0, 70.0],
            };
            if args.export_type == "report-md" {
                report.write_markdown(&levels, &args.file)?;
                println!("  Status : Markdown report written.");
            } else {
                report.write_text(&levels, &args.file)?;
                println!("  Status : Text report written.");
            }
        }
        other => {
            anyhow::bail!("Unknown export type '{other}'. \
                Supported: ascii, geojson, csv, report-md, report-txt");
        }
    }

    Ok(())
}
