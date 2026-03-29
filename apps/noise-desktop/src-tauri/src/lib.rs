//! Tauri backend for the noise-desktop application.
//!
//! Exposes Tauri commands that let the frontend:
//!   - Create / list / get / delete projects
//!   - Add / list / delete scene objects (point sources, barriers, …)
//!   - Run grid calculations (horizontal, Lden/Ldn/single-period)
//!   - Export results as ASC / CSV / GeoJSON

use std::sync::{Arc, Mutex};
use tauri::{Manager, State};
use serde::{Deserialize, Serialize};
use nalgebra::Point3;

use noise_data::{
    db::Database,
    repository::{ProjectRepository, SceneObjectRepository, CalculationRepository},
    scenario::Project,
    entities::{
        SceneObject, Barrier,
        sources::{PointSource, RoadSource, RoadSurface},
    },
};
use noise_core::{
    engine::{PropagationConfig, diffraction::DiffractionEdge},
    grid::{BarrierSpec, CalculatorConfig, GridCalculator, HorizontalGrid,
           MultiPeriodConfig, MultiPeriodGridCalculator, SourceSpec},
};
use noise_export::{GridView, export_asc, export_csv, export_geojson};
use noise_export::geojson::DEFAULT_LEVELS;

// ─── Application state ────────────────────────────────────────────────────────

pub struct AppState {
    db: Arc<Mutex<Database>>,
}

impl AppState {
    pub fn new(db_path: &str) -> anyhow::Result<Self> {
        let db = Database::open(db_path)?;
        Ok(Self {
            db: Arc::new(Mutex::new(db)),
        })
    }
}

// ─── Response / DTO types ─────────────────────────────────────────────────────

/// Lightweight project listing entry.
#[derive(Debug, Serialize, Clone)]
pub struct ProjectSummary {
    pub id: String,
    pub name: String,
    pub crs_epsg: u32,
    pub scenario_count: usize,
}

/// Brief scenario descriptor used inside [`ProjectDetail`].
#[derive(Debug, Serialize, Clone)]
pub struct ScenarioInfo {
    pub id: String,
    pub name: String,
    pub is_base: bool,
}

/// Full project detail including base scenario and all variants.
#[derive(Debug, Serialize, Clone)]
pub struct ProjectDetail {
    pub id: String,
    pub name: String,
    pub crs_epsg: u32,
    pub description: String,
    pub base_scenario: ScenarioInfo,
    pub variants: Vec<ScenarioInfo>,
}

/// Summary row for a single scene object, including geometry for the CAD view.
#[derive(Debug, Serialize, Clone)]
pub struct ObjectInfo {
    pub row_id: i64,
    pub name: String,
    pub object_type: String,
    /// Point source / receiver position [x, y, z].
    pub position: Option<[f64; 3]>,
    /// Broadband sound power level (first octave band, dB) for point sources.
    pub lw_db: Option<f64>,
    /// Polyline vertices [[x,y,z], …] for road / barrier / line objects.
    pub vertices: Option<Vec<[f64; 3]>>,
    /// Barrier height above ground (m).
    pub height_m: Option<f64>,
    /// Road source emission height above ground (m).
    pub source_height_m: Option<f64>,
}

/// Result returned after a grid calculation.
#[derive(Debug, Serialize, Clone)]
pub struct CalcResult {
    pub calc_id: i64,
    pub metric: String,
    pub nx: usize,
    pub ny: usize,
    pub xmin: f64,
    pub ymin: f64,
    pub cellsize: f64,
    pub levels: Vec<f32>,
    pub mean_db: f64,
    pub max_db: f64,
    pub min_db: f64,
}

// ─── Tauri commands ───────────────────────────────────────────────────────────
//
// All commands live in the `commands` submodule.  This avoids the E0255
// "defined multiple times" error that arises when `#[tauri::command]`
// proc-macro-generated helper macros collide with the `generate_handler!`
// import in the same namespace.

