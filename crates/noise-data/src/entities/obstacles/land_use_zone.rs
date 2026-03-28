use nalgebra::Point3;
use serde::{Deserialize, Serialize};

/// Land use category for sensitivity classification.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum LandUseCategory {
    Residential,
    Commercial,
    Industrial,
    Educational,
    Healthcare,
    Recreational,
    Mixed,
    #[default] Unclassified,
}

impl LandUseCategory {
    /// Recommended limit levels (EU Directive 2002/49/EC) for Lden / Ln (dBA).
    pub fn recommended_limits_dba(self) -> (f64, f64) {
        match self {
            Self::Residential   => (55.0, 45.0),
            Self::Commercial    => (65.0, 55.0),
            Self::Industrial    => (70.0, 60.0),
            Self::Educational   => (55.0, 45.0),
            Self::Healthcare    => (50.0, 40.0),
            Self::Recreational  => (58.0, 48.0),
            Self::Mixed         => (60.0, 50.0),
            Self::Unclassified  => (65.0, 55.0),
        }
    }
}

/// A land use zone polygon.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LandUseZone {
    pub id: u64,
    pub name: String,
    pub boundary: Vec<Point3<f64>>,
    pub category: LandUseCategory,
    /// Optional population count within this zone.
    pub population: Option<u32>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn residential_limit_lower_than_industrial() {
        let (res_lden, _) = LandUseCategory::Residential.recommended_limits_dba();
        let (ind_lden, _) = LandUseCategory::Industrial.recommended_limits_dba();
        assert!(res_lden < ind_lden);
    }
}
