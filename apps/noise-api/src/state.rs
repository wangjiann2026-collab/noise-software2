//! Shared application state for the noise-api server.
//!
//! [`AppState`] is cloned into every request handler via Axum's
//! `State<AppState>` extractor.  The inner `Arc<Mutex<Database>>` ensures
//! a single `rusqlite::Connection` is shared safely across the Tokio thread
//! pool.  `jobs` tracks in-flight and completed calculation jobs in memory.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicU64, Ordering};
use noise_data::db::Database;
use serde::Serialize;

// ─── Job tracking ─────────────────────────────────────────────────────────────

/// Status of a single calculation job (kept in memory in `AppState::jobs`).
#[derive(Debug, Clone, Serialize)]
pub struct JobRecord {
    pub job_id:        u64,
    pub scenario_id:   String,
    /// `"pending"` | `"running"` | `"completed"` | `"failed"`
    pub status:        String,
    pub metric:        String,
    pub grid_type:     String,
    pub resolution_m:  f64,
    pub progress_pct:  u8,
    /// Row ID in `calculation_results` when `status == "completed"`.
    pub calc_result_id: Option<i64>,
    pub error:         Option<String>,
}

// ─── AppState ─────────────────────────────────────────────────────────────────

/// Shared application state.
///
/// `Clone` is cheap — copies only the `Arc` pointers.
#[derive(Clone)]
pub struct AppState {
    pub db:           Arc<Mutex<Database>>,
    pub jobs:         Arc<Mutex<HashMap<u64, JobRecord>>>,
    pub next_job_id:  Arc<AtomicU64>,
}

impl AppState {
    /// Open (or create) the database at `db_path` and run all migrations.
    pub fn new(db_path: &str) -> anyhow::Result<Self> {
        let db = Database::open(db_path)?;
        Ok(Self {
            db:          Arc::new(Mutex::new(db)),
            jobs:        Arc::new(Mutex::new(HashMap::new())),
            next_job_id: Arc::new(AtomicU64::new(1)),
        })
    }

    /// In-memory database (tests).
    pub fn in_memory() -> anyhow::Result<Self> {
        let db = Database::open_in_memory()?;
        Ok(Self {
            db:          Arc::new(Mutex::new(db)),
            jobs:        Arc::new(Mutex::new(HashMap::new())),
            next_job_id: Arc::new(AtomicU64::new(1)),
        })
    }

    /// Allocate the next monotonically increasing job ID.
    pub fn alloc_job_id(&self) -> u64 {
        self.next_job_id.fetch_add(1, Ordering::SeqCst)
    }
}
