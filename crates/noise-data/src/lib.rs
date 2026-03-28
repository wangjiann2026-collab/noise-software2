//! # noise-data
//!
//! Data models, SQLite persistence, scenario variant management,
//! geometric transformation utilities, and object repositories.

pub mod db;
pub mod entities;
pub mod repository;
pub mod scenario;
pub mod transform;

pub mod prelude {
    pub use crate::db::Database;
    pub use crate::entities::{ObjectType, SceneObject};
    pub use crate::repository::{
        CalculationRepository, ProjectRepository, RepoError,
        SceneObjectRepository, UserRepository, StoredUser,
    };
    pub use crate::scenario::{Project, Scenario, ScenarioVariant, VariantResolver};
    pub use crate::transform::GeometricTransform;
}
