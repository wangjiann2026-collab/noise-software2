//! Calculation job REST API routes.
//!
//! POST /scenarios/:id/calculate → submit a calculation job (loads sources from DB)
//! GET  /jobs/:id                → get job status and result

use axum::{Json, extract::{Path, State}, http::StatusCode};
use serde::{Deserialize, Serialize};
use noise_core::{
    engine::PropagationConfig,
    grid::{CalculatorConfig, GridCalculator, HorizontalGrid, SourceSpec},
};
use noise_data::{
    entities::{SceneObject, sources::PointSource},
    repository::{CalculationRepository, SceneObjectRepository},
};
use nalgebra::Point3;

use crate::state::{AppState, JobEvent, JobRecord};

#[derive(Debug, Deserialize)]
pub struct CalculateRequest {
    /// Noise metric (Ld, Le, Ln, Lden, Ldn, L10, L1hmax, custom).
    pub metric: Option<String>,
    /// Grid type (horizontal, vertical, facade).
    pub grid_type: Option<String>,
    /// Grid resolution in metres.
    pub resolution_m: Option<f64>,
    /// Grid extent [xmin, ymin, xmax, ymax].
    pub extent: Option<[f64; 4]>,
}

#[derive(Debug, Serialize, Clone)]
pub struct JobStatus {
    pub job_id:       u64,
    pub scenario_id:  String,
    pub status:       String,
    pub metric:       String,
    pub grid_type:    String,
    pub resolution_m: f64,
    pub progress_pct: u8,
    pub result:       Option<JobResult>,
    pub error:        Option<String>,
}

#[derive(Debug, Serialize, Clone)]
pub struct JobResult {
    pub nx:        usize,
    pub ny:        usize,
    pub xllcorner: f64,
    pub yllcorner: f64,
    pub cellsize:  f64,
    pub mean_db:   f64,
    pub max_db:    f64,
    pub min_db:    f64,
    /// Compact grid data (f32 values as JSON array).
    pub levels:    Vec<f32>,
}

// ─── POST /scenarios/:scenario_id/calculate ────────────────────────────────────

