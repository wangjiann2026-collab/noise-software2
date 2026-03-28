//! Shared application state for the noise-api server.
//!
//! [`AppState`] is cloned into every request handler via Axum's
//! `State<AppState>` extractor.  The inner `Arc<Mutex<Database>>` ensures
//! exactly one `rusqlite::Connection` is shared safely across the Tokio
//! thread pool (SQLite is single-writer; concurrent reads go through the
//! same mutex for simplicity).

use std::sync::{Arc, Mutex};
use noise_data::db::Database;

/// Shared application state.
///
/// `Clone` is cheap — it copies the `Arc` pointer.
#[derive(Clone)]
pub struct AppState {
    pub db: Arc<Mutex<Database>>,
}

impl AppState {
    /// Open (or create) the database at `db_path` and run all migrations.
    pub fn new(db_path: &str) -> anyhow::Result<Self> {
        let db = Database::open(db_path)?;
        Ok(Self { db: Arc::new(Mutex::new(db)) })
    }

    /// In-memory database for tests.
    pub fn in_memory() -> anyhow::Result<Self> {
        let db = Database::open_in_memory()?;
        Ok(Self { db: Arc::new(Mutex::new(db)) })
    }
}
