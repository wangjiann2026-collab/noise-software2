//! Building facade noise calculation grid.

use nalgebra::Point3;
use serde::{Deserialize, Serialize};

/// A facade grid attached to one wall of a building.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FacadeGrid {
    pub id: u64,
    pub building_id: u64,
    /// Name of the building wall (e.g., "North", "South").
    pub wall_name: String,
    /// Ordered vertices of the wall base line.
    pub base_vertices: Vec<Point3<f64>>,
    /// Wall height (m).
    pub wall_height_m: f64,
    /// Horizontal spacing (m).
    pub dx: f64,
    /// Vertical spacing (m).
    pub dz: f64,
    /// Standoff distance from wall surface (m), typically 0.1 m.
    pub standoff_m: f64,
    /// Calculated noise levels (dBA), row-major.
    pub results: Vec<f32>,
}

impl FacadeGrid {
    pub fn point_count(&self) -> usize {
        let wall_len: f64 = self.base_vertices
            .windows(2)
            .map(|w| (w[1] - w[0]).norm())
            .sum();
        let nx = (wall_len / self.dx).ceil() as usize + 1;
        let nz = (self.wall_height_m / self.dz).ceil() as usize + 1;
        nx * nz
    }
}
