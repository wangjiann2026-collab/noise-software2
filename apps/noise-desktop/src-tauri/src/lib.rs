//! Tauri backend for the noise-desktop application.
//!
//! Exposes Tauri commands that let the frontend:
//!   - Create / list / get / delete projects
//!   - Add / list / delete scene objects (point sources, barriers, …)
//!   - Run grid calculations (horizontal, Lden/Ldn/single-period)
//!   - Export results as ASC / CSV / GeoJSON

use std::sync::{Arc, Mutex};
use tauri::{Manager, State};
use serde::Serialize;
use nalgebra::Point3;

use noise_data::{
    db::Database,
    repository::{ProjectRepository, SceneObjectRepository, CalculationRepository},
    scenario::{Project, ScenarioVariant},
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
///
/// For large grids (> `LARGE_GRID_PERSIST_THRESHOLD` cells) `levels` is
/// empty and `levels_file` contains the path to a raw little-endian f32
/// binary file written to the system temp directory.
#[derive(Debug, Serialize, Clone)]
pub struct CalcResult {
    pub calc_id: i64,
    pub metric: String,
    pub nx: usize,
    pub ny: usize,
    pub xmin: f64,
    pub ymin: f64,
    pub cellsize: f64,
    /// Inline levels – populated for grids ≤ LARGE_GRID_PERSIST_THRESHOLD cells.
    pub levels: Vec<f32>,
    /// Path to raw f32-LE binary file – populated for larger grids.
    pub levels_file: Option<String>,
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

/// Add a new scenario variant to an existing project.
#[tauri::command]
pub fn add_scenario(
    state: State<AppState>,
    project_id: String,
    name: String,
) -> Result<ScenarioInfo, String> {
    let uuid = uuid::Uuid::parse_str(&project_id)
        .map_err(|e| format!("invalid project_id: {e}"))?;
    let db = state.db.lock().map_err(|e| e.to_string())?;
    let repo = ProjectRepository::new(db.connection());
    let mut project = repo.get(uuid).map_err(|e| e.to_string())?;
    let variant = ScenarioVariant::new(&name, project.base_scenario.id);
    let info = ScenarioInfo {
        id: variant.id.to_string(),
        name: variant.name.clone(),
        is_base: false,
    };
    project.variants.push(variant);
    repo.insert(&project).map_err(|e| e.to_string())?;
    Ok(info)
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

/// Grids larger than this are written to a temp binary file rather than
/// stored inline in SQLite, to keep DB writes practical.
const LARGE_GRID_PERSIST_THRESHOLD: usize = 50_000_000; // 50 M cells ≈ 7071×7071

/// Maximum supported grid size (1 billion cells).
const MAX_GRID_CELLS: usize = 1_000_000_000;

/// Run a horizontal grid calculation for a scenario and persist the result.
///
/// Heavy computation runs in a `spawn_blocking` thread – the UI stays
/// responsive for any grid size.  The hard limit is 1 billion cells.
/// For grids > 50 M cells the levels are written to a temp binary file
/// (raw little-endian f32) instead of being stored in SQLite.
#[tauri::command]
pub async fn run_calculation(
    state: State<'_, AppState>,
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
    let n_cells = nx.checked_mul(ny).unwrap_or(usize::MAX);
    if n_cells > MAX_GRID_CELLS {
        return Err(format!(
            "格点数 {nx}×{ny}={n_cells} 超过硬件上限 10 亿，请缩小范围或降低分辨率"
        ));
    }

    // ── Load scene objects from DB (lock held only briefly) ──────────────────
    let (mut sources, barriers): (Vec<SourceSpec>, Vec<BarrierSpec>) = {
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
    if sources.is_empty() {
        sources.push(SourceSpec {
            id: 0,
            position: Point3::new((xmin + xmax) / 2.0, ymin + 10.0, 0.5),
            lw_db: [82.0; 8],
            g_source: 0.0,
        });
    }

    // ── Run heavy computation in a background thread ──────────────────────────
    let metric_for_closure = metric.clone();
    let levels: Vec<f32> = tauri::async_runtime::spawn_blocking(move || {
        let metric = metric_for_closure;
        let mut grid = HorizontalGrid::new(
            1,
            "desktop_grid",
            Point3::new(xmin, ymin, 0.0),
            resolution_m,
            resolution_m,
            nx,
            ny,
            4.0,
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
        grid.results
    })
    .await
    .map_err(|e| e.to_string())?;

    // ── Persist result ────────────────────────────────────────────────────────
    // For large grids, write a raw f32-LE binary file instead of a JSON blob.
    let large_grid = levels.len() > LARGE_GRID_PERSIST_THRESHOLD;
    let levels_file: Option<String>;
    let calc_id: i64;

    if large_grid {
        // Write binary temp file: nx*ny × 4 bytes, little-endian f32
        let tmp_path = std::env::temp_dir()
            .join(format!("noisecad_{scenario_id}_{metric}.bin"));
        let bytes: Vec<u8> = levels.iter()
            .flat_map(|&f| f.to_le_bytes())
            .collect();
        std::fs::write(&tmp_path, bytes)
            .map_err(|e| format!("failed to write levels file: {e}"))?;
        levels_file = Some(tmp_path.to_string_lossy().into_owned());

        // Store only metadata in DB (no inline levels)
        let data = serde_json::json!({
            "nx": nx, "ny": ny,
            "xmin": xmin, "ymin": ymin,
            "cellsize": resolution_m,
            "levels_file": levels_file,
        });
        calc_id = {
            let db = state.db.lock().map_err(|e| e.to_string())?;
            CalculationRepository::new(db.connection())
                .insert(&scenario_id, "horizontal", &metric, &data)
                .map_err(|e| e.to_string())?
        };
    } else {
        levels_file = None;
        let data = serde_json::json!({
            "nx": nx, "ny": ny,
            "xmin": xmin, "ymin": ymin,
            "cellsize": resolution_m,
            "levels": levels,
        });
        calc_id = {
            let db = state.db.lock().map_err(|e| e.to_string())?;
            CalculationRepository::new(db.connection())
                .insert(&scenario_id, "horizontal", &metric, &data)
                .map_err(|e| e.to_string())?
        };
    };

    // ── Statistics ────────────────────────────────────────────────────────────
    let finite: Vec<f32> = levels.iter().copied()
        .filter(|&v| v.is_finite() && v > 0.0)
        .collect();
    let (min_db, max_db, mean_db) = if finite.is_empty() {
        (0.0_f64, 0.0, 0.0)
    } else {
        let min  = finite.iter().copied().fold(f32::INFINITY,     f32::min) as f64;
        let max  = finite.iter().copied().fold(f32::NEG_INFINITY, f32::max) as f64;
        let mean = finite.iter().sum::<f32>() as f64 / finite.len() as f64;
        (min, max, mean)
    };

    Ok(CalcResult {
        calc_id, metric, nx, ny, xmin, ymin,
        cellsize: resolution_m,
        levels: if large_grid { vec![] } else { levels },
        levels_file,
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

/// 批量平移对象（世界坐标增量 dx, dy, dz）
#[tauri::command]
pub fn move_objects(
    state: State<AppState>,
    row_ids: Vec<i64>,
    dx: f64,
    dy: f64,
    dz: f64,
) -> Result<(), String> {
    let db = state.db.lock().map_err(|e| e.to_string())?;
    let repo = SceneObjectRepository::new(db.connection());
    for row_id in row_ids {
        let mut obj = repo.get(row_id).map_err(|e| e.to_string())?;
        translate_object(&mut obj, dx, dy, dz);
        repo.update(row_id, &obj).map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// 复制对象并偏移 dx, dy（返回新对象 row_ids）
#[tauri::command]
pub fn copy_objects(
    state: State<AppState>,
    scenario_id: String,
    row_ids: Vec<i64>,
    dx: f64,
    dy: f64,
) -> Result<Vec<i64>, String> {
    let db = state.db.lock().map_err(|e| e.to_string())?;
    let repo = SceneObjectRepository::new(db.connection());
    let mut new_ids = Vec::new();
    for row_id in row_ids {
        let mut obj = repo.get(row_id).map_err(|e| e.to_string())?;
        translate_object(&mut obj, dx, dy, 0.0);
        let new_id = repo.insert(&scenario_id, &obj).map_err(|e| e.to_string())?;
        new_ids.push(new_id);
    }
    Ok(new_ids)
}

/// 旋转对象（绕基点 cx,cy，angle_deg 逆时针为正）
#[tauri::command]
pub fn rotate_objects(
    state: State<AppState>,
    row_ids: Vec<i64>,
    cx: f64,
    cy: f64,
    angle_deg: f64,
) -> Result<(), String> {
    let angle_rad = angle_deg.to_radians();
    let db = state.db.lock().map_err(|e| e.to_string())?;
    let repo = SceneObjectRepository::new(db.connection());
    for row_id in row_ids {
        let mut obj = repo.get(row_id).map_err(|e| e.to_string())?;
        rotate_object(&mut obj, cx, cy, angle_rad);
        repo.update(row_id, &obj).map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// 缩放对象（绕基点 cx,cy，factor > 0）
#[tauri::command]
pub fn scale_objects(
    state: State<AppState>,
    row_ids: Vec<i64>,
    cx: f64,
    cy: f64,
    factor: f64,
) -> Result<(), String> {
    if factor <= 0.0 { return Err("scale factor must be positive".into()); }
    let db = state.db.lock().map_err(|e| e.to_string())?;
    let repo = SceneObjectRepository::new(db.connection());
    for row_id in row_ids {
        let mut obj = repo.get(row_id).map_err(|e| e.to_string())?;
        scale_object(&mut obj, cx, cy, factor);
        repo.update(row_id, &obj).map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// 镜像对象（沿轴线 (x1,y1)-(x2,y2)），keep_original=true 时保留原对象
/// 返回新对象 row_ids（keep_original=false 时为空 vec，对象已被就地修改）
#[tauri::command]
pub fn mirror_objects(
    state: State<AppState>,
    scenario_id: String,
    row_ids: Vec<i64>,
    x1: f64, y1: f64,
    x2: f64, y2: f64,
    keep_original: bool,
) -> Result<Vec<i64>, String> {
    let db = state.db.lock().map_err(|e| e.to_string())?;
    let repo = SceneObjectRepository::new(db.connection());
    let mut new_ids = Vec::new();
    for row_id in row_ids {
        let mut obj = repo.get(row_id).map_err(|e| e.to_string())?;
        mirror_object(&mut obj, x1, y1, x2, y2);
        if keep_original {
            let new_id = repo.insert(&scenario_id, &obj).map_err(|e| e.to_string())?;
            new_ids.push(new_id);
        } else {
            repo.update(row_id, &obj).map_err(|e| e.to_string())?;
        }
    }
    Ok(new_ids)
}

/// 修剪折线：只保留 [start_idx..=end_idx] 之间的顶点
#[tauri::command]
pub fn trim_polyline(
    state: State<AppState>,
    row_id: i64,
    start_idx: usize,
    end_idx: usize,
) -> Result<(), String> {
    let db = state.db.lock().map_err(|e| e.to_string())?;
    let repo = SceneObjectRepository::new(db.connection());
    let mut obj = repo.get(row_id).map_err(|e| e.to_string())?;
    let verts = get_vertices(&obj)
        .ok_or("object does not have polyline vertices")?;
    let end = end_idx.min(verts.len().saturating_sub(1));
    let start = start_idx.min(end);
    if end - start < 1 { return Err("trim would leave fewer than 2 vertices".into()); }
    let trimmed = verts[start..=end].to_vec();
    set_vertices(&mut obj, trimmed);
    repo.update(row_id, &obj).map_err(|e| e.to_string())
}

/// 打断折线：在第 seg_idx 段的 t 参数（0..1）处分裂为两个对象
/// 返回 (原对象row_id, 新对象row_id)
#[tauri::command]
pub fn break_polyline(
    state: State<AppState>,
    scenario_id: String,
    row_id: i64,
    seg_idx: usize,
    t: f64,
) -> Result<(i64, i64), String> {
    let t = t.clamp(0.001, 0.999);
    let db = state.db.lock().map_err(|e| e.to_string())?;
    let repo = SceneObjectRepository::new(db.connection());
    let obj = repo.get(row_id).map_err(|e| e.to_string())?;
    let verts = get_vertices(&obj).ok_or("object does not have polyline vertices")?;
    if seg_idx + 1 >= verts.len() {
        return Err(format!("seg_idx {} out of range for {} vertices", seg_idx, verts.len()));
    }
    let a = verts[seg_idx];
    let b = verts[seg_idx + 1];
    let bp = [
        a[0] + t * (b[0] - a[0]),
        a[1] + t * (b[1] - a[1]),
        a[2] + t * (b[2] - a[2]),
    ];

    // Part A: vertices[0..=seg_idx] + break_point
    let mut part_a = verts[..=seg_idx].to_vec();
    part_a.push(bp);

    // Part B: break_point + vertices[seg_idx+1..]
    let mut part_b = vec![bp];
    part_b.extend_from_slice(&verts[seg_idx + 1..]);

    // Update original to part_a, create new object for part_b
    let mut obj_a = obj.clone();
    set_vertices(&mut obj_a, part_a);
    repo.update(row_id, &obj_a).map_err(|e| e.to_string())?;

    let mut obj_b = obj.clone();
    set_vertices(&mut obj_b, part_b);
    let new_id = repo.insert(&scenario_id, &obj_b).map_err(|e| e.to_string())?;

    Ok((row_id, new_id))
}

/// 合并两条折线（自动找最近端点拼接），row_id_b 被删除，row_id_a 变为合并结果
#[tauri::command]
pub fn join_polylines(
    state: State<AppState>,
    row_id_a: i64,
    row_id_b: i64,
) -> Result<i64, String> {
    let db = state.db.lock().map_err(|e| e.to_string())?;
    let repo = SceneObjectRepository::new(db.connection());
    let obj_a = repo.get(row_id_a).map_err(|e| e.to_string())?;
    let obj_b = repo.get(row_id_b).map_err(|e| e.to_string())?;
    let va = get_vertices(&obj_a).ok_or("object A has no vertices")?;
    let vb = get_vertices(&obj_b).ok_or("object B has no vertices")?;

    // 检查四种端点组合，选距离最近的
    let dist = |a: [f64;3], b: [f64;3]| {
        ((a[0]-b[0]).powi(2) + (a[1]-b[1]).powi(2)).sqrt()
    };
    let d_ee = dist(va[va.len()-1], vb[0]);           // end_a -> start_b
    let d_es = dist(va[va.len()-1], vb[vb.len()-1]);  // end_a -> end_b (b reversed)
    let d_se = dist(va[0], vb[0]);                    // start_a -> start_b (a reversed)
    let d_ss = dist(va[0], vb[vb.len()-1]);           // start_a -> end_b (both reversed concepts)

    let min_d = d_ee.min(d_es).min(d_se).min(d_ss);
    let mut merged = if (d_ee - min_d).abs() < 1e-9 {
        let mut v = va.to_vec(); v.extend_from_slice(&vb); v
    } else if (d_es - min_d).abs() < 1e-9 {
        let mut v = va.to_vec(); let mut rb = vb.to_vec(); rb.reverse(); v.extend(rb); v
    } else if (d_se - min_d).abs() < 1e-9 {
        let mut ra = va.to_vec(); ra.reverse(); ra.extend_from_slice(&vb); ra
    } else {
        let mut ra = va.to_vec(); ra.reverse(); let mut rb = vb.to_vec(); rb.reverse(); ra.extend(rb); ra
    };

    // 如果接合点重合（距离 < 0.01m），去掉重复顶点
    if merged.len() > 1 {
        let mid = va.len(); // 接合处下标
        if mid < merged.len() {
            let p = merged[mid-1]; let q = merged[mid];
            if dist(p, q) < 0.01 { merged.remove(mid); }
        }
    }

    let mut obj_merged = obj_a.clone();
    set_vertices(&mut obj_merged, merged);
    repo.update(row_id_a, &obj_merged).map_err(|e| e.to_string())?;
    repo.delete(row_id_b).map_err(|e| e.to_string())?;
    Ok(row_id_a)
}

/// 偏移折线（平行复制），distance > 0 向左偏移（逆时针法线方向）
/// 返回新对象的 row_id
#[tauri::command]
pub fn offset_polyline(
    state: State<AppState>,
    scenario_id: String,
    row_id: i64,
    distance: f64,
) -> Result<i64, String> {
    if distance.abs() < 1e-9 { return Err("offset distance must not be zero".into()); }
    let db = state.db.lock().map_err(|e| e.to_string())?;
    let repo = SceneObjectRepository::new(db.connection());
    let obj = repo.get(row_id).map_err(|e| e.to_string())?;
    let verts = get_vertices(&obj).ok_or("object has no vertices")?;
    let offset_verts = offset_vertices_miter(verts, distance);
    let mut new_obj = obj.clone();
    set_vertices(&mut new_obj, offset_verts);
    let new_id = repo.insert(&scenario_id, &new_obj).map_err(|e| e.to_string())?;
    Ok(new_id)
}

/// 延伸折线端点到指定坐标
/// extend_start=true 在首部插入，false 在末尾追加
#[tauri::command]
pub fn extend_polyline(
    state: State<AppState>,
    row_id: i64,
    extend_start: bool,
    target_x: f64,
    target_y: f64,
) -> Result<(), String> {
    let db = state.db.lock().map_err(|e| e.to_string())?;
    let repo = SceneObjectRepository::new(db.connection());
    let mut obj = repo.get(row_id).map_err(|e| e.to_string())?;
    let mut verts = get_vertices(&obj).ok_or("object has no vertices")?;
    let z = if extend_start { verts[0][2] } else { verts[verts.len()-1][2] };
    let new_pt = [target_x, target_y, z];
    if extend_start { verts.insert(0, new_pt); } else { verts.push(new_pt); }
    set_vertices(&mut obj, verts);
    repo.update(row_id, &obj).map_err(|e| e.to_string())
}

/// 在折线第 seg_idx 段的 t 处插入新顶点，返回新顶点下标
#[tauri::command]
pub fn insert_vertex(
    state: State<AppState>,
    row_id: i64,
    seg_idx: usize,
    t: f64,
) -> Result<usize, String> {
    let t = t.clamp(0.001, 0.999);
    let db = state.db.lock().map_err(|e| e.to_string())?;
    let repo = SceneObjectRepository::new(db.connection());
    let mut obj = repo.get(row_id).map_err(|e| e.to_string())?;
    let mut verts = get_vertices(&obj).ok_or("object has no vertices")?;
    if seg_idx + 1 >= verts.len() {
        return Err(format!("seg_idx {} out of range", seg_idx));
    }
    let a = verts[seg_idx]; let b = verts[seg_idx+1];
    let new_pt = [a[0]+t*(b[0]-a[0]), a[1]+t*(b[1]-a[1]), a[2]+t*(b[2]-a[2])];
    let insert_at = seg_idx + 1;
    verts.insert(insert_at, new_pt);
    set_vertices(&mut obj, verts);
    repo.update(row_id, &obj).map_err(|e| e.to_string())?;
    Ok(insert_at)
}

} // pub mod commands

// ─── CAD geometry helpers ─────────────────────────────────────────────────────

/// 提取折线顶点为 Vec<[f64;3]>
fn get_vertices(obj: &SceneObject) -> Option<Vec<[f64; 3]>> {
    match obj {
        SceneObject::RoadSource(rs) =>
            Some(rs.vertices.iter().map(|v| [v.x, v.y, v.z]).collect()),
        SceneObject::Barrier(b) =>
            Some(b.vertices.iter().map(|v| [v.x, v.y, v.z]).collect()),
        _ => None,
    }
}

/// 将 Vec<[f64;3]> 写回对象的折线顶点
fn set_vertices(obj: &mut SceneObject, verts: Vec<[f64; 3]>) {
    let pts: Vec<Point3<f64>> = verts.iter().map(|v| Point3::new(v[0], v[1], v[2])).collect();
    match obj {
        SceneObject::RoadSource(rs) => rs.vertices = pts,
        SceneObject::Barrier(b)     => b.vertices = pts,
        _ => {}
    }
}

/// 平移对象（修改内部几何）
fn translate_object(obj: &mut SceneObject, dx: f64, dy: f64, dz: f64) {
    match obj {
        SceneObject::PointSource(ps) => {
            ps.position = Point3::new(ps.position.x+dx, ps.position.y+dy, ps.position.z+dz);
        }
        _ => {
            if let Some(verts) = get_vertices(obj) {
                let moved: Vec<[f64;3]> = verts.iter().map(|v| [v[0]+dx, v[1]+dy, v[2]+dz]).collect();
                set_vertices(obj, moved);
            }
        }
    }
}

/// 旋转对象（绕 (cx,cy)，angle_rad 逆时针）
fn rotate_object(obj: &mut SceneObject, cx: f64, cy: f64, angle_rad: f64) {
    let cos_a = angle_rad.cos();
    let sin_a = angle_rad.sin();
    let rot = |x: f64, y: f64| -> (f64, f64) {
        let dx = x - cx; let dy = y - cy;
        (cx + dx*cos_a - dy*sin_a, cy + dx*sin_a + dy*cos_a)
    };
    match obj {
        SceneObject::PointSource(ps) => {
            let (nx, ny) = rot(ps.position.x, ps.position.y);
            ps.position = Point3::new(nx, ny, ps.position.z);
        }
        _ => {
            if let Some(verts) = get_vertices(obj) {
                let rotated: Vec<[f64;3]> = verts.iter().map(|v| {
                    let (nx, ny) = rot(v[0], v[1]); [nx, ny, v[2]]
                }).collect();
                set_vertices(obj, rotated);
            }
        }
    }
}

/// 缩放对象（绕 (cx,cy)，factor）
fn scale_object(obj: &mut SceneObject, cx: f64, cy: f64, factor: f64) {
    let sc = |x: f64, y: f64| -> (f64, f64) {
        (cx + (x-cx)*factor, cy + (y-cy)*factor)
    };
    match obj {
        SceneObject::PointSource(ps) => {
            let (nx, ny) = sc(ps.position.x, ps.position.y);
            ps.position = Point3::new(nx, ny, ps.position.z);
        }
        _ => {
            if let Some(verts) = get_vertices(obj) {
                let scaled: Vec<[f64;3]> = verts.iter().map(|v| {
                    let (nx, ny) = sc(v[0], v[1]); [nx, ny, v[2]]
                }).collect();
                set_vertices(obj, scaled);
            }
        }
    }
}

/// 镜像对象（关于直线 (x1,y1)-(x2,y2)）
fn mirror_object(obj: &mut SceneObject, x1: f64, y1: f64, x2: f64, y2: f64) {
    let mir = |x: f64, y: f64| -> (f64, f64) {
        let dx = x2-x1; let dy = y2-y1;
        let len_sq = dx*dx + dy*dy;
        if len_sq < 1e-12 { return (x, y); }
        let t = ((x-x1)*dx + (y-y1)*dy) / len_sq;
        let fx = x1 + t*dx; let fy = y1 + t*dy;
        (2.0*fx - x, 2.0*fy - y)
    };
    match obj {
        SceneObject::PointSource(ps) => {
            let (nx, ny) = mir(ps.position.x, ps.position.y);
            ps.position = Point3::new(nx, ny, ps.position.z);
        }
        _ => {
            if let Some(verts) = get_vertices(obj) {
                let mirrored: Vec<[f64;3]> = verts.iter().map(|v| {
                    let (nx, ny) = mir(v[0], v[1]); [nx, ny, v[2]]
                }).collect();
                set_vertices(obj, mirrored);
            }
        }
    }
}

/// Miter joint 偏移折线顶点
fn offset_vertices_miter(verts: Vec<[f64; 3]>, distance: f64) -> Vec<[f64; 3]> {
    let n = verts.len();
    if n < 2 { return verts; }
    let seg_normal = |a: [f64;3], b: [f64;3]| -> [f64; 2] {
        let dx = b[0]-a[0]; let dy = b[1]-a[1];
        let len = (dx*dx+dy*dy).sqrt().max(1e-12);
        [-dy/len, dx/len]  // 左法线（逆时针）
    };
    let mut result = Vec::with_capacity(n);
    for i in 0..n {
        let normal = if i == 0 {
            seg_normal(verts[0], verts[1])
        } else if i == n-1 {
            seg_normal(verts[n-2], verts[n-1])
        } else {
            let n1 = seg_normal(verts[i-1], verts[i]);
            let n2 = seg_normal(verts[i], verts[i+1]);
            // Miter：两段法线的平均，长度调整
            let dot = n1[0]*n2[0] + n1[1]*n2[1];
            let miter_scale = if (1.0 + dot).abs() < 1e-6 { 1.0 }
                              else { 1.0 / (1.0 + dot).sqrt() };
            // 钳制 miter 长度（防止锐角产生过长尖刺）
            let clamped = miter_scale.min(5.0);
            [(n1[0]+n2[0]) * clamped * 0.5, (n1[1]+n2[1]) * clamped * 0.5]
        };
        result.push([
            verts[i][0] + normal[0] * distance,
            verts[i][1] + normal[1] * distance,
            verts[i][2],
        ]);
    }
    result
}

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
            commands::add_scenario,
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
            commands::move_objects,
            commands::copy_objects,
            commands::rotate_objects,
            commands::scale_objects,
            commands::mirror_objects,
            commands::trim_polyline,
            commands::break_polyline,
            commands::join_polylines,
            commands::offset_polyline,
            commands::extend_polyline,
            commands::insert_vertex,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
