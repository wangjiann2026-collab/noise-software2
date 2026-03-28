//! Vertical noise calculation grid (cross-section plane).

use nalgebra::Point3;
use serde::{Deserialize, Serialize};

/// A vertical grid defined by a baseline segment and height range.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerticalGrid {
    pub id: u64,
    pub name: String,
    /// Start point of the baseline (ground level).
    pub start: Point3<f64>,
    /// End point of the baseline (ground level).
    pub end: Point3<f64>,
    /// Horizontal spacing along the baseline (m).
    pub dx: f64,
    /// Vertical spacing (m).
    pub dz: f64,
    /// Number of horizontal divisions.
    pub nx: usize,
    /// Number of vertical divisions.
    pub nz: usize,
    /// Calculated noise levels in row-major order (dBA).
    pub results: Vec<f32>,
}

impl VerticalGrid {
    pub fn point_count(&self) -> usize {
        self.nx * self.nz
    }

    pub fn receiver_points(&self) -> impl Iterator<Item = Point3<f64>> + '_ {
        let dir = (self.end - self.start) / (self.nx.max(1) as f64);
        (0..self.nz).flat_map(move |zi| {
            (0..self.nx).map(move |xi| {
                let base = self.start + dir * xi as f64;
                Point3::new(base.x, base.y, base.z + zi as f64 * self.dz)
            })
        })
    }
}
