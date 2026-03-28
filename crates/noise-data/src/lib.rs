//! # noise-data
//!
//! Data models, SQLite persistence, scenario variant management,
//! and geometric transformation utilities.

pub mod db;
pub mod entities;
pub mod scenario;
pub mod transform;

pub mod prelude {
    pub use crate::db::Database;
    pub use crate::entities::{ReceiverPoint, SceneObject};
    pub use crate::scenario::{Project, Scenario, ScenarioVariant};
    pub use crate::transform::GeometricTransform;
}
