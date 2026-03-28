//! Server-side noise map rendering endpoints.
//!
//! These routes compute a noise grid on-the-fly for a given project/scenario
//! and return the result as a PNG image, SVG contour overlay, or JSON stats.
//! They use the CPU-only path from `noise-render` (no GPU required).
//!
//! # Routes
//! | Method | Path                                      | Returns            |
//! |--------|-------------------------------------------|--------------------|
//! | GET    | /projects/:pid/scenarios/:sid/render/png  | image/png          |
//! | GET    | /projects/:pid/scenarios/:sid/render/svg  | image/svg+xml      |
//! | GET    | /projects/:pid/scenarios/:sid/render/stats| application/json   |
//!
//! # Query parameters (all optional)
//! | Name        | Default | Description                        |
//! |-------------|---------|------------------------------------|
//! | `nx`        | 40      | Grid columns                       |
//! | `ny`        | 40      | Grid rows                          |
//! | `spacing_m` | 5.0     | Cell spacing (metres)              |
//! | `lw_db`     | 95.0    | Source sound power level (dBA)     |
//! | `width_px`  | 800     | SVG/PNG viewport width             |
//! | `height_px` | 600     | SVG/PNG viewport height            |

use axum::{
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode, header},
    Json,
    response::{IntoResponse, Response},
};
use nalgebra::Point3;
use serde::{Deserialize, Serialize};
use std::io::Cursor;

use noise_core::grid::{
    calculator::{CalculatorConfig, GridCalculator, SourceSpec},
    horizontal::HorizontalGrid,
};
use noise_render::{
    color::ColorMap,
    export::svg::{render_to_string as svg_to_string, SvgStyle, LevelColor},
    map2d::Map2DRenderer,
};

use crate::state::AppState;

/// Optional query parameters shared by all render endpoints.
#[derive(Debug, Deserialize)]
pub struct RenderQuery {
    /// Grid columns.
    #[serde(default = "default_n")]
    pub nx: u64,
    /// Grid rows.
    #[serde(default = "default_n")]
    pub ny: u64,
    /// Cell spacing in metres.
    #[serde(default = "default_spacing")]
    pub spacing_m: f64,
    /// Source sound power level (dBA, flat spectrum).
    #[serde(default = "default_lw")]
    pub lw_db: f64,
    /// Viewport width in pixels (PNG/SVG).
    #[serde(default = "default_width")]
    pub width_px: u32,
    /// Viewport height in pixels (PNG/SVG).
    #[serde(default = "default_height")]
    pub height_px: u32,
}

fn default_n()       -> u64  { 40 }
fn default_spacing() -> f64  { 5.0 }
fn default_lw()      -> f64  { 95.0 }
fn default_width()   -> u32  { 800 }
fn default_height()  -> u32  { 600 }

/// JSON response for `/render/stats`.
#[derive(Debug, Serialize)]
pub struct RenderStats {
    pub project_id:  String,
    pub scenario_id: String,
    pub nx:          u64,
    pub ny:          u64,
    pub spacing_m:   f64,
    pub point_count: usize,
    pub min_dba:     f32,
    pub max_dba:     f32,
    pub mean_dba:    f32,
    pub exceed_55_pct: f32,
    pub exceed_65_pct: f32,
}

// ─── Helpers ──────────────────────────────────────────────────────────────────

/// Compute a demo noise grid for the given parameters.
/// The source is placed at the grid centre, 0.5 m above ground.
fn compute_demo_grid(q: &RenderQuery) -> HorizontalGrid {
    let mut grid = HorizontalGrid::new(
        1,
        "render",
        Point3::new(0.0, 0.0, 0.0),
        q.spacing_m,
        q.spacing_m,
        q.nx as usize,
        q.ny as usize,
        4.0, // receiver height
    );

    let cx = q.nx as f64 * q.spacing_m / 2.0;
    let cy = q.ny as f64 * q.spacing_m / 2.0;
    let source = SourceSpec {
        id: 1,
        position: Point3::new(cx, cy, 0.5),
        lw_db: [q.lw_db; 8],
        g_source: 0.5,
    };

    let calc = GridCalculator::new(CalculatorConfig::default());
    calc.calculate(&mut grid, &[source], &[], None);
    grid
}

/// Standard contour levels for environmental noise (dBA).
const CONTOUR_LEVELS: &[f32] = &[45.0, 50.0, 55.0, 60.0, 65.0, 70.0, 75.0];

fn contour_palette() -> Vec<LevelColor> {
    let colors = [
        "#2c7bb6", "#00a6ca", "#00ccbc", "#90eb9d",
        "#ffff8c", "#f9d057", "#f29e2e", "#e76818",
    ];
    CONTOUR_LEVELS.iter().enumerate().map(|(i, &lvl)| {
        let c = colors.get(i).copied().unwrap_or("#333333");
        (lvl, c.to_string())
    }).collect()
}

