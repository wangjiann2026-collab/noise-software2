pub mod barrier;
pub mod building;
pub mod terrain;

pub use barrier::Barrier;
pub use building::Building;
pub use terrain::Terrain;

use nalgebra::{Point3, Unit, Vector3};

/// Trait for any surface that can reflect acoustic rays.
pub trait ReflectorSurface: Send + Sync {
    /// Normal vector of the surface at a given point.
    fn normal_at(&self, point: &Point3<f64>) -> Unit<Vector3<f64>>;
    /// Reflection loss coefficient per octave band (0.0 = perfect reflector, 1.0 = full absorber).
    fn absorption_coefficients(&self) -> &[f64];
    /// Test if a ray segment [from → to] intersects this surface.
    /// Returns intersection point and parameter t ∈ [0,1] if hit.
    fn intersect_segment(&self, from: &Point3<f64>, to: &Point3<f64>) -> Option<(Point3<f64>, f64)>;
}
