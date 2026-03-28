//! Calculation job REST API routes.
//!
//! POST /scenarios/:id/calculate → submit a calculation job
//! GET  /jobs/:id                → get job status and result

use axum::{Json, extract::Path, http::StatusCode};
use serde::{Deserialize, Serialize};
use noise_core::grid::{GridCalculator, HorizontalGrid, SourceSpec, CalculatorConfig};
use noise_core::engine::PropagationConfig;
use nalgebra::Point3;

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
    pub job_id: u64,
    pub scenario_id: String,
    pub status: String,
    pub metric: String,
    pub grid_type: String,
    pub resolution_m: f64,
    pub progress_pct: u8,
    pub result: Option<JobResult>,
}

#[derive(Debug, Serialize, Clone)]
pub struct JobResult {
    pub nx: usize,
    pub ny: usize,
    pub xllcorner: f64,
    pub yllcorner: f64,
    pub cellsize: f64,
    pub mean_db: f64,
    pub max_db: f64,
    pub min_db: f64,
    /// Compact grid data (f32 values as JSON array).
    pub levels: Vec<f32>,
}

/// POST /scenarios/:scenario_id/calculate
pub async fn submit_calculate(
    Path(scenario_id): Path<String>,
    Json(body): Json<CalculateRequest>,
) -> Result<Json<JobStatus>, (StatusCode, Json<serde_json::Value>)> {
    let metric = body.metric.unwrap_or_else(|| "Lden".into());
    let grid_type = body.grid_type.unwrap_or_else(|| "horizontal".into());
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

    // Run a synchronous calculation for the demo.
    // In production this would be enqueued as an async job.
    let demo_sources = vec![
        SourceSpec {
            id: 1,
            position: Point3::new((xmin + xmax) / 2.0, ymin + 10.0, 0.5),
            lw_db: [82.0; 8],
            g_source: 0.0,
        },
    ];

    let mut grid = HorizontalGrid::new(
        1,
        "api_grid",
        Point3::new(xmin, ymin, 0.0),
        resolution, resolution,
        nx, ny,
        4.0,
    );

    let cfg = CalculatorConfig {
        propagation: PropagationConfig::default(),
        g_receiver: 0.0,
        g_middle: 0.0,
    };
    let calc = GridCalculator::new(cfg);
    let _peak = calc.calculate(&mut grid, &demo_sources, &[], None);

    // Compute stats.
    let levels: Vec<f32> = grid.results;
    let finite: Vec<f32> = levels.iter().copied().filter(|&v| v.is_finite() && v > 0.0).collect();
    let (min_db, max_db, mean_db) = if finite.is_empty() {
        (0.0, 0.0, 0.0)
    } else {
        let min = finite.iter().copied().fold(f32::INFINITY, f32::min) as f64;
        let max = finite.iter().copied().fold(f32::NEG_INFINITY, f32::max) as f64;
        let mean = finite.iter().sum::<f32>() as f64 / finite.len() as f64;
        (min, max, mean)
    };

    static JOB_COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
    let job_id = JOB_COUNTER.fetch_add(1, std::sync::atomic::Ordering::SeqCst);

    Ok(Json(JobStatus {
        job_id,
        scenario_id: scenario_id.clone(),
        status: "completed".into(),
        metric: metric.clone(),
        grid_type: grid_type.clone(),
        resolution_m: resolution,
        progress_pct: 100,
        result: Some(JobResult {
            nx,
            ny,
            xllcorner: xmin,
            yllcorner: ymin,
            cellsize: resolution,
            mean_db: (mean_db * 10.0).round() / 10.0,
            max_db:  (max_db  * 10.0).round() / 10.0,
            min_db:  (min_db  * 10.0).round() / 10.0,
            levels,
        }),
    }))
}

/// GET /jobs/:job_id — return job status (demo: always completed).
pub async fn get_job(
    Path(job_id): Path<u64>,
) -> Result<Json<JobStatus>, (StatusCode, Json<serde_json::Value>)> {
    if job_id == 0 {
        return Err((
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": "Job not found" })),
        ));
    }
    Ok(Json(JobStatus {
        job_id,
        scenario_id: "demo".into(),
        status: "completed".into(),
        metric: "Lden".into(),
        grid_type: "horizontal".into(),
        resolution_m: 10.0,
        progress_pct: 100,
        result: Some(JobResult {
            nx: 4, ny: 4,
            xllcorner: 0.0, yllcorner: 0.0, cellsize: 10.0,
            mean_db: 57.3, max_db: 68.1, min_db: 45.0,
            levels: (0..16).map(|i| 45.0 + i as f32 * 1.5).collect(),
        }),
    }))
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn submit_returns_completed_job() {
        let req = CalculateRequest {
            metric: Some("Lden".into()),
            grid_type: Some("horizontal".into()),
            resolution_m: Some(20.0),
            extent: Some([0.0, 0.0, 100.0, 100.0]),
        };
        let resp = submit_calculate(Path("scenario-1".into()), Json(req)).await.unwrap();
        assert_eq!(resp.0.status, "completed");
        assert_eq!(resp.0.progress_pct, 100);
        assert!(resp.0.result.is_some());
        let r = resp.0.result.unwrap();
        assert_eq!(r.nx, 5); // 100/20
        assert_eq!(r.ny, 5);
    }

    #[tokio::test]
    async fn submit_negative_resolution_returns_error() {
        let req = CalculateRequest {
            metric: None, grid_type: None,
            resolution_m: Some(-5.0),
            extent: None,
        };
        let result = submit_calculate(Path("s1".into()), Json(req)).await;
        assert!(result.is_err());
        assert_eq!(result.err().unwrap().0, StatusCode::UNPROCESSABLE_ENTITY);
    }

    #[tokio::test]
    async fn get_job_returns_status() {
        let resp = get_job(Path(42)).await.unwrap();
        assert_eq!(resp.0.job_id, 42);
        assert_eq!(resp.0.status, "completed");
    }

    #[tokio::test]
    async fn get_job_zero_returns_404() {
        let result = get_job(Path(0)).await;
        assert!(result.is_err());
        assert_eq!(result.err().unwrap().0, StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn submit_default_extent_succeeds() {
        let req = CalculateRequest {
            metric: None, grid_type: None, resolution_m: None, extent: None,
        };
        let resp = submit_calculate(Path("s-default".into()), Json(req)).await.unwrap();
        assert!(resp.0.result.is_some());
    }
}