fn grid_bounds(grid: &HorizontalGrid) -> [f32; 4] {
    let ox = grid.origin.x as f32;
    let oy = grid.origin.y as f32;
    [ox, oy,
     ox + grid.nx as f32 * grid.dx as f32,
     oy + grid.ny as f32 * grid.dy as f32]
}

// ─── Handlers ─────────────────────────────────────────────────────────────────

/// `GET /projects/:pid/scenarios/:sid/render/png`
///
/// Returns a PNG heatmap of the computed noise grid.
pub async fn render_png(
    State(state): State<AppState>,
    Path((pid, sid)): Path<(String, String)>,
    Query(q): Query<RenderQuery>,
) -> Response {
    // Verify project exists.
    if let Err(resp) = verify_project(&state, &pid) {
        return resp.into_response();
    }

    let grid = tokio::task::spawn_blocking(move || compute_demo_grid(&q))
        .await
        .expect("blocking task panicked");

    let renderer = Map2DRenderer {
        color_map: ColorMap::who_standard(),
        ..Default::default()
    };

    let (w, h, rgba) = match renderer.render_to_buffer(&grid) {
        Ok(v) => v,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": e.to_string() })),
            ).into_response();
        }
    };

    // Encode RGBA buffer to PNG bytes.
    let png_bytes = match encode_png(w, h, rgba) {
        Ok(b) => b,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": e })),
            ).into_response();
        }
    };

    let mut headers = HeaderMap::new();
    headers.insert(header::CONTENT_TYPE, "image/png".parse().unwrap());
    headers.insert(
        header::CONTENT_DISPOSITION,
        format!("inline; filename=\"noise_{}_{}.png\"", pid, sid)
            .parse().unwrap(),
    );
    (StatusCode::OK, headers, png_bytes).into_response()
}

/// `GET /projects/:pid/scenarios/:sid/render/svg`
///
/// Returns an SVG iso-contour overlay for the computed noise grid.
pub async fn render_svg(
    State(state): State<AppState>,
    Path((pid, sid)): Path<(String, String)>,
    Query(q): Query<RenderQuery>,
) -> Response {
    if let Err(resp) = verify_project(&state, &pid) {
        return resp.into_response();
    }

    let width_px  = q.width_px;
    let height_px = q.height_px;

    let grid = tokio::task::spawn_blocking(move || compute_demo_grid(&q))
        .await
        .expect("blocking task panicked");

    let renderer = Map2DRenderer::default();
    let isolines = renderer.extract_isolines(&grid, CONTOUR_LEVELS);
    let bounds   = grid_bounds(&grid);
    let palette  = contour_palette();
    let style    = SvgStyle { width_px, height_px, ..Default::default() };

    let svg = match svg_to_string(&isolines, bounds, &palette, &style) {
        Ok(s) => s,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": e.to_string() })),
            ).into_response();
        }
    };

    let mut headers = HeaderMap::new();
    headers.insert(header::CONTENT_TYPE, "image/svg+xml".parse().unwrap());
    headers.insert(
        header::CONTENT_DISPOSITION,
        format!("inline; filename=\"noise_{}_{}.svg\"", pid, sid)
            .parse().unwrap(),
    );
    (StatusCode::OK, headers, svg).into_response()
}

/// `GET /projects/:pid/scenarios/:sid/render/stats`
///
/// Returns JSON statistics for the computed noise grid.
pub async fn render_stats(
    State(state): State<AppState>,
    Path((pid, sid)): Path<(String, String)>,
    Query(q): Query<RenderQuery>,
) -> Result<Json<RenderStats>, (StatusCode, Json<serde_json::Value>)> {
    verify_project(&state, &pid)?;

    let spacing = q.spacing_m;
    let nx      = q.nx;
    let ny      = q.ny;

    let grid = tokio::task::spawn_blocking(move || compute_demo_grid(&q))
        .await
        .expect("blocking task panicked");

    let renderer = Map2DRenderer::default();
    let stats = renderer.grid_stats(&grid).ok_or_else(|| (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(serde_json::json!({ "error": "no results computed" })),
    ))?;

    let exceed_55 = grid.results.iter().filter(|&&v| v >= 55.0).count();
    let exceed_65 = grid.results.iter().filter(|&&v| v >= 65.0).count();
    let total     = grid.results.len().max(1);

    Ok(Json(RenderStats {
        project_id:  pid,
        scenario_id: sid,
        nx,
        ny,
        spacing_m:   spacing,
        point_count: stats.count,
        min_dba:     stats.min_dba,
        max_dba:     stats.max_dba,
        mean_dba:    stats.mean_dba,
        exceed_55_pct: exceed_55 as f32 / total as f32 * 100.0,
        exceed_65_pct: exceed_65 as f32 / total as f32 * 100.0,
    }))
}

// ─── Internal helpers ─────────────────────────────────────────────────────────