pub mod commands {
    use super::*;

/// Create a new project and return its summary.
#[tauri::command]
pub fn new_project(
    state: State<AppState>,
    name: String,
    crs_epsg: u32,
) -> Result<ProjectSummary, String> {
    let project = Project::new(&name, crs_epsg);
    let scenario_count = 1 + project.variants.len(); // base + variants
    let summary = ProjectSummary {
        id: project.id.to_string(),
        name: project.name.clone(),
        crs_epsg: project.crs_epsg,
        scenario_count,
    };
    let db = state.db.lock().map_err(|e| e.to_string())?;
    ProjectRepository::new(db.connection())
        .insert(&project)
        .map_err(|e| e.to_string())?;
    Ok(summary)
}

/// Return a summary list of all projects.
#[tauri::command]
pub fn list_projects(state: State<AppState>) -> Result<Vec<ProjectSummary>, String> {
    let db = state.db.lock().map_err(|e| e.to_string())?;
    let repo = ProjectRepository::new(db.connection());
    let pairs = repo.list_all().map_err(|e| e.to_string())?;
    // To get accurate scenario_count we'd need an extra query; for the list
    // view we load each project individually.  That is acceptable — project
    // counts are typically small.
    let mut summaries = Vec::with_capacity(pairs.len());
    for (uuid, _name) in pairs {
        let project = repo.get(uuid).map_err(|e| e.to_string())?;
        summaries.push(ProjectSummary {
            id: project.id.to_string(),
            name: project.name.clone(),
            crs_epsg: project.crs_epsg,
            scenario_count: 1 + project.variants.len(),
        });
    }
    Ok(summaries)
}

/// Return full detail for a single project including scenarios.
#[tauri::command]
pub fn get_project(
    state: State<AppState>,
    project_id: String,
) -> Result<ProjectDetail, String> {
    let uuid = uuid::Uuid::parse_str(&project_id)
        .map_err(|e| format!("invalid project_id: {e}"))?;
    let db = state.db.lock().map_err(|e| e.to_string())?;
    let project = ProjectRepository::new(db.connection())
        .get(uuid)
        .map_err(|e| e.to_string())?;

    let base = ScenarioInfo {
        id: project.base_scenario.id.to_string(),
        name: project.base_scenario.name.clone(),
        is_base: true,
    };
    let variants: Vec<ScenarioInfo> = project
        .variants
        .iter()
        .map(|v| ScenarioInfo {
            id: v.id.to_string(),
            name: v.name.clone(),
            is_base: false,
        })
        .collect();

    Ok(ProjectDetail {
        id: project.id.to_string(),
        name: project.name.clone(),
        crs_epsg: project.crs_epsg,
        description: project.description.clone(),
        base_scenario: base,
        variants,
    })
}

/// Delete a project (cascades to scenarios, objects, and calculations).
#[tauri::command]
pub fn delete_project(
    state: State<AppState>,
    project_id: String,
) -> Result<(), String> {
    let uuid = uuid::Uuid::parse_str(&project_id)
        .map_err(|e| format!("invalid project_id: {e}"))?;
    let db = state.db.lock().map_err(|e| e.to_string())?;
    ProjectRepository::new(db.connection())
        .delete(uuid)
        .map_err(|e| e.to_string())
}

/// Add an omnidirectional point source to a scenario.
///
/// Returns the auto-assigned database row id.
#[tauri::command]
pub fn add_point_source(
    state: State<AppState>,
    scenario_id: String,
    name: String,
    x: f64,
    y: f64,
    z: f64,
    lw_db: f64,
) -> Result<i64, String> {
    let ps = PointSource::omnidirectional(
        1,
        &name,
        Point3::new(x, y, z),
        [lw_db; 8],
    );
    let obj = SceneObject::PointSource(ps);
    let db = state.db.lock().map_err(|e| e.to_string())?;
    SceneObjectRepository::new(db.connection())
        .insert(&scenario_id, &obj)
        .map_err(|e| e.to_string())
}

/// List all scene objects belonging to a scenario.
#[tauri::command]
pub fn list_objects(
    state: State<AppState>,
    scenario_id: String,
) -> Result<Vec<ObjectInfo>, String> {
    let db = state.db.lock().map_err(|e| e.to_string())?;
    let objects = SceneObjectRepository::new(db.connection())
        .list(&scenario_id, None)
        .map_err(|e| e.to_string())?;
    Ok(objects
        .into_iter()
        .map(|(row_id, obj)| {
            let (position, lw_db, vertices, height_m, source_height_m) = match &obj {
                SceneObject::PointSource(ps) => (
                    Some([ps.position.x, ps.position.y, ps.position.z]),
                    Some(ps.lw_db[0]),
                    None,
                    None,
                    None,
                ),
                SceneObject::RoadSource(rs) => (
                    None,
                    None,
                    Some(rs.vertices.iter().map(|v| [v.x, v.y, v.z]).collect()),
                    None,
                    Some(rs.source_height_m),
                ),
                SceneObject::Barrier(b) => (
                    None,
                    None,
                    Some(b.vertices.iter().map(|v| [v.x, v.y, v.z]).collect()),
                    Some(b.height_m),
                    None,
                ),
                _ => (None, None, None, None, None),
            };
            ObjectInfo {
                row_id,
                name: obj.name().to_owned(),
                object_type: obj.object_type().as_str().to_owned(),
                position,
                lw_db,
                vertices,
                height_m,
                source_height_m,
            }
        })
        .collect())
}

/// Delete a single scene object by its database row id.
#[tauri::command]
pub fn delete_object(
    state: State<AppState>,
    row_id: i64,
) -> Result<(), String> {
    let db = state.db.lock().map_err(|e| e.to_string())?;
    SceneObjectRepository::new(db.connection())
        .delete(row_id)
        .map_err(|e| e.to_string())
}

/// Run a horizontal grid calculation for a scenario and persist the result.
///
/// `metric` selects the noise descriptor: `"Lden"`, `"Ldn"`, or any
/// single-period label recognised by the engine.
#[tauri::command]
pub fn run_calculation(
    state: State<AppState>,
    scenario_id: String,
    metric: String,
    resolution_m: f64,
    xmin: f64,
    ymin: f64,
    xmax: f64,
    ymax: f64,
) -> Result<CalcResult, String> {
    if resolution_m <= 0.0 {
        return Err("resolution_m must be positive".into());
    }

    let nx = ((xmax - xmin) / resolution_m).ceil() as usize;
    let ny = ((ymax - ymin) / resolution_m).ceil() as usize;
    if nx == 0 || ny == 0 {
        return Err("grid extent produces a zero-sized grid".into());
    }

    // ── Load scene objects from DB ────────────────────────────────────────────
    let (sources, barriers): (Vec<SourceSpec>, Vec<BarrierSpec>) = {
        let db = state.db.lock().map_err(|e| e.to_string())?;
        let repo = SceneObjectRepository::new(db.connection());
        let objects = repo
            .list(&scenario_id, None)
            .map_err(|e| e.to_string())?;

        let mut srcs: Vec<SourceSpec> = Vec::new();
        let mut bars: Vec<BarrierSpec> = Vec::new();
        for (_row_id, obj) in &objects {
            append_sources(obj, &mut srcs);
            append_barriers(obj, &mut bars);
        }
        (srcs, bars)
    };

    // Fall back to a demo source at the grid centre when none exist yet.
    let sources = if sources.is_empty() {
        vec![SourceSpec {
            id: 0,
            position: Point3::new((xmin + xmax) / 2.0, ymin + 10.0, 0.5),
            lw_db: [82.0; 8],
            g_source: 0.0,
        }]
    } else {
        sources
    };

    // ── Build grid and run calculator ─────────────────────────────────────────
    let mut grid = HorizontalGrid::new(
        1,
        "desktop_grid",
        Point3::new(xmin, ymin, 0.0),
        resolution_m,
        resolution_m,
        nx,
        ny,
        4.0, // receiver height above ground (m)
    );

    let cfg = CalculatorConfig {
        propagation: PropagationConfig::default(),
        g_receiver: 0.5,
        g_middle: 0.5,
        max_source_range_m: None,
    };

    match metric.as_str() {
        "Lden" => {
            let mp = MultiPeriodGridCalculator::new(cfg, MultiPeriodConfig::default());
            mp.calculate_lden(&mut grid, &sources, &barriers);
        }
        "Ldn" => {
            let mp = MultiPeriodGridCalculator::new(cfg, MultiPeriodConfig::default());
            mp.calculate_ldn(&mut grid, &sources, &barriers);
        }
        _ => {
            GridCalculator::new(cfg).calculate(&mut grid, &sources, &barriers, None);
        }
    }

    let levels = grid.results.clone();

    // ── Persist result ────────────────────────────────────────────────────────
    let data = serde_json::json!({
        "nx": nx,
        "ny": ny,
        "xmin": xmin,
        "ymin": ymin,
        "cellsize": resolution_m,
        "levels": levels,
    });

    let calc_id = {
        let db = state.db.lock().map_err(|e| e.to_string())?;
        CalculationRepository::new(db.connection())
            .insert(&scenario_id, "horizontal", &metric, &data)
            .map_err(|e| e.to_string())?
    };

    // ── Compute statistics ────────────────────────────────────────────────────
    let finite: Vec<f32> = levels
        .iter()
        .copied()
        .filter(|&v| v.is_finite() && v > 0.0)
        .collect();
    let (min_db, max_db, mean_db) = if finite.is_empty() {
        (0.0_f64, 0.0, 0.0)
    } else {
        let min = finite.iter().copied().fold(f32::INFINITY, f32::min) as f64;
        let max = finite.iter().copied().fold(f32::NEG_INFINITY, f32::max) as f64;
        let mean = finite.iter().sum::<f32>() as f64 / finite.len() as f64;
        (min, max, mean)
    };

    Ok(CalcResult {
        calc_id,
        metric,
        nx,
        ny,
        xmin,
        ymin,
        cellsize: resolution_m,
        levels,
        mean_db: (mean_db * 10.0).round() / 10.0,
        max_db:  (max_db  * 10.0).round() / 10.0,
        min_db:  (min_db  * 10.0).round() / 10.0,
    })
}

/// Export a saved calculation result to a file.
///
/// Supported `format` values: `"asc"`, `"csv"`, `"geojson"`.
#[tauri::command]
pub fn export_file(
    state: State<AppState>,
    calc_id: i64,
    format: String,
    file_path: String,
) -> Result<(), String> {
    // Load calculation from DB.
    let cr = {
        let db = state.db.lock().map_err(|e| e.to_string())?;
        CalculationRepository::new(db.connection())
            .get(calc_id)
            .map_err(|e| e.to_string())?
    };

    // Reconstruct grid parameters from stored JSON.
    let nx = cr.data["nx"].as_u64().unwrap_or(0) as usize;
    let ny = cr.data["ny"].as_u64().unwrap_or(0) as usize;
    let xmin = cr.data["xmin"].as_f64().unwrap_or(0.0);
    let ymin = cr.data["ymin"].as_f64().unwrap_or(0.0);
    let cellsize = cr.data["cellsize"].as_f64().unwrap_or(10.0);
    let levels: Vec<f32> = cr.data["levels"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|v| v.as_f64().map(|f| f as f32))
                .collect()
        })
        .unwrap_or_default();

    let view = GridView {
        levels,
        nx,
        ny,
        xllcorner: xmin,
        yllcorner: ymin,
        cellsize,
    };

    let content = match format.to_lowercase().as_str() {
        "asc" => export_asc(&view),
        "csv" => export_csv(&view),
        "geojson" => {
            let json = export_geojson(&view, DEFAULT_LEVELS);
            serde_json::to_string_pretty(&json)
                .map_err(|e| format!("JSON serialisation failed: {e}"))?
        }
        other => return Err(format!("unsupported export format: {other}")),
    };

    std::fs::write(&file_path, content)
        .map_err(|e| format!("failed to write {file_path}: {e}"))
}

