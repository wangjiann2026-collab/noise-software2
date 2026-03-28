use nalgebra::Point3;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TrainType { Passenger, Freight, HighSpeed, Urban }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrainFlow {
    pub train_type: TrainType,
    pub flow_day: f64,
    pub flow_evening: f64,
    pub flow_night: f64,
    pub speed_kmh: f64,
    pub axle_count: u32,
    pub wagon_count: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum RailCondition { #[default] Good, Average, Poor }

/// Railway noise source (CNOSSOS-EU rail model).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RailwaySource {
    pub id: u64,
    pub name: String,
    pub vertices: Vec<Point3<f64>>,
    pub train_flows: Vec<TrainFlow>,
    pub rail_condition: RailCondition,
    pub track_count: u8,
    pub source_height_m: f64,
    pub sample_spacing_m: f64,
}

impl RailwaySource {
    pub fn total_length_m(&self) -> f64 {
        self.vertices.windows(2).map(|w| (w[1] - w[0]).norm()).sum()
    }
}
