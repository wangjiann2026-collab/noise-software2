use nalgebra::Point3;
use serde::{Deserialize, Serialize};

/// Ground impedance categories (ISO 9613-2 Table 3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum GroundType {
    /// G = 0: Hard ground (asphalt, concrete, water).
    Hard,
    /// G = 0.5: Mixed ground.
    Mixed,
    /// G = 1: Soft ground (farmland, vegetation).
    #[default]
    Soft,
}

impl GroundType {
    /// Flow resistivity factor G (0.0 = hard, 1.0 = soft).
    pub fn g_factor(&self) -> f64 {
        match self {
            Self::Hard => 0.0,
            Self::Mixed => 0.5,
            Self::Soft => 1.0,
        }
    }
}

/// A terrain patch represented as a regular grid of elevation values.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Terrain {
    pub id: u64,
    pub name: String,
    /// Origin of the grid (south-west corner).
    pub origin: Point3<f64>,
    /// Grid resolution (m).
    pub cell_size_m: f64,
    /// Number of columns (X direction).
    pub cols: usize,
    /// Number of rows (Y direction).
    pub rows: usize,
    /// Elevation values in row-major order (m above datum).
    pub elevations: Vec<f32>,
    pub ground_type: GroundType,
}

impl Terrain {
    /// Bilinear interpolation of elevation at (x, y).
    pub fn elevation_at(&self, x: f64, y: f64) -> Option<f64> {
        let lx = (x - self.origin.x) / self.cell_size_m;
        let ly = (y - self.origin.y) / self.cell_size_m;
        if lx < 0.0 || ly < 0.0 { return None; }
        let col = lx.floor() as usize;
        let row = ly.floor() as usize;
        if col + 1 >= self.cols || row + 1 >= self.rows { return None; }

        let fx = lx.fract();
        let fy = ly.fract();
        let idx = |r: usize, c: usize| r * self.cols + c;

        let z00 = self.elevations[idx(row, col)] as f64;
        let z10 = self.elevations[idx(row + 1, col)] as f64;
        let z01 = self.elevations[idx(row, col + 1)] as f64;
        let z11 = self.elevations[idx(row + 1, col + 1)] as f64;

        Some(z00 * (1.0 - fx) * (1.0 - fy)
            + z01 * fx * (1.0 - fy)
            + z10 * (1.0 - fx) * fy
            + z11 * fx * fy)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn flat_terrain(elev: f32) -> Terrain {
        Terrain {
            id: 1,
            name: "flat".into(),
            origin: Point3::new(0.0, 0.0, 0.0),
            cell_size_m: 1.0,
            cols: 3,
            rows: 3,
            elevations: vec![elev; 9],
            ground_type: GroundType::Soft,
        }
    }

    #[test]
    fn flat_terrain_returns_constant_elevation() {
        let t = flat_terrain(42.0);
        let z = t.elevation_at(1.0, 1.0).unwrap();
        assert!((z - 42.0).abs() < 1e-4);
    }

    #[test]
    fn out_of_bounds_returns_none() {
        let t = flat_terrain(0.0);
        assert!(t.elevation_at(-1.0, 0.0).is_none());
        assert!(t.elevation_at(100.0, 0.0).is_none());
    }
}
