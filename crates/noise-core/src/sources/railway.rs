//! Railway noise source based on CNOSSOS-EU rail emission model.

use super::NoiseSource;
use nalgebra::Point3;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TrainType {
    Passenger,
    Freight,
    HighSpeed,
    Urban,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrainFlow {
    pub train_type: TrainType,
    /// Trains per hour (day).
    pub flow_day: f64,
    /// Trains per hour (evening).
    pub flow_evening: f64,
    /// Trains per hour (night).
    pub flow_night: f64,
    /// Mean speed (km/h).
    pub speed_kmh: f64,
    /// Number of axles per train.
    pub axle_count: u32,
    /// Number of wagons per train.
    pub wagon_count: u32,
}

/// Rail surface roughness condition.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum RailCondition {
    #[default]
    Good,
    Average,
    Poor,
}

/// A railway noise source.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RailwaySource {
    pub id: u64,
    pub name: String,
    /// Track centre-line vertices.
    pub vertices: Vec<Point3<f64>>,
    pub train_flows: Vec<TrainFlow>,
    pub rail_condition: RailCondition,
    /// Number of tracks.
    pub track_count: u8,
    /// Source height above rail (m), typically 0.5 m for rolling noise.
    pub source_height_m: f64,
    /// Sampling spacing along track (m).
    pub sample_spacing_m: f64,
}

impl NoiseSource for RailwaySource {
    fn id(&self) -> u64 { self.id }
    fn name(&self) -> &str { &self.name }

    fn sound_power_db(&self) -> &[f64] {
        // Stub — full CNOSSOS-EU rail emission in Phase 4.
        &[85.0; 8]
    }

    fn sample_points(&self) -> Vec<Point3<f64>> {
        let mut points = Vec::new();
        let h = self.source_height_m;
        let spacing = self.sample_spacing_m.max(0.1);
        for seg in self.vertices.windows(2) {
            let start = seg[0] + nalgebra::Vector3::new(0.0, 0.0, h);
            let end = seg[1] + nalgebra::Vector3::new(0.0, 0.0, h);
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
