use nalgebra::Point3;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum VehicleCategory { Cat1, Cat2, Cat3, Cat4, Cat5 }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrafficFlow {
    pub category: VehicleCategory,
    pub flow_day: f64,
    pub flow_evening: f64,
    pub flow_night: f64,
    pub speed_kmh: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum RoadSurface {
    #[default] DenseAsphalt,
    PorousAsphalt,
    Concrete,
    Cobblestones,
}

/// Road traffic noise source (CNOSSOS-EU road model).
///
/// When `emission_lw_db` is set, it is used directly as the per-sample
/// sound power (converted from a flat A-weighted spectrum).  When it is
/// `None` the CNOSSOS traffic-flow model is used instead.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoadSource {
    pub id: u64,
    pub name: String,
    pub vertices: Vec<Point3<f64>>,
    pub traffic_flows: Vec<TrafficFlow>,
    pub surface: RoadSurface,
    pub gradient_pct: f64,
    pub source_height_m: f64,
    pub sample_spacing_m: f64,
    /// Direct A-weighted sound power level (dBA re 1 pW / metre of road).
    /// When set this bypasses the traffic-flow model.
    /// `None` → fall back to traffic flows (or the default 80 dB stub).
    #[serde(default)]
    pub emission_lw_db: Option<f64>,
}

impl RoadSource {
    pub fn total_length_m(&self) -> f64 {
        self.vertices.windows(2).map(|w| (w[1] - w[0]).norm()).sum()
    }

    /// Total daily vehicle count across all categories.
    pub fn total_daily_flow(&self) -> f64 {
        self.traffic_flows.iter().map(|f| (f.flow_day + f.flow_evening + f.flow_night) * 1.0).sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_road(flows: Vec<TrafficFlow>) -> RoadSource {
        RoadSource {
            id: 1, name: "Test Road".into(),
            vertices: vec![Point3::origin(), Point3::new(100.0, 0.0, 0.0)],
            traffic_flows: flows,
            surface: RoadSurface::DenseAsphalt,
            gradient_pct: 0.0,
            source_height_m: 0.05,
            sample_spacing_m: 5.0,
        }
    }

    #[test]
    fn total_length_correct() {
        let road = make_road(vec![]);
        assert!((road.total_length_m() - 100.0).abs() < 1e-9);
    }

    #[test]
    fn total_daily_flow_sums_all_categories() {
        let flows = vec![
            TrafficFlow { category: VehicleCategory::Cat1, flow_day: 1000.0, flow_evening: 100.0, flow_night: 50.0, speed_kmh: 50.0 },
            TrafficFlow { category: VehicleCategory::Cat3, flow_day: 50.0,   flow_evening: 10.0,  flow_night: 5.0,  speed_kmh: 50.0 },
        ];
        let road = make_road(flows);
        assert!((road.total_daily_flow() - 1215.0).abs() < 1e-6);
    }
}
