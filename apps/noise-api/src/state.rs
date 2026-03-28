//! Shared application state for the noise-api server.
//!
//! [`AppState`] is cloned into every request handler via Axum's
//! `State<AppState>` extractor.  The inner `Arc<Mutex<Database>>` ensures
//! a single `rusqlite::Connection` is shared safely across the Tokio thread
//! pool.  `jobs` tracks in-flight and completed calculation jobs in memory.
//!
//! ## Job progress fan-out
//! `event_tx` is a [`tokio::sync::broadcast`] sender that carries [`JobEvent`]
//! messages.  Handlers broadcast events as calculations progress; WebSocket
//! clients subscribe via `event_tx.subscribe()`.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicU64, Ordering};
use noise_data::db::Database;
use serde::Serialize;
use tokio::sync::broadcast;

// ─── Job events ───────────────────────────────────────────────────────────────

/// Events published on the broadcast channel during job execution.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum JobEvent {
    /// Calculation is running — `pct` is 0–99.
    Progress {
        job_id:  u64,
        pct:     u8,
        message: String,
    },
    /// Calculation finished successfully.
    Completed {
        job_id:         u64,
        calc_result_id: i64,
    },
    /// Calculation failed.
    Failed {
        job_id: u64,
        error:  String,
    },
}

impl JobEvent {
    /// Extract the job ID from any variant.
    pub fn job_id(&self) -> u64 {
        match self {
            Self::Progress  { job_id, .. } => *job_id,
            Self::Completed { job_id, .. } => *job_id,
            Self::Failed    { job_id, .. } => *job_id,
        }
    }
}

// ─── Job record ───────────────────────────────────────────────────────────────

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

/// Capacity of the broadcast channel (number of events buffered per subscriber).
const BROADCAST_CAPACITY: usize = 256;

/// Shared application state.
///
/// `Clone` is cheap — copies only the `Arc` / channel-sender pointers.
#[derive(Clone)]
pub struct AppState {
    pub db:           Arc<Mutex<Database>>,
    pub jobs:         Arc<Mutex<HashMap<u64, JobRecord>>>,
    pub next_job_id:  Arc<AtomicU64>,
    /// Broadcast sender for job-progress events.
    /// Handlers call `event_tx.send(event)` to notify all WebSocket subscribers.
    pub event_tx:     broadcast::Sender<JobEvent>,
}

impl AppState {
    /// Open (or create) the database at `db_path` and run all migrations.
    pub fn new(db_path: &str) -> anyhow::Result<Self> {
        let db = Database::open(db_path)?;
        let (event_tx, _) = broadcast::channel(BROADCAST_CAPACITY);
        Ok(Self {
            db:          Arc::new(Mutex::new(db)),
            jobs:        Arc::new(Mutex::new(HashMap::new())),
            next_job_id: Arc::new(AtomicU64::new(1)),
            event_tx,
        })
    }

    /// In-memory database (tests).
    pub fn in_memory() -> anyhow::Result<Self> {
        let db = Database::open_in_memory()?;
        let (event_tx, _) = broadcast::channel(BROADCAST_CAPACITY);
        Ok(Self {
            db:          Arc::new(Mutex::new(db)),
            jobs:        Arc::new(Mutex::new(HashMap::new())),
            next_job_id: Arc::new(AtomicU64::new(1)),
            event_tx,
        })
    }

    /// Allocate the next monotonically increasing job ID.
    pub fn alloc_job_id(&self) -> u64 {
        self.next_job_id.fetch_add(1, Ordering::SeqCst)
    }
}