/// Add a road source polyline to a scenario.
#[tauri::command]
pub fn add_road_source(
    state: State<AppState>,
    scenario_id: String,
    name: String,
    vertices: Vec<[f64; 3]>,
    source_height_m: f64,
    sample_spacing_m: f64,
) -> Result<i64, String> {
    if vertices.len() < 2 {
        return Err("road source requires at least 2 vertices".into());
    }
    let verts: Vec<Point3<f64>> = vertices.iter()
        .map(|v| Point3::new(v[0], v[1], v[2])).collect();
    let rs = RoadSource {
        id: 1, name,
        vertices: verts,
        traffic_flows: vec![],
        surface: RoadSurface::DenseAsphalt,
        gradient_pct: 0.0,
        source_height_m,
        sample_spacing_m,
    };
    let db = state.db.lock().map_err(|e| e.to_string())?;
    SceneObjectRepository::new(db.connection())
        .insert(&scenario_id, &SceneObject::RoadSource(rs))
        .map_err(|e| e.to_string())
}

/// Add a noise barrier polyline to a scenario.
#[tauri::command]
pub fn add_barrier(
    state: State<AppState>,
    scenario_id: String,
    name: String,
    vertices: Vec<[f64; 3]>,
    height_m: f64,
) -> Result<i64, String> {
    if vertices.len() < 2 {
        return Err("barrier requires at least 2 vertices".into());
    }
    let verts: Vec<Point3<f64>> = vertices.iter()
        .map(|v| Point3::new(v[0], v[1], v[2])).collect();
    let barrier = Barrier::new(1, name, verts, height_m);
    let db = state.db.lock().map_err(|e| e.to_string())?;
    SceneObjectRepository::new(db.connection())
        .insert(&scenario_id, &SceneObject::Barrier(barrier))
        .map_err(|e| e.to_string())
}

