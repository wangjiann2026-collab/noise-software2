use super::ReflectorSurface;
use nalgebra::{Point3, Unit, Vector3};
use serde::{Deserialize, Serialize};

/// A building represented as a 2.5D extruded polygon.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Building {
    pub id: u64,
    pub name: String,
    /// Ground-level footprint vertices (counter-clockwise).
    pub footprint: Vec<Point3<f64>>,
    /// Building height above ground (m).
    pub height_m: f64,
    /// Façade absorption coefficient per octave band.
    pub absorption_coeffs: [f64; 8],
    /// Reflection loss (dB) per octave band from façade material.
    pub reflection_loss_db: [f64; 8],
}

impl Building {
    /// Approximate axis-aligned bounding box check (XY plane).
    pub fn contains_xy(&self, x: f64, y: f64) -> bool {
        let n = self.footprint.len();
        if n < 3 { return false; }
        let mut inside = false;
        let mut j = n - 1;
        for i in 0..n {
            let xi = self.footprint[i].x;
            let yi = self.footprint[i].y;
            let xj = self.footprint[j].x;
            let yj = self.footprint[j].y;
            if ((yi > y) != (yj > y)) && (x < (xj - xi) * (y - yi) / (yj - yi) + xi) {
                inside = !inside;
            }
            j = i;
        }
        inside
    }
}

impl ReflectorSurface for Building {
    fn normal_at(&self, _point: &Point3<f64>) -> Unit<Vector3<f64>> {
        Unit::new_normalize(Vector3::z())
    }

    fn absorption_coefficients(&self) -> &[f64] {
        &self.absorption_coeffs
    }

    fn intersect_segment(&self, _from: &Point3<f64>, _to: &Point3<f64>) -> Option<(Point3<f64>, f64)> {
        // Full intersection test implemented in Phase 4.
        None
    }
}
