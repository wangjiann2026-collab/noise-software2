//! # noise-core
//!
//! Core acoustic simulation engine for 3D environmental noise mapping.
//!
//! ## Architecture
//!
//! ```text
//! noise-core
//! ├── engine/      — Ray tracing, angle scanning, propagation models
//! ├── sources/     — Road, railway, point, line noise sources
//! ├── obstacles/   — Buildings, barriers, terrain, reflectors
//! ├── grid/        — Horizontal, vertical, facade calculation grids
//! ├── metrics/     — Ld, Ln, Lden, L10, L1hmax, custom formulas
//! └── parallel/    — Rayon-based parallel computation scheduler
//! ```

pub mod engine;
pub mod grid;
pub mod metrics;
pub mod obstacles;
pub mod parallel;
pub mod sources;

/// Re-export commonly used types.
pub mod prelude {
    pub use crate::engine::{AngleScanner, PropagationModel, RayTracer};
    pub use crate::grid::{FacadeGrid, HorizontalGrid, VerticalGrid};
    pub use crate::metrics::{EvalMetric, MetricResult, NoiseMetrics};
    pub use crate::obstacles::{Barrier, Building, Terrain};
    pub use crate::parallel::ParallelScheduler;
    pub use crate::sources::{LineSource, NoiseSource, PointSource, RailwaySource, RoadSource};
}