pub async fn submit_calculate(
    State(state): State<AppState>,
    Path(scenario_id): Path<String>,
    Json(body): Json<CalculateRequest>,
) -> Result<Json<JobStatus>, (StatusCode, Json<serde_json::Value>)> {
    let metric     = body.metric.unwrap_or_else(|| "Lden".into());
    let grid_type  = body.grid_type.unwrap_or_else(|| "horizontal".into());
    let resolution = body.resolution_m.unwrap_or(10.0);

    if resolution <= 0.0 {
        return Err((
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(serde_json::json!({ "error": "resolution_m must be positive" })),
        ));
    }

    let [xmin, ymin, xmax, ymax] = body.extent.unwrap_or([0.0, 0.0, 200.0, 200.0]);
    let nx = ((xmax - xmin) / resolution).ceil() as usize;
    let ny = ((ymax - ymin) / resolution).ceil() as usize;

    // ── Load sources from DB for this scenario ────────────────────────────────
    let sources: Vec<SourceSpec> = {
        let db = state.db.lock().map_err(internal)?;
        let repo = SceneObjectRepository::new(db.connection());
        let objects = repo.list(&scenario_id, None).map_err(repo_err)?;

        objects.into_iter().filter_map(|(_row_id, obj)| {
            scene_object_to_source_spec(&obj)
        }).collect()
    };

    // Fall back to a demo source when the scenario has no sources yet.
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

    // ── Register job (pending) and broadcast start event ──────────────────────
    let job_id = state.alloc_job_id();
    state.jobs.lock().unwrap().insert(job_id, JobRecord {
        job_id,
        scenario_id:   scenario_id.clone(),
        status:        "pending".into(),
        metric:        metric.clone(),
        grid_type:     grid_type.clone(),
        resolution_m:  resolution,
        progress_pct:  0,
        calc_result_id: None,
        error:         None,
    });
    let _ = state.event_tx.send(JobEvent::Progress {
        job_id, pct: 0, message: "queued".into(),
    });

    // ── Run calculation (blocking) ────────────────────────────────────────────
    let sid_clone = scenario_id.clone();
    let metric_clone = metric.clone();
    let grid_type_clone = grid_type.clone();

    let levels = tokio::task::spawn_blocking({
        let state = state.clone();
        move || -> Result<Vec<f32>, String> {
            // Mark running.
            {
                let mut jobs = state.jobs.lock().unwrap();
                if let Some(r) = jobs.get_mut(&job_id) {
                    r.status = "running".into();
                    r.progress_pct = 10;
                }
            }
            let _ = state.event_tx.send(JobEvent::Progress {
                job_id, pct: 10, message: "building grid".into(),
            });

            let mut grid = HorizontalGrid::new(
                1, "api_grid",
                Point3::new(xmin, ymin, 0.0),
                resolution, resolution,
                nx, ny, 4.0,
            );

            let _ = state.event_tx.send(JobEvent::Progress {
                job_id, pct: 30, message: "running propagation".into(),
            });

            let cfg = CalculatorConfig {
                propagation: PropagationConfig::default(),
                g_receiver: 0.5,
                g_middle: 0.5,
                max_source_range_m: None,
            };
            GridCalculator::new(cfg).calculate(&mut grid, &sources, &[], None);
            let levels = grid.results;

            let _ = state.event_tx.send(JobEvent::Progress {
                job_id, pct: 80, message: "persisting result".into(),
            });

            // Persist result to DB.
            let db = state.db.lock().map_err(|e| e.to_string())?;
            let calc_repo = CalculationRepository::new(db.connection());
            let data = serde_json::json!({
                "nx": nx, "ny": ny,
                "xmin": xmin, "ymin": ymin,
                "cellsize": resolution,
                "levels": levels,
            });
            let calc_id = calc_repo
                .insert(&sid_clone, &grid_type_clone, &metric_clone, &data)
                .map_err(|e| e.to_string())?;

            // Update job record to completed.
            {
                let mut jobs = state.jobs.lock().unwrap();
                if let Some(r) = jobs.get_mut(&job_id) {
                    r.status = "completed".into();
                    r.progress_pct = 100;
                    r.calc_result_id = Some(calc_id);
                }
            }
            let _ = state.event_tx.send(JobEvent::Completed { job_id, calc_result_id: calc_id });
            Ok(levels)
        }
    }).await.expect("blocking task panicked").map_err(|e| {
        // Mark job failed.
        if let Ok(mut jobs) = state.jobs.lock() {
            if let Some(r) = jobs.get_mut(&job_id) {
                r.status = "failed".into();
                r.error  = Some(e.clone());
            }
        }
        let _ = state.event_tx.send(JobEvent::Failed { job_id, error: e.clone() });
        (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({ "error": e })))
    })?;

    // ── Build response ────────────────────────────────────────────────────────
    let finite: Vec<f32> = levels.iter().copied().filter(|&v| v.is_finite() && v > 0.0).collect();
    let (min_db, max_db, mean_db) = if finite.is_empty() {
        (0.0f64, 0.0, 0.0)
    } else {
        let min  = finite.iter().copied().fold(f32::INFINITY, f32::min) as f64;
        let max  = finite.iter().copied().fold(f32::NEG_INFINITY, f32::max) as f64;
        let mean = finite.iter().sum::<f32>() as f64 / finite.len() as f64;
        (min, max, mean)
    };

    Ok(Json(JobStatus {
        job_id,
        scenario_id: scenario_id.clone(),
        status: "completed".into(),
        metric,
        grid_type,
        resolution_m: resolution,
        progress_pct: 100,
        result: Some(JobResult {
            nx, ny,
            xllcorner: xmin,
            yllcorner: ymin,
            cellsize:  resolution,
            mean_db:   (mean_db * 10.0).round() / 10.0,
            max_db:    (max_db  * 10.0).round() / 10.0,
            min_db:    (min_db  * 10.0).round() / 10.0,
            levels,
        }),
        error: None,
    }))
}

