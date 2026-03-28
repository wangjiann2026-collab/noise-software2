use super::NoiseSource;
use nalgebra::Point3;
use serde::{Deserialize, Serialize};

/// A stationary point noise source (e.g., industrial equipment, HVAC).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PointSource {
    pub id: u64,
    pub name: String,
    /// 3D position (x, y, z) in project coordinate system.
    pub position: Point3<f64>,
    /// Sound power level per octave band [63, 125, 250, 500, 1k, 2k, 4k, 8k] Hz (dBW).
    pub lw_db: [f64; 8],
}

impl NoiseSource for PointSource {
    fn id(&self) -> u64 { self.id }
    fn name(&self) -> &str { &self.name }
    fn sound_power_db(&self) -> &[f64] { &self.lw_db }
    fn sample_points(&self) -> Vec<Point3<f64>> { vec![self.position] }
}
