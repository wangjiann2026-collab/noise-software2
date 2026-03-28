use nalgebra::Point3;
use serde::{Deserialize, Serialize};

/// Ground type classification for acoustic impedance.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum GroundClass {
    /// G=0: Hard (asphalt, concrete, water surface).
    Hard,
    /// G=0.3: Mixed hard (parking, compacted gravel).
    MixedHard,
    /// G=0.5: Mixed.
    Mixed,
    /// G=0.7: Mixed soft (lawn, light vegetation).
    MixedSoft,
    /// G=1.0: Soft (farmland, dense vegetation, forest).
    #[default]
    Soft,
    /// User-defined G value.
    Custom(u8), // stored as G*100 to keep it Copy
}

impl GroundClass {
    pub fn g_factor(self) -> f64 {
        match self {
            Self::Hard      => 0.0,
            Self::MixedHard => 0.3,
            Self::Mixed     => 0.5,
            Self::MixedSoft => 0.7,
            Self::Soft      => 1.0,
            Self::Custom(v) => v as f64 / 100.0,
        }
    }
}

/// A ground absorption zone (polygon).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GroundAbsorption {
    pub id: u64,
    pub name: String,
    /// Polygon boundary vertices.
    pub boundary: Vec<Point3<f64>>,
    pub ground_class: GroundClass,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn g_factors_in_range() {
        assert_eq!(GroundClass::Hard.g_factor(), 0.0);
        assert_eq!(GroundClass::Soft.g_factor(), 1.0);
        assert!((GroundClass::Custom(75).g_factor() - 0.75).abs() < 1e-9);
    }
}
