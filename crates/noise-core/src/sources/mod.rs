pub mod line;
pub mod point;
pub mod railway;
pub mod road;

pub use line::LineSource;
pub use point::PointSource;
pub use railway::RailwaySource;
pub use road::RoadSource;

use nalgebra::Point3;
use serde::{Deserialize, Serialize};

/// Unified noise source trait — all source types implement this.
pub trait NoiseSource: Send + Sync {
    /// Unique identifier.
    fn id(&self) -> u64;
    /// Human-readable name.
    fn name(&self) -> &str;
    /// Sound power level per octave band (dB re 1 pW), 8 bands: 63–8000 Hz.
    fn sound_power_db(&self) -> &[f64];
    /// Representative 3D position(s) for ray launching.
    fn sample_points(&self) -> Vec<Point3<f64>>;
}

/// Octave band center frequencies supported throughout the engine.
pub const OCTAVE_BANDS_HZ: [f64; 8] = [63.0, 125.0, 250.0, 500.0, 1000.0, 2000.0, 4000.0, 8000.0];
