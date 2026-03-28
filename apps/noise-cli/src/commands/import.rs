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

    /// Print imported object list.
    #[arg(long)]
    pub list: bool,
}

pub async fn run(args: ImportArgs) -> anyhow::Result<()> {
    use noise_io::import::{detect_format, ImportError};
    use noise_io::import::dxf::import_dxf;
    use noise_io::import::geojson::import_geojson;
    use noise_io::import::shapefile::import_shapefile;
    use noise_io::import::xml::import_xml;
    use noise_io::import::ascii::import_ascii;

    let fmt = args.format
        .as_deref()
        .or_else(|| detect_format(&args.input))
        .unwrap_or("unknown");

    println!("Import");
    println!("  File   : {}", args.input);
    println!("  Format : {fmt}");
    println!("  Project: {}", args.project);

    let scene = match fmt {
        "dxf"       => import_dxf(&args.input)
                        .map_err(|e| anyhow::anyhow!("{e}"))?,
        "geojson"   => import_geojson(&args.input)
                        .map_err(|e| anyhow::anyhow!("{e}"))?,
        "shapefile" => import_shapefile(&args.input)
                        .map_err(|e| anyhow::anyhow!("{e}"))?,
        "xml"       => import_xml(&args.input)
                        .map_err(|e| anyhow::anyhow!("{e}"))?,
        "ascii"     => {
            // ASCII grids contain elevation/noise data, not scene objects.
            let grid = import_ascii(&args.input)
                .map_err(|e| anyhow::anyhow!("{e}"))?;
            println!("  ASCII grid: {}×{} cells, cell size {:.1} m",
                grid.ncols, grid.nrows, grid.cellsize);
            println!("  CRS       : (not embedded in ASCII format)");
            println!("  Status    : ASCII grid loaded — use 'export' to convert.");
            return Ok(());
        }
        other => {
            anyhow::bail!("Unsupported or unrecognised format: '{other}'. \
                Supported: dxf, geojson, shapefile, ascii, xml");
        }
    };

    println!("  Objects : {}", scene.total());
    if let Some(epsg) = scene.crs_epsg {
        println!("  CRS     : EPSG:{epsg}");
    }

    if args.list {
        use noise_io::import::ObjectKind;
        println!();
        println!("{:<6} {:<12} {}", "ID", "Kind", "Label");
        println!("{}", "-".repeat(50));
        for obj in &scene.objects {
            println!("{:<6} {:<12} {}", obj.id, format!("{:?}", obj.kind), obj.label);
        }
    }

    // Summary by kind.
    use noise_io::import::ObjectKind;
    let counts = [
        ("Building",   ObjectKind::Building),
        ("Barrier",    ObjectKind::Barrier),
        ("Road",       ObjectKind::Road),
        ("Rail",       ObjectKind::Rail),
        ("Receiver",   ObjectKind::Receiver),
        ("GroundZone", ObjectKind::GroundZone),
    ];
    println!();
    for (label, kind) in &counts {
        let n = scene.count_by_kind(*kind);
        if n > 0 { println!("  {label:<12}: {n}"); }
    }

    println!("  Status  : Import complete.");
    Ok(())
}
