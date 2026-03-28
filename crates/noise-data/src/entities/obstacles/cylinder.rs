use nalgebra::Point3;
use serde::{Deserialize, Serialize};

/// A vertical cylinder obstacle (e.g., silo, tank, chimney).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Cylinder {
    pub id: u64,
    pub name: String,
    pub center: Point3<f64>,
    pub radius_m: f64,
    pub height_m: f64,
    pub absorption_coeffs: [f64; 8],
}

impl Cylinder {
    pub fn surface_area_m2(&self) -> f64 {
        2.0 * std::f64::consts::PI * self.radius_m
            * (self.radius_m + self.height_m)
    }
}