/// Update the geometry (position or vertices) of an existing scene object.
///
/// - Point sources: supply `x`, `y`, `z`.
/// - Road / barrier: supply `vertices`.
#[tauri::command]
pub fn update_object_geometry(
    state: State<AppState>,
    row_id: i64,
    x: Option<f64>,
    y: Option<f64>,
    z: Option<f64>,
    vertices: Option<Vec<[f64; 3]>>,
) -> Result<(), String> {
    let db = state.db.lock().map_err(|e| e.to_string())?;
    let repo = SceneObjectRepository::new(db.connection());
    let mut obj = repo.get(row_id).map_err(|e| e.to_string())?;
    match &mut obj {
        SceneObject::PointSource(ps) => {
            if let (Some(nx), Some(ny), Some(nz)) = (x, y, z) {
                ps.position = Point3::new(nx, ny, nz);
            }
        }
        SceneObject::RoadSource(rs) => {
            if let Some(verts) = vertices {
                rs.vertices = verts.iter().map(|v| Point3::new(v[0], v[1], v[2])).collect();
            }
        }
        SceneObject::Barrier(b) => {
            if let Some(verts) = vertices {
                b.vertices = verts.iter().map(|v| Point3::new(v[0], v[1], v[2])).collect();
            }
        }
        _ => return Err("geometry update not supported for this object type".into()),
    }
    repo.update(row_id, &obj).map_err(|e| e.to_string())
}

} // pub mod commands

