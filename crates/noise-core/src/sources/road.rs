//! Road traffic noise source based on CNOSSOS-EU road emission model.

use super::NoiseSource;
use nalgebra::Point3;
use serde::{Deserialize, Serialize};

/// Vehicle category per CNOSSOS-EU.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum VehicleCategory {
    Cat1, // Passenger cars
    Cat2, // Medium heavy vehicles
    Cat3, // Heavy vehicles
    Cat4, // Two-wheelers
    Cat5, // Open category
}

/// Traffic flow data for one vehicle category.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrafficFlow {
    pub category: VehicleCategory,
    /// Vehicles per hour (day period).
    pub flow_day: f64,
    /// Vehicles per hour (evening period).
    pub flow_evening: f64,
    /// Vehicles per hour (night period).
    pub flow_night: f64,
    /// Mean speed (km/h).
    pub speed_kmh: f64,
}

/// Road surface type affecting rolling noise.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum RoadSurface {
    #[default]
    DenseAsphalt,
    PorousAsphalt,
    Concrete,
    Cobblestones,
}

/// A road traffic noise source.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoadSource {
    pub id: u64,
    pub name: String,
    /// Road centre-line vertices.
    pub vertices: Vec<Point3<f64>>,
    /// Traffic flow by vehicle category.
    pub traffic_flows: Vec<TrafficFlow>,
    pub surface: RoadSurface,
    /// Gradient (%).
    pub gradient_pct: f64,
    /// Source height above road surface (m), typically 0.05 m.
    pub source_height_m: f64,
    /// Sampling spacing along road (m).
    pub sample_spacing_m: f64,
}

impl NoiseSource for RoadSource {
    fn id(&self) -> u64 { self.id }
    fn name(&self) -> &str { &self.name }

    /// Returns equivalent Lw per octave band for day period (stub — full
    /// CNOSSOS-EU emission formula implemented in Phase 4).
    fn sound_power_db(&self) -> &[f64] {
        // Placeholder — will be computed dynamically from traffic flows.
        &[80.0; 8]
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
