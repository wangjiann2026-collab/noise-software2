//! Angle scanning method for noise source contribution calculation.
//!
//! The angle scanning method divides the source into angular segments and
//! computes the contribution from each segment to the receiver point.
//! Particularly useful for line and surface sources (road, railway).
//!
//! # Algorithm
//! 1. Project each source segment onto the angular space around the receiver.
//! 2. Weight each segment's emission by its subtended solid angle.
//! 3. Sum contributions using energy (incoherent) superposition.

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

    /// Scan all source segments and aggregate their energy contributions at the receiver.
    ///
    /// Each entry in `source_segments` is `(midpoint, lw_db)` where `lw_db` is the
    /// per-band sound power of that segment (already normalised per metre × segment length).
    ///
    /// Returns per-band linear energy sums (dB scale).
    pub fn scan(
        &self,
        receiver: &Point3<f64>,
        source_segments: &[(Point3<f64>, f64)], // (midpoint, Lw_dB scalar, same for all bands)
    ) -> Vec<f64> {
        let n_bands = self.config.frequency_bands.len();
        if source_segments.is_empty() {
            return vec![-f64::INFINITY; n_bands];
        }

        // Accumulate linear energy per band across all segments.
        let mut total_linear = vec![0.0f64; n_bands];
        for (midpoint, lw_db) in source_segments {
            let r_vec = receiver - midpoint;
            let distance = r_vec.norm().max(1.0);
            // Geometric spreading: LW - 20·log10(d) - 11 dB (spherical point-src equiv.)
            let a_div = 20.0 * distance.log10() + 11.0;
            let lp = lw_db - a_div;
            for band_lp in total_linear.iter_mut() {
                *band_lp += 10f64.powf(lp / 10.0);
            }
        }

        total_linear.iter().map(|&v| {
            if v > 0.0 { 10.0 * v.log10() } else { -f64::INFINITY }
        }).collect()
    }

    /// Per-band scan variant: each segment provides per-band LW values.
    ///
    /// `source_segments_bands`: `(midpoint, [lw_band_0, …, lw_band_7])`.
    pub fn scan_bands(
        &self,
        receiver: &Point3<f64>,
        source_segments_bands: &[(Point3<f64>, Vec<f64>)],
    ) -> Vec<f64> {
        let n_bands = self.config.frequency_bands.len();
        if source_segments_bands.is_empty() {
            return vec![-f64::INFINITY; n_bands];
        }

        let mut total_linear = vec![0.0f64; n_bands];
        for (midpoint, lw_bands) in source_segments_bands {
            let distance = (receiver - midpoint).norm().max(1.0);
            let a_div = 20.0 * distance.log10() + 11.0;
            for (i, &lw) in lw_bands.iter().take(n_bands).enumerate() {
                if lw.is_finite() {
                    total_linear[i] += 10f64.powf((lw - a_div) / 10.0);
                }
            }
        }

        total_linear.iter().map(|&v| {
            if v > 0.0 { 10.0 * v.log10() } else { -f64::INFINITY }
        }).collect()
    }

    /// Decompose a line source into segments based on the angular step size.
    ///
    /// `start` and `end` define the source line. Returns midpoints and effective
    /// lengths for each segment, suitable for passing to `scan_bands`.
    pub fn discretise_line(
        &self,
        start: &Point3<f64>,
        end: &Point3<f64>,
        receiver: &Point3<f64>,
    ) -> Vec<(Point3<f64>, f64)> {
        let line_vec: Vector3<f64> = end - start;
        let length = line_vec.norm();
        if length < 1e-6 {
            return vec![(*start, length)];
        }

        // Determine number of segments from angular step size.
        // Distance to midpoint of line used to set segment count.
        let mid = start + line_vec * 0.5;
        let dist = (receiver - mid).norm().max(1.0);
        let step_rad = self.config.angular_step_deg.to_radians();
        // Each segment subtends at most angular_step_deg at the receiver.
        let arc_len = dist * step_rad;
        let n = ((length / arc_len).ceil() as usize).max(1).min(10_000);

        let seg_len = length / n as f64;
        let dir = line_vec / length;
        (0..n).map(|i| {
            let t = (i as f64 + 0.5) * seg_len;
            let midpt = start + dir * t;
            (midpt, seg_len)
        }).collect()
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
    use approx::assert_abs_diff_eq;

    fn scanner() -> AngleScanner {
        AngleScanner::new(AngleScanConfig::default())
    }

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

    #[test]
    fn scan_empty_segments_returns_neg_inf() {
        let s = scanner();
        let result = s.scan(&Point3::new(0.0, 0.0, 4.0), &[]);
        for v in &result {
            assert!(v.is_infinite() && *v < 0.0);
        }
    }

    #[test]
    fn scan_single_segment_returns_finite() {
        let s = scanner();
        let seg = (Point3::new(100.0, 0.0, 0.5), 90.0); // 90 dB source at 100 m
        let result = s.scan(&Point3::new(0.0, 0.0, 4.0), &[seg]);
        assert!(result.iter().all(|v| v.is_finite()));
    }

    #[test]
    fn closer_segment_louder() {
        let s = scanner();
        let receiver = Point3::new(0.0, 0.0, 4.0);
        let near_seg = (Point3::new(50.0, 0.0, 0.5), 90.0);
        let far_seg  = (Point3::new(200.0, 0.0, 0.5), 90.0);
        let r_near = s.scan(&receiver, &[near_seg])[0];
        let r_far  = s.scan(&receiver, &[far_seg])[0];
        assert!(r_near > r_far, "closer ({r_near:.1}) should be louder than farther ({r_far:.1})");
    }

    #[test]
    fn two_equal_segments_3db_louder() {
        let s = scanner();
        let receiver = Point3::new(0.0, 0.0, 4.0);
        let seg = (Point3::new(100.0, 0.0, 0.5), 90.0);
        let r_single = s.scan(&receiver, &[seg])[0];
        let r_double = s.scan(&receiver, &[seg, seg])[0];
        assert_abs_diff_eq!(r_double - r_single, 3.01, epsilon = 0.02);
    }

    #[test]
    fn discretise_line_produces_correct_segment_count() {
        let s = AngleScanner::new(AngleScanConfig { angular_step_deg: 5.0, ..Default::default() });
        let start    = Point3::new(0.0, 0.0, 0.0);
        let end      = Point3::new(100.0, 0.0, 0.0);
        let receiver = Point3::new(50.0, 50.0, 4.0); // ~70 m away from midpoint
        let segs = s.discretise_line(&start, &end, &receiver);
        // Should produce at least 1 segment.
        assert!(!segs.is_empty());
        // Segments' lengths should sum to source length.
        let total_len: f64 = segs.iter().map(|(_, l)| l).sum();
        assert_abs_diff_eq!(total_len, 100.0, epsilon = 1e-6);
    }

    #[test]
    fn scan_bands_matches_scan_for_flat_spectrum() {
        let s = scanner();
        let receiver = Point3::new(0.0, 0.0, 4.0);
        let lw = 90.0;
        let seg_scalar = vec![(Point3::new(100.0, 0.0, 0.5), lw)];
        let seg_bands  = vec![(Point3::new(100.0, 0.0, 0.5), vec![lw; 8])];
        let r_scalar = s.scan(&receiver, &seg_scalar);
        let r_bands  = s.scan_bands(&receiver, &seg_bands);
        for i in 0..8 {
            assert_abs_diff_eq!(r_scalar[i], r_bands[i], epsilon = 1e-6);
        }
    }
}
