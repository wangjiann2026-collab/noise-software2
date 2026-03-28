//! Export endpoints for completed calculation results.
//!
//! ## Routes
//! | Method | Path                              | Response          |
//! |--------|-----------------------------------|-------------------|
//! | GET    | /calculations/:id/export/geojson  | `application/geo+json` |
//! | GET    | /calculations/:id/export/asc      | `text/plain`      |
//! | GET    | /calculations/:id/export/csv      | `text/csv`        |
//!
//! `:id` is the integer `calc_result_id` stored in `calculation_results`.
//!
//! Optional query parameter `levels` (comma-separated dBA values) controls the
//! iso-contour levels for the GeoJSON export.

use axum::{
    extract::{Path, Query, State},
    http::{header, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use serde::Deserialize;
use noise_data::repository::CalculationRepository;
use noise_export::{GridView, export_geojson, export_asc, export_csv};
use noise_export::geojson::DEFAULT_LEVELS;

use crate::state::AppState;

// ─── Query params ────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct GeoJsonQuery {
    /// Comma-separated iso-contour levels, e.g. `?levels=45,50,55,60,65`.
    pub levels: Option<String>,
}

// ─── Shared helper ───────────────────────────────────────────────────────────

/// Load a [`GridView`] from the database for the given calculation ID.
fn load_grid_view(
    state: &AppState,
    calc_id: i64,
) -> Result<GridView, (StatusCode, Json<serde_json::Value>)> {
    if calc_id <= 0 {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": "calc_id must be a positive integer" })),
        ));
    }
    let db = state.db.lock().map_err(|_| (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(serde_json::json!({ "error": "DB lock error" })),
    ))?;
    let repo = CalculationRepository::new(db.connection());
    let cr = repo.get(calc_id).map_err(|e| (
        StatusCode::NOT_FOUND,
        Json(serde_json::json!({ "error": format!("Calculation {calc_id} not found: {e}") })),
    ))?;

    let nx       = cr.data["nx"].as_u64().unwrap_or(0) as usize;
    let ny       = cr.data["ny"].as_u64().unwrap_or(0) as usize;
    let xmin     = cr.data["xmin"].as_f64().unwrap_or(0.0);
    let ymin     = cr.data["ymin"].as_f64().unwrap_or(0.0);
    let cellsize = cr.data["cellsize"].as_f64().unwrap_or(10.0);
    let levels: Vec<f32> = cr.data["levels"]
        .as_array()
        .map(|a| a.iter().filter_map(|v| v.as_f64().map(|f| f as f32)).collect())
        .unwrap_or_default();

    if nx == 0 || ny == 0 {
        return Err((
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(serde_json::json!({ "error": "Stored calculation has zero-size grid" })),
        ));
    }

    Ok(GridView { levels, nx, ny, xllcorner: xmin, yllcorner: ymin, cellsize })
}

// ─── Handlers ────────────────────────────────────────────────────────────────

/// `GET /calculations/:id/export/geojson`
pub async fn export_geojson_handler(
    State(state): State<AppState>,
    Path(calc_id): Path<i64>,
    Query(q): Query<GeoJsonQuery>,
) -> Result<Response, (StatusCode, Json<serde_json::Value>)> {
    let view = load_grid_view(&state, calc_id)?;

    // Parse custom iso-levels from query parameter (fall back to defaults).
    let custom_levels: Vec<f32> = q.levels
        .as_deref()
        .unwrap_or("")
        .split(',')
        .filter_map(|s| s.trim().parse::<f32>().ok())
        .collect();
    let iso_levels: &[f32] = if custom_levels.is_empty() { DEFAULT_LEVELS } else { &custom_levels };

    let fc = export_geojson(&view, iso_levels);
    let body = serde_json::to_string(&fc).unwrap_or_default();

    Ok((
        [(header::CONTENT_TYPE, "application/geo+json; charset=utf-8"),
         (header::CONTENT_DISPOSITION, "inline; filename=\"noise_contours.geojson\"")],
        body,
    ).into_response())
}

