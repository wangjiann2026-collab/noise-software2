use super::NoiseSource;
use nalgebra::Point3;
use serde::{Deserialize, Serialize};

/// A line noise source defined by a polyline (e.g., generic industrial line).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LineSource {
    pub id: u64,
    pub name: String,
    /// Ordered list of 3D vertices defining the polyline.
    pub vertices: Vec<Point3<f64>>,
    /// Sound power level per unit length per octave band (dB/m).
    pub lw_per_meter_db: [f64; 8],
    /// Sampling interval along the line (m) for discrete point approximation.
    pub sample_spacing_m: f64,
}

impl LineSource {
    /// Total length of the polyline (m).
    pub fn total_length_m(&self) -> f64 {
        self.vertices
            .windows(2)
            .map(|w| (w[1] - w[0]).norm())
            .sum()
    }
}

impl NoiseSource for LineSource {
    fn id(&self) -> u64 { self.id }
    fn name(&self) -> &str { &self.name }

    fn sound_power_db(&self) -> &[f64] { &self.lw_per_meter_db }

    fn sample_points(&self) -> Vec<Point3<f64>> {
        let mut points = Vec::new();
        let spacing = self.sample_spacing_m.max(0.1);
        for seg in self.vertices.windows(2) {
            let start = seg[0];
            let end = seg[1];
            let dir = end - start;
            let len = dir.norm();
            if len < 1e-9 { continue; }
            let unit = dir / len;
            let mut t = 0.0;
            while t < len {
                points.push(start + unit * t);
                t += spacing;
            }
        }
        points
    }
}
