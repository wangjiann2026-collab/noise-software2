//! Repository pattern for all scene object types.
//!
//! Each repository wraps a `rusqlite::Connection` and provides typed CRUD
//! operations. All queries use prepared statements to prevent SQL injection.

pub mod calculations;
pub mod projects;
pub mod scene_objects;
pub mod users;

pub use calculations::CalculationRepository;
pub use projects::ProjectRepository;
pub use scene_objects::SceneObjectRepository;
pub use users::{StoredUser, UserRepository};

use thiserror::Error;

#[derive(Debug, Error)]
pub enum RepoError {
    #[error("SQLite error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("Serialization error: {0}")]
    Serde(#[from] serde_json::Error),
    #[error("Object not found: id={0}")]
    NotFound(u64),
    #[error("Scenario not found: {0}")]
    ScenarioNotFound(String),
    #[error("Validation error: {0}")]
    Validation(String),
}