// ─── Private helpers ──────────────────────────────────────────────────────────

/// Append [`SourceSpec`] entries for a single [`SceneObject`].
///
/// - `PointSource` → one spec.
/// - `RoadSource`  → one spec per uniform sample along the polyline, with the
///   total source power split across all samples (energy-conservative).
/// - All other types are silently ignored here (barriers are handled separately).
fn append_sources(obj: &SceneObject, out: &mut Vec<SourceSpec>) {
    match obj {
        SceneObject::PointSource(ps) => {
            out.push(SourceSpec {
                id: ps.id,
                position: ps.position,
                lw_db: ps.lw_db,
                g_source: 0.5,
            });
        }
        SceneObject::RoadSource(rs) => {
            if rs.vertices.len() < 2 {
                // Degenerate road — emit a single point at the first vertex.
                if let Some(&v) = rs.vertices.first() {
                    out.push(SourceSpec {
                        id: rs.id,
                        position: Point3::new(v.x, v.y, rs.source_height_m),
                        lw_db: [80.0; 8],
                        g_source: 0.0,
                    });
                }
                return;
            }

            let spacing = rs.sample_spacing_m.max(1.0);
            let samples = sample_polyline(&rs.vertices, spacing, rs.source_height_m);
            let n = samples.len() as f64;
            if n == 0.0 {
                return;
            }

            // Energy split: Lw_sample = Lw_road − 10·log10(N).
            let split_offset = -10.0 * n.log10();
            let base_lw = [80.0_f64; 8];
            let sample_lw: [f64; 8] = base_lw.map(|lw| lw + split_offset);

            for (i, pos) in samples.into_iter().enumerate() {
                out.push(SourceSpec {
                    id: rs.id * 10_000 + i as u64 + 1,
                    position: pos,
                    lw_db: sample_lw,
                    g_source: 0.0,
                });
            }
        }
        _ => {}
    }
}

