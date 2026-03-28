use nalgebra::Point3;
use serde::{Deserialize, Serialize};

/// A generic line noise source (polyline of any length).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LineSource {
    pub id: u64,
    pub name: String,
    pub vertices: Vec<Point3<f64>>,
    /// Sound power per unit length per octave band (dBW/m).
    pub lw_per_m_db: [f64; 8],
    pub source_height_m: f64,
}

impl LineSource {
    pub fn total_length_m(&self) -> f64 {
        self.vertices.windows(2).map(|w| (w[1] - w[0]).norm()).sum()
    }
}
