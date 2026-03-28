use nalgebra::Point3;
use serde::{Deserialize, Serialize};

/// Barrier top profile.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum BarrierTop {
    #[default] Flat,
    Curved,
    TShape,
    YShape,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum BarrierMaterial {
    #[default] Concrete,
    Absorptive, // e.g., mineral wool panels
    Transparent,
    Earth,
}

impl BarrierMaterial {
    pub fn absorption_coefficients(self) -> [f64; 8] {
        match self {
            Self::Concrete    => [0.02, 0.02, 0.03, 0.03, 0.04, 0.05, 0.05, 0.05],
            Self::Absorptive  => [0.15, 0.25, 0.50, 0.80, 0.90, 0.90, 0.85, 0.80],
            Self::Transparent => [0.10, 0.07, 0.05, 0.04, 0.03, 0.03, 0.03, 0.03],
            Self::Earth       => [0.50, 0.60, 0.70, 0.80, 0.85, 0.85, 0.80, 0.75],
        }
    }
}

/// A noise barrier (vertical wall along a polyline).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Barrier {
    pub id: u64,
    pub name: String,
    pub vertices: Vec<Point3<f64>>,
    pub height_m: f64,
    pub material: BarrierMaterial,
    pub top_profile: BarrierTop,
    /// If true, both sides have the same material (default).
    pub symmetric: bool,
}

impl Barrier {
    pub fn new(id: u64, name: impl Into<String>, vertices: Vec<Point3<f64>>, height_m: f64) -> Self {
        Self {
            id, name: name.into(), vertices, height_m,
            material: BarrierMaterial::Concrete,
            top_profile: BarrierTop::Flat,
            symmetric: true,
        }
    }

    pub fn total_length_m(&self) -> f64 {
        self.vertices.windows(2).map(|w| (w[1] - w[0]).norm()).sum()
    }

    pub fn surface_area_m2(&self) -> f64 {
        self.total_length_m() * self.height_m
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn barrier_length_and_area() {
        let b = Barrier::new(1, "B", vec![
            Point3::new(0.0, 0.0, 0.0),
            Point3::new(100.0, 0.0, 0.0),
        ], 5.0);
        assert!((b.total_length_m() - 100.0).abs() < 1e-9);
        assert!((b.surface_area_m2() - 500.0).abs() < 1e-9);
    }

    #[test]
    fn absorptive_material_has_higher_coefficients() {
        let abs = BarrierMaterial::Absorptive.absorption_coefficients();
        let con = BarrierMaterial::Concrete.absorption_coefficients();
        // Mid-frequency (index 3 = 500Hz) should be much higher for absorptive.
        assert!(abs[3] > con[3] * 5.0);
    }
}
