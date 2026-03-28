use nalgebra::Point3;
use serde::{Deserialize, Serialize};

/// Building material affecting acoustic reflection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum FacadeMaterial {
    #[default] Concrete,
    Brick,
    Glass,
    Wood,
    MetalPanel,
}

impl FacadeMaterial {
    /// Absorption coefficients per octave band [63–8k Hz].
    pub fn absorption_coefficients(self) -> [f64; 8] {
        match self {
            Self::Concrete   => [0.02, 0.02, 0.03, 0.03, 0.04, 0.05, 0.05, 0.05],
            Self::Brick      => [0.03, 0.03, 0.03, 0.04, 0.05, 0.07, 0.07, 0.07],
            Self::Glass      => [0.18, 0.06, 0.04, 0.03, 0.02, 0.02, 0.02, 0.02],
            Self::Wood       => [0.15, 0.11, 0.10, 0.07, 0.06, 0.07, 0.07, 0.07],
            Self::MetalPanel => [0.05, 0.04, 0.03, 0.03, 0.03, 0.04, 0.04, 0.04],
        }
    }
}

/// A building: 2.5D extruded polygon with optional per-floor data.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Building {
    pub id: u64,
    pub name: String,
    /// Ground-level footprint vertices (counter-clockwise, closed polygon).
    pub footprint: Vec<Point3<f64>>,
    /// Building height above ground (m).
    pub height_m: f64,
    /// Number of floors (for facade receptor positioning).
    pub floors: u32,
    pub facade_material: FacadeMaterial,
    /// Whether to compute facade noise levels for this building.
    pub compute_facade: bool,
    /// Population (for impact assessment).
    pub population: Option<u32>,
}

impl Building {
    pub fn new(
        id: u64,
        name: impl Into<String>,
        footprint: Vec<Point3<f64>>,
        height_m: f64,
    ) -> Self {
        Self {
            id,
            name: name.into(),
            footprint,
            height_m,
            floors: ((height_m / 3.0).round() as u32).max(1),
            facade_material: FacadeMaterial::Concrete,
            compute_facade: true,
            population: None,
        }
    }

    /// Approximate floor area (m²) using the shoelace formula.
    pub fn floor_area_m2(&self) -> f64 {
        let n = self.footprint.len();
        if n < 3 { return 0.0; }
        let mut area = 0.0_f64;
        for i in 0..n {
            let j = (i + 1) % n;
            area += self.footprint[i].x * self.footprint[j].y;
            area -= self.footprint[j].x * self.footprint[i].y;
        }
        area.abs() / 2.0
    }

    /// Point-in-polygon test (XY plane only).
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
            if ((yi > y) != (yj > y)) && x < (xj - xi) * (y - yi) / (yj - yi) + xi {
                inside = !inside;
            }
            j = i;
        }
        inside
    }

    pub fn absorption_coefficients(&self) -> [f64; 8] {
        self.facade_material.absorption_coefficients()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn square_building(side: f64) -> Building {
        Building::new(1, "B1", vec![
            Point3::new(0.0, 0.0, 0.0),
            Point3::new(side, 0.0, 0.0),
            Point3::new(side, side, 0.0),
            Point3::new(0.0, side, 0.0),
        ], 12.0)
    }

    #[test]
    fn floor_area_correct() {
        let b = square_building(10.0);
        assert!((b.floor_area_m2() - 100.0).abs() < 1e-6);
    }

    #[test]
    fn point_in_polygon_inside() {
        let b = square_building(10.0);
        assert!(b.contains_xy(5.0, 5.0));
    }

    #[test]
    fn point_in_polygon_outside() {
        let b = square_building(10.0);
        assert!(!b.contains_xy(15.0, 5.0));
    }

    #[test]
    fn floors_inferred_from_height() {
        let b = square_building(10.0); // height=12m → 4 floors
        assert_eq!(b.floors, 4);
    }
}
