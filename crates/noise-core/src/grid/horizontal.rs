//! Horizontal noise calculation grid.

use nalgebra::Point3;
use serde::{Deserialize, Serialize};

/// A rectangular horizontal grid of receiver points.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HorizontalGrid {
    pub id: u64,
    pub name: String,
    /// South-west origin.
    pub origin: Point3<f64>,
    /// Grid spacing in X direction (m).
    pub dx: f64,
    /// Grid spacing in Y direction (m).
    pub dy: f64,
    /// Number of columns.
    pub nx: usize,
    /// Number of rows.
    pub ny: usize,
    /// Receiver height above ground (m).
    pub receiver_height_m: f64,
    /// Calculated noise levels in row-major order (dBA). Empty until computed.
    pub results: Vec<f32>,
}

impl HorizontalGrid {
    pub fn new(
        id: u64,
        name: impl Into<String>,
        origin: Point3<f64>,
        dx: f64,
        dy: f64,
        nx: usize,
        ny: usize,
        receiver_height_m: f64,
    ) -> Self {
        Self {
            id,
            name: name.into(),
            origin,
            dx,
            dy,
            nx,
            ny,
            receiver_height_m,
            results: Vec::new(),
        }
    }

    /// Total number of receiver points.
    pub fn point_count(&self) -> usize {
        self.nx * self.ny
    }

    /// Iterator over all receiver 3D positions.
    pub fn receiver_points(&self) -> impl Iterator<Item = Point3<f64>> + '_ {
        let h = self.receiver_height_m;
        (0..self.ny).flat_map(move |row| {
            (0..self.nx).map(move |col| {
                Point3::new(
                    self.origin.x + col as f64 * self.dx,
                    self.origin.y + row as f64 * self.dy,
                    self.origin.z + h,
                )
            })
        })
    }

    /// Coverage area in m².
    pub fn area_m2(&self) -> f64 {
        (self.nx as f64 * self.dx) * (self.ny as f64 * self.dy)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn point_count_matches_dimensions() {
        let g = HorizontalGrid::new(1, "test", Point3::origin(), 10.0, 10.0, 5, 4, 4.0);
        assert_eq!(g.point_count(), 20);
    }

    #[test]
    fn receiver_points_count_matches() {
        let g = HorizontalGrid::new(1, "test", Point3::origin(), 5.0, 5.0, 3, 3, 4.0);
        assert_eq!(g.receiver_points().count(), 9);
    }
}