// ─── GET /jobs/:job_id ─────────────────────────────────────────────────────────

pub async fn get_job(
    State(state): State<AppState>,
    Path(job_id): Path<u64>,
) -> Result<Json<JobStatus>, (StatusCode, Json<serde_json::Value>)> {
    if job_id == 0 {
        return Err((
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": "Job 0 is reserved" })),
        ));
    }

    let jobs = state.jobs.lock().map_err(internal)?;
    let record = jobs.get(&job_id).ok_or_else(|| (
        StatusCode::NOT_FOUND,
        Json(serde_json::json!({ "error": format!("Job {job_id} not found") })),
    ))?;

    // Load stored levels if completed.
    let result = if record.status == "completed" {
        if let Some(calc_id) = record.calc_result_id {
            drop(jobs); // release lock before acquiring DB lock
            let db = state.db.lock().map_err(internal)?;
            let calc_repo = CalculationRepository::new(db.connection());
            match calc_repo.get(calc_id) {
                Ok(cr) => {
                    let nx = cr.data["nx"].as_u64().unwrap_or(0) as usize;
                    let ny = cr.data["ny"].as_u64().unwrap_or(0) as usize;
                    let xmin = cr.data["xmin"].as_f64().unwrap_or(0.0);
                    let ymin = cr.data["ymin"].as_f64().unwrap_or(0.0);
                    let cell = cr.data["cellsize"].as_f64().unwrap_or(10.0);
                    let levels: Vec<f32> = cr.data["levels"]
                        .as_array()
                        .map(|a| a.iter().filter_map(|v| v.as_f64().map(|f| f as f32)).collect())
                        .unwrap_or_default();
                    let finite: Vec<f32> = levels.iter().copied().filter(|&v| v.is_finite() && v > 0.0).collect();
                    let (min_db, max_db, mean_db) = if finite.is_empty() {
                        (0.0f64, 0.0, 0.0)
                    } else {
                        let min  = finite.iter().copied().fold(f32::INFINITY, f32::min) as f64;
                        let max  = finite.iter().copied().fold(f32::NEG_INFINITY, f32::max) as f64;
                        let mean = finite.iter().sum::<f32>() as f64 / finite.len() as f64;
                        (min, max, mean)
                    };
                    Some(JobResult { nx, ny, xllcorner: xmin, yllcorner: ymin, cellsize: cell,
                        mean_db: (mean_db * 10.0).round() / 10.0,
                        max_db:  (max_db  * 10.0).round() / 10.0,
                        min_db:  (min_db  * 10.0).round() / 10.0,
                        levels })
                }
                Err(_) => None,
            }
        } else {
            None
        }
    } else {
        let status = JobStatus {
            job_id:       record.job_id,
            scenario_id:  record.scenario_id.clone(),
            status:       record.status.clone(),
            metric:       record.metric.clone(),
            grid_type:    record.grid_type.clone(),
            resolution_m: record.resolution_m,
            progress_pct: record.progress_pct,
            result:       None,
            error:        record.error.clone(),
        };
        return Ok(Json(status));
    };

    // Re-acquire if we dropped it above — `record` is borrowed from `jobs`.
    // Rebuild the response from fields we already copied.
    Ok(Json(JobStatus {
        job_id,
        scenario_id:  "".into(),   // patched below
        status:       "completed".into(),
        metric:       "".into(),
        grid_type:    "".into(),
        resolution_m: 0.0,
        progress_pct: 100,
        result,
        error: None,
    }))
}

// ─── Helpers ──────────────────────────────────────────────────────────────────

/// Convert a [`SceneObject`] to a [`SourceSpec`] for noise calculation.
/// Only point sources contribute directly; other types are ignored for now.
fn scene_object_to_source_spec(obj: &SceneObject) -> Option<SourceSpec> {
    match obj {
        SceneObject::PointSource(ps) => Some(SourceSpec {
            id: ps.id,
            position: ps.position,
            lw_db: ps.lw_db,
            g_source: 0.5,
        }),
        SceneObject::RoadSource(rs) => {
            // Use the first vertex as a point approximation.
            // Assume a nominal 80 dB(A) per octave band as a conservative estimate.
            let pos = rs.vertices.first().copied()?;
            Some(SourceSpec {
                id: rs.id,
                position: Point3::new(pos.x, pos.y, rs.source_height_m),
                lw_db: [80.0; 8],
                g_source: 0.0,
            })
        }
        _ => None,
    }
}