/// Append [`BarrierSpec`] entries for a single [`SceneObject`].
///
/// Each segment of a `Barrier` polyline becomes one `BarrierSpec` whose
/// diffracting edge is placed at the segment midpoint at `height_m`.
fn append_barriers(obj: &SceneObject, out: &mut Vec<BarrierSpec>) {
    if let SceneObject::Barrier(b) = obj {
        for seg in b.vertices.windows(2) {
            let mid = Point3::new(
                (seg[0].x + seg[1].x) * 0.5,
                (seg[0].y + seg[1].y) * 0.5,
                b.height_m,
            );
            out.push(BarrierSpec {
                edge: DiffractionEdge {
                    point: mid,
                    height_m: b.height_m,
                },
            });
        }
    }
}

/// Uniformly sample points along a 3-D polyline at `spacing` metre intervals.
///
/// The returned points all have their z coordinate replaced by `height_z`
/// (source emission height above ground).
fn sample_polyline(
    vertices: &[Point3<f64>],
    spacing: f64,
    height_z: f64,
) -> Vec<Point3<f64>> {
    let mut result = Vec::new();
    let mut accumulated = 0.0_f64;

    // Always emit a point at the very start of the polyline.
    if let Some(&first) = vertices.first() {
        result.push(Point3::new(first.x, first.y, height_z));
    }

    for seg in vertices.windows(2) {
        let dx = seg[1].x - seg[0].x;
        let dy = seg[1].y - seg[0].y;
        let seg_len = (dx * dx + dy * dy).sqrt();
        if seg_len < 1e-9 {
            continue;
        }

        let dir_x = dx / seg_len;
        let dir_y = dy / seg_len;

        // Distance along this segment to the first new sample.
        let mut dist_in_seg = spacing - (accumulated % spacing);
        if accumulated % spacing < 1e-9 {
            dist_in_seg = spacing;
        }

        while dist_in_seg <= seg_len {
            let x = seg[0].x + dir_x * dist_in_seg;
            let y = seg[0].y + dir_y * dist_in_seg;
            result.push(Point3::new(x, y, height_z));
            dist_in_seg += spacing;
        }
        accumulated += seg_len;
    }

    result
}

// ─── Tauri entry point ────────────────────────────────────────────────────────

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_shell::init())
        .setup(|app| {
            let db_dir = app
                .path()
                .app_data_dir()
                .unwrap_or_else(|_| std::path::PathBuf::from("."));
            std::fs::create_dir_all(&db_dir).ok();
            let db_path = db_dir.join("noise.db");
            let state = AppState::new(db_path.to_str().unwrap_or("noise.db"))
                .expect("failed to open database");
            app.manage(state);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::new_project,
            commands::list_projects,
            commands::get_project,
            commands::delete_project,
            commands::add_point_source,
            commands::add_road_source,
            commands::add_barrier,
            commands::update_object_geometry,
            commands::list_objects,
            commands::delete_object,
            commands::run_calculation,
            commands::export_file,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
