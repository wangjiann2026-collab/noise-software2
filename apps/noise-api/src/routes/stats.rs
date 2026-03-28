//! Noise exposure statistics for completed calculations.
//!
//! ## Routes
//! | Method | Path                      | Description                          |
//! |--------|---------------------------|--------------------------------------|
//! | GET    | /calculations/:id/stats   | Exposure stats for a calculation     |
//! | GET    | /calculations/:id/stats/lden | Multi-period Lden stats on demand |
//!
//! ## Response (GET /calculations/:id/stats)
//! Returns [`ExposureStats`] JSON with counts, percentages, and distribution
//! across EU END threshold bands (55 / 65 / 70 dBA).
//!
//! ## Query parameters
//! - `thresholds` — comma-separated dBA values; defaults to EU END levels.

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use serde::Deserialize;
use noise_core::metrics::{compute_exposure, ExposureStats, EU_END_THRESHOLDS};
use noise_data::repository::CalculationRepository;

use crate::state::AppState;

#[derive(Debug, Deserialize)]
pub struct StatsQuery {
    /// Comma-separated custom threshold levels, e.g. `?thresholds=50,55,60,65,70`.
    pub thresholds: Option<String>,
}

// ─── GET /calculations/:id/stats ─────────────────────────────────────────────

pub async fn calc_stats(
    State(state): State<AppState>,
    Path(calc_id): Path<i64>,
    Query(q): Query<StatsQuery>,
) -> Result<Json<ExposureStats>, (StatusCode, Json<serde_json::Value>)> {
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

    let levels: Vec<f32> = cr.data["levels"]
        .as_array()
        .map(|a| a.iter().filter_map(|v| v.as_f64().map(|f| f as f32)).collect())
        .unwrap_or_default();

    // Parse custom thresholds or use EU END defaults.
    let custom: Vec<f64> = q.thresholds
        .as_deref()
        .unwrap_or("")
        .split(',')
        .filter_map(|s| s.trim().parse::<f64>().ok())
        .collect();
    let thresholds: &[f64] = if custom.is_empty() { &EU_END_THRESHOLDS } else { &custom };

    let stats = compute_exposure(&levels, thresholds);
    Ok(Json(stats))
}

// ─── Unit tests ───────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use axum::extract::{Path, Query, State};
    use noise_data::{repository::ProjectRepository, scenario::Project};

    fn state_with_calc(levels: Vec<f32>) -> (AppState, i64) {
        let state = AppState::in_memory().unwrap();
        let project = Project::new("Stats Test", 32650);
        let sid = project.base_scenario.id.to_string();
        let calc_id = {
            let db = state.db.lock().unwrap();
            ProjectRepository::new(db.connection()).insert(&project).unwrap();
            let data = serde_json::json!({
                "nx": levels.len(), "ny": 1,
                "xmin": 0.0, "ymin": 0.0,
                "cellsize": 10.0,
                "levels": levels
            });
            CalculationRepository::new(db.connection())
                .insert(&sid, "horizontal", "Lden", &data)
                .unwrap()
        };
        (state, calc_id)
    }

    #[tokio::test]
    async fn stats_returns_correct_min_max() {
        let levels = vec![50.0f32, 55.0, 60.0, 65.0, 70.0, 75.0];
        let (state, calc_id) = state_with_calc(levels);

        let q = StatsQuery { thresholds: None };
        let result = calc_stats(
            State(state),
            Path(calc_id),
            Query(q),
        ).await.unwrap();

        let stats = result.0;
        assert_eq!(stats.min_db, 50.0);
        assert_eq!(stats.max_db, 75.0);
        assert_eq!(stats.valid_receivers, 6);
    }

    #[tokio::test]
    async fn stats_above_thresholds_correct() {
        let levels = vec![48.0f32, 56.0, 62.0, 68.0, 72.0];
        let (state, calc_id) = state_with_calc(levels);

        let result = calc_stats(
            State(state),
            Path(calc_id),
            Query(StatsQuery { thresholds: None }),
        ).await.unwrap();

        let stats = result.0;
        // EU END at 55: 56, 62, 68, 72 → 4 above
        let t55 = stats.above_thresholds.iter()
            .find(|t| (t.threshold_db - 55.0).abs() < 0.1).unwrap();
        assert_eq!(t55.count_above, 4);
    }

    #[tokio::test]
    async fn stats_invalid_calc_id_returns_bad_request() {
        let state = AppState::in_memory().unwrap();
        let result = calc_stats(
            State(state),
            Path(0),
            Query(StatsQuery { thresholds: None }),
        ).await;
        assert!(result.is_err());
        assert_eq!(result.err().unwrap().0, StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn stats_missing_calc_returns_not_found() {
        let state = AppState::in_memory().unwrap();
        let result = calc_stats(
            State(state),
            Path(999),
            Query(StatsQuery { thresholds: None }),
        ).await;
        assert!(result.is_err());
        assert_eq!(result.err().unwrap().0, StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn stats_custom_thresholds() {
        let levels = vec![50.0f32, 55.0, 60.0, 65.0, 70.0];
        let (state, calc_id) = state_with_calc(levels);

        let q = StatsQuery { thresholds: Some("50,60".into()) };
        let result = calc_stats(State(state), Path(calc_id), Query(q)).await.unwrap();
        let stats = result.0;
        // Custom thresholds: 50, 60 → 2 bands above
        assert_eq!(stats.above_thresholds.len(), 2);
    }
}