/// `GET /calculations/:id/export/asc`
pub async fn export_asc_handler(
    State(state): State<AppState>,
    Path(calc_id): Path<i64>,
) -> Result<Response, (StatusCode, Json<serde_json::Value>)> {
    let view = load_grid_view(&state, calc_id)?;
    let body = export_asc(&view);

    Ok((
        [(header::CONTENT_TYPE, "text/plain; charset=utf-8"),
         (header::CONTENT_DISPOSITION, "attachment; filename=\"noise_grid.asc\"")],
        body,
    ).into_response())
}

/// `GET /calculations/:id/export/csv`
pub async fn export_csv_handler(
    State(state): State<AppState>,
    Path(calc_id): Path<i64>,
) -> Result<Response, (StatusCode, Json<serde_json::Value>)> {
    let view = load_grid_view(&state, calc_id)?;
    let body = export_csv(&view);

    Ok((
        [(header::CONTENT_TYPE, "text/csv; charset=utf-8"),
         (header::CONTENT_DISPOSITION, "attachment; filename=\"noise_levels.csv\"")],
        body,
    ).into_response())
}

// ─── Unit tests ───────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use noise_data::{repository::ProjectRepository, scenario::Project};

    fn state_with_calc() -> (AppState, i64) {
        let state = AppState::in_memory().unwrap();
        let project = Project::new("Export Test", 32650);
        let sid = project.base_scenario.id.to_string();
        let calc_id = {
            let db = state.db.lock().unwrap();
            ProjectRepository::new(db.connection()).insert(&project).unwrap();
            let data = serde_json::json!({
                "nx": 4, "ny": 4,
                "xmin": 0.0, "ymin": 0.0,
                "cellsize": 10.0,
                "levels": (0..16_u32).map(|i| 50.0 + i as f64 * 2.0).collect::<Vec<_>>()
            });
            CalculationRepository::new(db.connection())
                .insert(&sid, "horizontal", "Lden", &data)
                .unwrap()
        };
        (state, calc_id)
    }

    #[test]
    fn load_grid_view_returns_correct_dimensions() {
        let (state, calc_id) = state_with_calc();
        let view = load_grid_view(&state, calc_id).unwrap();
        assert_eq!(view.nx, 4);
        assert_eq!(view.ny, 4);
        assert_eq!(view.cellsize, 10.0);
    }

    #[test]
    fn load_grid_view_not_found() {
        let state = AppState::in_memory().unwrap();
        let result = load_grid_view(&state, 9999);
        assert!(result.is_err());
        assert_eq!(result.err().unwrap().0, StatusCode::NOT_FOUND);
    }

    #[test]
    fn load_grid_view_invalid_id() {
        let state = AppState::in_memory().unwrap();
        let result = load_grid_view(&state, 0);
        assert!(result.is_err());
        assert_eq!(result.err().unwrap().0, StatusCode::BAD_REQUEST);
    }

    #[test]
    fn geojson_levels_from_query() {
        let (state, calc_id) = state_with_calc();
        let view = load_grid_view(&state, calc_id).unwrap();
        let fc = export_geojson(&view, &[55.0, 60.0, 65.0]);
        assert_eq!(fc["type"], "FeatureCollection");
        assert!(fc["features"].is_array());
    }

    #[test]
    fn asc_export_has_header() {
        let (state, calc_id) = state_with_calc();
        let view = load_grid_view(&state, calc_id).unwrap();
        let asc = export_asc(&view);
        assert!(asc.contains("ncols        4"));
        assert!(asc.contains("nrows        4"));
    }

    #[test]
    fn csv_export_has_correct_rows() {
        let (state, calc_id) = state_with_calc();
        let view = load_grid_view(&state, calc_id).unwrap();
        let csv = export_csv(&view);
        let lines: Vec<&str> = csv.lines().collect();
        // Header + 16 valid data points
        assert_eq!(lines.len(), 17);
    }
}