/// Verify the project UUID is valid and exists in the DB.
fn verify_project(
    state: &AppState,
    pid: &str,
) -> Result<(), (StatusCode, Json<serde_json::Value>)> {
    let uid = uuid::Uuid::parse_str(pid).map_err(|_| (
        StatusCode::BAD_REQUEST,
        Json(serde_json::json!({ "error": format!("'{pid}' is not a valid UUID") })),
    ))?;
    let db = state.db.lock().map_err(|e| (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(serde_json::json!({ "error": e.to_string() })),
    ))?;
    let repo = noise_data::repository::ProjectRepository::new(db.connection());
    repo.get(uid).map_err(|_| (
        StatusCode::NOT_FOUND,
        Json(serde_json::json!({ "error": format!("Project '{pid}' not found") })),
    ))?;
    Ok(())
}

/// Encode a raw RGBA buffer as PNG bytes.
fn encode_png(w: u32, h: u32, rgba: Vec<u8>) -> Result<Vec<u8>, String> {
    let img = image::RgbaImage::from_raw(w, h, rgba)
        .ok_or("invalid RGBA dimensions")?;
    let mut cursor = Cursor::new(Vec::new());
    img.write_to(&mut cursor, image::ImageFormat::Png)
        .map_err(|e| e.to_string())?;
    Ok(cursor.into_inner())
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use noise_data::{repository::ProjectRepository, scenario::Project};

    fn test_state_with_project() -> (AppState, String) {
        let state = AppState::in_memory().unwrap();
        let project = Project::new("Render Test", 32650);
        let pid = project.id.to_string();
        {
            let db = state.db.lock().unwrap();
            let repo = ProjectRepository::new(db.connection());
            repo.insert(&project).unwrap();
        }
        (state, pid)
    }

    #[test]
    fn compute_demo_grid_fills_results() {
        let q = RenderQuery {
            nx: 10, ny: 10, spacing_m: 5.0, lw_db: 90.0,
            width_px: 800, height_px: 600,
        };
        let grid = compute_demo_grid(&q);
        assert_eq!(grid.results.len(), 100);
        assert!(grid.results.iter().any(|&v| v > 0.0));
    }

    #[test]
    fn encode_png_produces_valid_header() {
        let rgba = vec![255u8; 4 * 4 * 4]; // 4×4 RGBA
        let png = encode_png(4, 4, rgba).unwrap();
        // PNG magic bytes: 0x89 'P' 'N' 'G'
        assert_eq!(&png[0..4], &[0x89, 0x50, 0x4E, 0x47]);
    }

    #[tokio::test]
    async fn render_stats_valid_project() {
        let (state, pid) = test_state_with_project();
        let sid = "scenario-1".to_string();
        let q = RenderQuery {
            nx: 10, ny: 10, spacing_m: 5.0, lw_db: 90.0,
            width_px: 800, height_px: 600,
        };
        let resp = render_stats(
            State(state),
            Path((pid, sid)),
            Query(q),
        ).await.unwrap();

        assert_eq!(resp.0.nx, 10);
        assert_eq!(resp.0.ny, 10);
        assert_eq!(resp.0.point_count, 100);
        assert!(resp.0.max_dba > resp.0.min_dba);
        assert!(resp.0.mean_dba > 0.0 && resp.0.mean_dba < 120.0);
    }

    #[tokio::test]
    async fn render_stats_missing_project_returns_404() {
        let state = AppState::in_memory().unwrap();
        let q = RenderQuery {
            nx: 5, ny: 5, spacing_m: 5.0, lw_db: 90.0,
            width_px: 800, height_px: 600,
        };
        let result = render_stats(
            State(state),
            Path((uuid::Uuid::new_v4().to_string(), "s1".into())),
            Query(q),
        ).await;
        assert!(result.is_err());
        assert_eq!(result.err().unwrap().0, StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn render_stats_invalid_uuid_returns_400() {
        let state = AppState::in_memory().unwrap();
        let q = RenderQuery {
            nx: 5, ny: 5, spacing_m: 5.0, lw_db: 90.0,
            width_px: 800, height_px: 600,
        };
        let result = render_stats(
            State(state),
            Path(("not-a-uuid".into(), "s1".into())),
            Query(q),
        ).await;
        assert!(result.is_err());
        assert_eq!(result.err().unwrap().0, StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn render_png_valid_project_returns_png_bytes() {
        let (state, pid) = test_state_with_project();
        let q = RenderQuery {
            nx: 8, ny: 8, spacing_m: 5.0, lw_db: 90.0,
            width_px: 400, height_px: 300,
        };
        let resp = render_png(
            State(state),
            Path((pid, "s1".into())),
            Query(q),
        ).await;
        let status = resp.status();
        assert_eq!(status, StatusCode::OK);
    }

    #[tokio::test]
    async fn render_svg_valid_project_returns_xml() {
        let (state, pid) = test_state_with_project();
        let q = RenderQuery {
            nx: 8, ny: 8, spacing_m: 5.0, lw_db: 90.0,
            width_px: 400, height_px: 300,
        };
        let resp = render_svg(
            State(state),
            Path((pid, "s1".into())),
            Query(q),
        ).await;
        assert_eq!(resp.status(), StatusCode::OK);
    }
}
