use nalgebra::Point3;
use serde::{Deserialize, Serialize};

/// A generic 3D planar reflector (flat panel at any orientation).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Reflector3D {
    pub id: u64,
    pub name: String,
    /// Corner vertices of the reflector panel (3 or 4 points).
    pub vertices: Vec<Point3<f64>>,
    /// Absorption coefficient per octave band.
    pub absorption_coeffs: [f64; 8],
}

impl Reflector3D {
    /// Normal vector computed from the first triangle of vertices.
    pub fn normal(&self) -> Option<nalgebra::Unit<nalgebra::Vector3<f64>>> {
        if self.vertices.len() < 3 { return None; }
        let a = self.vertices[1] - self.vertices[0];
        let b = self.vertices[2] - self.vertices[0];
        Some(nalgebra::Unit::new_normalize(a.cross(&b)))
    }
}