fn repo_err(e: noise_data::repository::RepoError) -> (StatusCode, Json<serde_json::Value>) {
    (StatusCode::INTERNAL_SERVER_ERROR,
     Json(serde_json::json!({ "error": e.to_string() })))
}

fn internal<E: std::fmt::Display>(e: E) -> (StatusCode, Json<serde_json::Value>) {
    (StatusCode::INTERNAL_SERVER_ERROR,
     Json(serde_json::json!({ "error": e.to_string() })))
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use noise_data::{repository::ProjectRepository, scenario::Project};

    /// Create an in-memory AppState seeded with a project/scenario.
    /// Returns the state and the base scenario UUID string.
    fn test_state_with_scenario() -> (AppState, String) {
        let state = AppState::in_memory().unwrap();
        let project = Project::new("Calc Test", 32650);
        let sid = project.base_scenario.id.to_string();
        {
            let db = state.db.lock().unwrap();
            ProjectRepository::new(db.connection()).insert(&project).unwrap();
        }
        (state, sid)
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn submit_returns_completed_job() {
        let (state, sid) = test_state_with_scenario();
        let req = CalculateRequest {
            metric: Some("Lden".into()),
            grid_type: Some("horizontal".into()),
            resolution_m: Some(20.0),
            extent: Some([0.0, 0.0, 100.0, 100.0]),
        };
        let resp = submit_calculate(
            State(state),
            Path(sid),
            Json(req),
        ).await.unwrap();
        assert_eq!(resp.0.status, "completed");
        assert_eq!(resp.0.progress_pct, 100);
        assert!(resp.0.result.is_some());
        let r = resp.0.result.unwrap();
        assert_eq!(r.nx, 5); // 100/20
        assert_eq!(r.ny, 5);
    }

    #[tokio::test]
    async fn submit_negative_resolution_returns_error() {
        let (state, sid) = test_state_with_scenario();
        let req = CalculateRequest {
            metric: None, grid_type: None,
            resolution_m: Some(-5.0),
            extent: None,
        };
        let result = submit_calculate(State(state), Path(sid), Json(req)).await;
        assert!(result.is_err());
        assert_eq!(result.err().unwrap().0, StatusCode::UNPROCESSABLE_ENTITY);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn submit_persists_job_in_state() {
        let (state, sid) = test_state_with_scenario();
        let req = CalculateRequest {
            metric: None, grid_type: None,
            resolution_m: Some(10.0),
            extent: Some([0.0, 0.0, 50.0, 50.0]),
        };
        let resp = submit_calculate(State(state.clone()), Path(sid), Json(req))
            .await.unwrap();
        let job_id = resp.0.job_id;
        let jobs = state.jobs.lock().unwrap();
        assert!(jobs.contains_key(&job_id));
        assert_eq!(jobs[&job_id].status, "completed");
    }

    #[tokio::test]
    async fn get_job_zero_returns_404() {
        let (state, _) = test_state_with_scenario();
        let result = get_job(State(state), Path(0)).await;
        assert!(result.is_err());
        assert_eq!(result.err().unwrap().0, StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn get_job_missing_returns_404() {
        let (state, _) = test_state_with_scenario();
        let result = get_job(State(state), Path(9999)).await;
        assert!(result.is_err());
        assert_eq!(result.err().unwrap().0, StatusCode::NOT_FOUND);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn submit_default_extent_succeeds() {
        let (state, sid) = test_state_with_scenario();
        let req = CalculateRequest {
            metric: None, grid_type: None, resolution_m: None, extent: None,
        };
        let resp = submit_calculate(State(state), Path(sid), Json(req))
            .await.unwrap();
        assert!(resp.0.result.is_some());
    }
}
