//! Angle scanning method for noise source contribution calculation.
//!
//! The angle scanning method divides the source into angular segments and
//! computes the contribution from each segment to the receiver point.
//! Particularly useful for line and surface sources (road, railway).

use nalgebra::{Point3, Vector3};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AngleScanConfig {
    /// Angular resolution in degrees. Smaller = more accurate, slower.
    pub angular_step_deg: f64,
    /// Frequency bands (Hz).
    pub frequency_bands: Vec<f64>,
}

impl Default for AngleScanConfig {
    fn default() -> Self {
        Self {
            angular_step_deg: 1.0,
            frequency_bands: vec![63.0, 125.0, 250.0, 500.0, 1000.0, 2000.0, 4000.0, 8000.0],
        }
    }
}

/// Represents a single angular segment contribution.
#[derive(Debug, Clone)]
pub struct SegmentContribution {
    /// Azimuth angle from receiver (degrees).
    pub azimuth_deg: f64,
    /// Elevation angle from receiver (degrees).
    pub elevation_deg: f64,
    /// Sound power level contributed from this segment (dB re 1 pW).
    pub lw_contribution_db: Vec<f64>,
}

/// Angle scanning engine for distributed source types.
pub struct AngleScanner {
    config: AngleScanConfig,
}

impl AngleScanner {
    pub fn new(config: AngleScanConfig) -> Self {
        Self { config }
    }

    /// Scan all angles from `receiver` and aggregate contributions from
    /// source segments that fall within each angular bin.
    ///
    /// Returns total A-weighted sound pressure level at the receiver.
    pub fn scan(
        &self,
        receiver: &Point3<f64>,
        source_segments: &[(Point3<f64>, f64)], // (midpoint, Lw_dB)
    ) -> Vec<f64> {
        // Stub — full implementation in Phase 4.
        // Returns per-band SPL contribution summed across all visible segments.
        let n_bands = self.config.frequency_bands.len();
        let _ = (receiver, source_segments);
        vec![-f64::INFINITY; n_bands]
    }

    pub fn config(&self) -> &AngleScanConfig {
        &self.config
    }
}

/// Add decibel values: L_total = 10·log10(Σ 10^(Li/10))
pub fn add_db(levels: &[f64]) -> f64 {
    let sum: f64 = levels.iter().map(|&l| 10f64.powf(l / 10.0)).sum();
    if sum <= 0.0 {
        return -f64::INFINITY;
    }
    10.0 * sum.log10()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn add_db_equal_sources_increases_by_3db() {
        let level = 60.0;
        let combined = add_db(&[level, level]);
        assert!((combined - 63.01).abs() < 0.01, "Got {combined}");
    }

    #[test]
    fn add_db_single_returns_same() {
        assert!((add_db(&[75.0]) - 75.0).abs() < 1e-6);
    }

    #[test]
    fn add_db_empty_returns_neg_inf() {
        assert_eq!(add_db(&[]).to_bits(), f64::NEG_INFINITY.to_bits());
    }
}
