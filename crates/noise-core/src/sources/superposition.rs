//! Multi-source energy superposition for noise mapping.
//!
//! Combines sound pressure levels (SPL) from multiple sources at a receiver
//! point using energy (incoherent) summation — the standard approach for
//! environmental noise assessment.
//!
//! # Energy summation
//!   L_total = 10·log₁₀(Σᵢ 10^(Lᵢ/10))

use serde::{Deserialize, Serialize};

/// A single source contribution at a receiver point.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceContribution {
    /// Numeric source identifier.
    pub source_id: u64,
    /// A-weighted SPL contribution (dBA).
    pub lp_dba: f64,
    /// Per-octave-band SPL (dB), 8 bands 63–8000 Hz.
    pub lp_bands_db: [f64; 8],
    /// Propagation distance (m).
    pub distance_m: f64,
}

/// Combined result at a single receiver.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReceiverResult {
    /// Receiver index within the grid (row-major).
    pub receiver_index: usize,
    /// A-weighted total SPL (dBA).
    pub lp_total_dba: f64,
    /// Per-band total SPL (dB).
    pub lp_total_bands_db: [f64; 8],
    /// Individual contributions (one per source).
    pub contributions: Vec<SourceContribution>,
}

/// A-weighting offsets for octave bands [63, 125, 250, 500, 1k, 2k, 4k, 8k] Hz.
const A_WEIGHTS: [f64; 8] = [-26.2, -16.1, -8.6, -3.2, 0.0, 1.2, 1.0, -1.1];

impl ReceiverResult {
    /// Compute a `ReceiverResult` from a list of per-band SPL contributions.
    ///
    /// Each entry in `band_contributions` is `(source_id, distance_m, lp_bands_db)`.
    pub fn from_band_contributions(
        receiver_index: usize,
        band_contributions: Vec<(u64, f64, [f64; 8])>,
    ) -> Self {
        let mut total_linear = [0.0f64; 8];
        let mut contributions = Vec::with_capacity(band_contributions.len());

        for (source_id, distance_m, lp_bands) in band_contributions {
            // Accumulate energy per band.
            for i in 0..8 {
                if lp_bands[i].is_finite() {
                    total_linear[i] += 10f64.powf(lp_bands[i] / 10.0);
                }
            }
            // A-weighted contribution from this source.
            let lp_dba = a_weighted_sum(&lp_bands);
            contributions.push(SourceContribution { source_id, lp_dba, lp_bands_db: lp_bands, distance_m });
        }

        let lp_total_bands_db: [f64; 8] = total_linear.map(|v| {
            if v > 0.0 { 10.0 * v.log10() } else { -f64::INFINITY }
        });
        let lp_total_dba = a_weighted_sum(&lp_total_bands_db);

        ReceiverResult { receiver_index, lp_total_dba, lp_total_bands_db, contributions }
    }
}

/// Energy sum of A-weighted octave band levels.
fn a_weighted_sum(bands: &[f64; 8]) -> f64 {
    let sum: f64 = bands.iter().zip(A_WEIGHTS.iter())
        .filter(|(l, _)| l.is_finite())
        .map(|(&l, &a)| 10f64.powf((l + a) / 10.0))
        .sum();
    if sum <= 0.0 { -f64::INFINITY } else { 10.0 * sum.log10() }
}

/// Combine A-weighted SPL levels from multiple sources (incoherent summation).
///
/// This is the standard environmental noise superposition formula.
///
/// # Example
/// ```
/// # use noise_core::sources::superposition::combine_dba;
/// let total = combine_dba(&[60.0, 60.0]);
/// assert!((total - 63.01).abs() < 0.02);
/// ```
pub fn combine_dba(levels_dba: &[f64]) -> f64 {
    let sum: f64 = levels_dba.iter()
        .filter(|l| l.is_finite())
        .map(|&l| 10f64.powf(l / 10.0))
        .sum();
    if sum <= 0.0 { -f64::INFINITY } else { 10.0 * sum.log10() }
}

/// Per-band energy superposition of octave-band arrays.
pub fn combine_bands(contributions: &[[f64; 8]]) -> [f64; 8] {
    let mut total = [0.0f64; 8];
    for bands in contributions {
        for i in 0..8 {
            if bands[i].is_finite() {
                total[i] += 10f64.powf(bands[i] / 10.0);
            }
        }
    }
    total.map(|v| if v > 0.0 { 10.0 * v.log10() } else { -f64::INFINITY })
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use approx::assert_abs_diff_eq;

    #[test]
    fn combine_two_equal_sources_adds_3db() {
        let total = combine_dba(&[60.0, 60.0]);
        assert_abs_diff_eq!(total, 63.01, epsilon = 0.02);
    }

    #[test]
    fn combine_single_source_unchanged() {
        let total = combine_dba(&[75.3]);
        assert_abs_diff_eq!(total, 75.3, epsilon = 1e-6);
    }

    #[test]
    fn combine_empty_returns_neg_inf() {
        let total = combine_dba(&[]);
        assert!(total.is_infinite() && total < 0.0);
    }

    #[test]
    fn combine_bands_energy_sum() {
        let b1 = [60.0f64; 8];
        let b2 = [60.0f64; 8];
        let combined = combine_bands(&[b1, b2]);
        for v in &combined {
            assert_abs_diff_eq!(*v, 63.01, epsilon = 0.02);
        }
    }

    #[test]
    fn receiver_result_from_single_source() {
        let bands = [80.0f64; 8];
        let contribs = vec![(1u64, 50.0, bands)];
        let result = ReceiverResult::from_band_contributions(0, contribs);
        assert_eq!(result.receiver_index, 0);
        assert_eq!(result.contributions.len(), 1);
        // Total should equal single source.
        for i in 0..8 {
            assert_abs_diff_eq!(result.lp_total_bands_db[i], 80.0, epsilon = 1e-6);
        }
    }

    #[test]
    fn receiver_result_from_two_sources() {
        let bands = [60.0f64; 8];
        let contribs = vec![(1u64, 30.0, bands), (2u64, 40.0, bands)];
        let result = ReceiverResult::from_band_contributions(0, contribs);
        for i in 0..8 {
            assert_abs_diff_eq!(result.lp_total_bands_db[i], 63.01, epsilon = 0.02);
        }
    }

    #[test]
    fn dominant_source_determines_total() {
        // 80 dB + 60 dB ≈ 80.04 dB (loud source dominates).
        let total = combine_dba(&[80.0, 60.0]);
        assert!(total > 80.0 && total < 80.1, "got {total}");
    }
}
