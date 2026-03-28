use super::ReflectorSurface;
use nalgebra::{Point3, Unit, Vector3};
use serde::{Deserialize, Serialize};

/// A noise barrier (sound wall) defined by a vertical wall polyline.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Barrier {
    pub id: u64,
    pub name: String,
    /// Ground-level vertices of the barrier centreline.
    pub vertices: Vec<Point3<f64>>,
    /// Height of the barrier (m).
    pub height_m: f64,
    /// Façade absorption coefficient per octave band (both sides).
    pub absorption_coeffs: [f64; 8],
}

impl ReflectorSurface for Barrier {
    fn normal_at(&self, _point: &Point3<f64>) -> Unit<Vector3<f64>> {
        // Stub: returns a placeholder normal; full per-segment normal in Phase 4.
        Unit::new_normalize(Vector3::x())
    }

    fn absorption_coefficients(&self) -> &[f64] {
        &self.absorption_coeffs
    }

    fn intersect_segment(&self, _from: &Point3<f64>, _to: &Point3<f64>) -> Option<(Point3<f64>, f64)> {
        None
    }
}
