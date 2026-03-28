//! Noise-exposure statistics from a calculated grid.
//!
//! Provides [`ExposureStats`] and helper functions to quantify how many grid
//! cells exceed WHO or EU Environmental Noise Directive threshold values.
//!
//! ## Quick usage
//! ```rust
//! use noise_core::metrics::exposure::{compute_exposure, WHO_THRESHOLDS};
//!
//! let levels: Vec<f32> = vec![48.0, 52.0, 56.0, 61.0, 68.0, 72.0, 55.0, 44.0];
//! let stats = compute_exposure(&levels, &WHO_THRESHOLDS);
//! assert_eq!(stats.valid_receivers, 8);
//! ```

use serde::{Deserialize, Serialize};

// ─── Standard threshold sets ──────────────────────────────────────────────────

/// WHO 2018 Environmental Noise Guidelines threshold levels (dBA).
pub const WHO_THRESHOLDS: [f64; 3] = [53.0, 58.0, 65.0];

/// EU Environmental Noise Directive (Directive 2002/49/EC) action-value levels.
pub const EU_END_THRESHOLDS: [f64; 3] = [55.0, 65.0, 70.0];

// ─── Data types ───────────────────────────────────────────────────────────────

/// How many grid cells fall within each noise band.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NoiseBand {
    /// Lower bound of the band, inclusive (dBA).
    pub lower_db: f64,
    /// Upper bound of the band, exclusive (dBA).  `None` = unbounded upper.
    pub upper_db: Option<f64>,
    /// Human-readable label, e.g. `"55–65 dB"` or `"> 70 dB"`.
    pub label: String,
    /// Number of receiver points in this band.
    pub count: usize,
    /// Percentage of *valid* receivers in this band.
    pub pct: f64,
}

/// Aggregated exposure statistics for a calculated noise grid.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExposureStats {
    /// Total number of receiver points in the grid (including NODATA).
    pub total_receivers: usize,
    /// Number of receivers with a finite, positive level.
    pub valid_receivers: usize,
    /// Minimum level across valid receivers (dBA).
    pub min_db:    f64,
    /// Maximum level across valid receivers (dBA).
    pub max_db:    f64,
    /// Mean level across valid receivers (dBA).
    pub mean_db:   f64,
    /// Median level across valid receivers (dBA).
    pub median_db: f64,
    /// 95th-percentile level across valid receivers (dBA).
    pub p95_db: f64,
    /// Number and percentage of receivers exceeding each threshold.
    pub above_thresholds: Vec<ThresholdExceedance>,
    /// Distribution across noise bands defined by the thresholds.
    pub bands: Vec<NoiseBand>,
}

/// Exceedance count for a single threshold.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThresholdExceedance {
    pub threshold_db: f64,
    pub count_above:  usize,
    pub pct_above:    f64,
}

// ─── Functions ────────────────────────────────────────────────────────────────

/// Compute full exposure statistics for a noise grid.
///
/// # Parameters
/// - `levels` — flat array of dBA values (non-finite = NODATA/no-source)
/// - `thresholds` — sorted ascending list of ISO threshold levels (dBA)
///
/// If `thresholds` is empty, [`EU_END_THRESHOLDS`] are used.
pub fn compute_exposure(levels: &[f32], thresholds: &[f64]) -> ExposureStats {
    let effective_thresholds: &[f64] = if thresholds.is_empty() {
        &EU_END_THRESHOLDS
    } else {
        thresholds
    };

    // Collect valid levels.
    let mut valid: Vec<f64> = levels
        .iter()
        .filter(|&&v| v.is_finite() && v > 0.0)
        .map(|&v| v as f64)
        .collect();

    let total = levels.len();
    let n_valid = valid.len();

    if n_valid == 0 {
        return ExposureStats {
            total_receivers:  total,
            valid_receivers:  0,
            min_db: 0.0, max_db: 0.0, mean_db: 0.0,
            median_db: 0.0, p95_db: 0.0,
            above_thresholds: effective_thresholds.iter().map(|&t| ThresholdExceedance {
                threshold_db: t, count_above: 0, pct_above: 0.0,
            }).collect(),
            bands: build_bands(&[], effective_thresholds, 0),
        };
    }

    valid.sort_by(|a, b| a.partial_cmp(b).unwrap());

    let min_db  = valid[0];
    let max_db  = *valid.last().unwrap();
    let mean_db = valid.iter().sum::<f64>() / n_valid as f64;
    let median_db = percentile(&valid, 50.0);
    let p95_db    = percentile(&valid, 95.0);

    let above_thresholds: Vec<ThresholdExceedance> = effective_thresholds
        .iter()
        .map(|&t| {
            let count_above = valid.iter().filter(|&&v| v >= t).count();
            ThresholdExceedance {
                threshold_db: t,
                count_above,
                pct_above: if n_valid > 0 { count_above as f64 / n_valid as f64 * 100.0 } else { 0.0 },
            }
        })
        .collect();

    let bands = build_bands(&valid, effective_thresholds, n_valid);

    ExposureStats {
        total_receivers: total,
        valid_receivers: n_valid,
        min_db:    (min_db  * 10.0).round() / 10.0,
        max_db:    (max_db  * 10.0).round() / 10.0,
        mean_db:   (mean_db * 10.0).round() / 10.0,
        median_db: (median_db * 10.0).round() / 10.0,
        p95_db:    (p95_db  * 10.0).round() / 10.0,
        above_thresholds,
        bands,
    }
}

/// Build noise-band distribution from sorted valid levels.
fn build_bands(sorted: &[f64], thresholds: &[f64], n_valid: usize) -> Vec<NoiseBand> {
    if thresholds.is_empty() || n_valid == 0 {
        return Vec::new();
    }

    let mut boundaries: Vec<f64> = thresholds.to_vec();
    boundaries.sort_by(|a, b| a.partial_cmp(b).unwrap());

    let mut bands = Vec::new();

    // Band: below first threshold.
    let first = boundaries[0];
    let count_below = sorted.iter().filter(|&&v| v < first).count();
    bands.push(NoiseBand {
        lower_db: 0.0, upper_db: Some(first),
        label: format!("< {first} dB"),
        count: count_below,
        pct: count_below as f64 / n_valid as f64 * 100.0,
    });

    // Bands: between thresholds.
    for window in boundaries.windows(2) {
        let lo = window[0];
        let hi = window[1];
        let count = sorted.iter().filter(|&&v| v >= lo && v < hi).count();
        bands.push(NoiseBand {
            lower_db: lo, upper_db: Some(hi),
            label: format!("{lo}–{hi} dB"),
            count,
            pct: count as f64 / n_valid as f64 * 100.0,
        });
    }

    // Band: above last threshold.
    let last = *boundaries.last().unwrap();
    let count_above = sorted.iter().filter(|&&v| v >= last).count();
    bands.push(NoiseBand {
        lower_db: last, upper_db: None,
        label: format!("> {last} dB"),
        count: count_above,
        pct: count_above as f64 / n_valid as f64 * 100.0,
    });

    bands
}

/// Interpolated percentile from a sorted ascending slice.
fn percentile(sorted: &[f64], pct: f64) -> f64 {
    if sorted.is_empty() { return 0.0; }
    let rank = pct / 100.0 * (sorted.len() - 1) as f64;
    let lo = rank.floor() as usize;
    let hi = (lo + 1).min(sorted.len() - 1);
    let frac = rank - lo as f64;
    sorted[lo] + frac * (sorted[hi] - sorted[lo])
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_levels() -> Vec<f32> {
        // 10 points spanning 40–85 dB
        vec![40.0, 45.0, 50.0, 55.0, 60.0, 65.0, 70.0, 75.0, 80.0, 85.0]
    }

    #[test]
    fn basic_stats_correct() {
        let stats = compute_exposure(&sample_levels(), &EU_END_THRESHOLDS);
        assert_eq!(stats.total_receivers, 10);
        assert_eq!(stats.valid_receivers, 10);
        assert_eq!(stats.min_db, 40.0);
        assert_eq!(stats.max_db, 85.0);
        assert!((stats.mean_db - 62.5).abs() < 0.1);
    }

    #[test]
    fn median_is_middle_value() {
        // Sorted: 40..85 — median between 60 and 65 = 62.5
        let stats = compute_exposure(&sample_levels(), &EU_END_THRESHOLDS);
        assert!((stats.median_db - 62.5).abs() < 0.5);
    }

    #[test]
    fn p95_is_near_top() {
        let stats = compute_exposure(&sample_levels(), &EU_END_THRESHOLDS);
        assert!(stats.p95_db >= 80.0, "P95 should be ≥ 80 dB, got {}", stats.p95_db);
    }

    #[test]
    fn above_thresholds_count() {
        let stats = compute_exposure(&sample_levels(), &EU_END_THRESHOLDS);
        // Levels: 40,45,50,55,60,65,70,75,80,85
        // Above 55 (≥ 55): 55,60,65,70,75,80,85 = 7
        let t55 = stats.above_thresholds.iter().find(|t| t.threshold_db == 55.0).unwrap();
        assert_eq!(t55.count_above, 7);
        // Above 65 (≥ 65): 65,70,75,80,85 = 5
        let t65 = stats.above_thresholds.iter().find(|t| t.threshold_db == 65.0).unwrap();
        assert_eq!(t65.count_above, 5);
    }

    #[test]
    fn band_counts_sum_to_valid() {
        let stats = compute_exposure(&sample_levels(), &EU_END_THRESHOLDS);
        let total: usize = stats.bands.iter().map(|b| b.count).sum();
        assert_eq!(total, stats.valid_receivers);
    }

    #[test]
    fn nodata_excluded_from_stats() {
        let levels: Vec<f32> = vec![f32::NEG_INFINITY, 60.0, 0.0, -1.0, 65.0];
        let stats = compute_exposure(&levels, &EU_END_THRESHOLDS);
        assert_eq!(stats.total_receivers, 5);
        assert_eq!(stats.valid_receivers, 2); // only 60.0 and 65.0 are valid
    }

    #[test]
    fn empty_levels_no_panic() {
        let stats = compute_exposure(&[], &EU_END_THRESHOLDS);
        assert_eq!(stats.total_receivers, 0);
        assert_eq!(stats.valid_receivers, 0);
        assert_eq!(stats.min_db, 0.0);
    }

    #[test]
    fn default_thresholds_used_when_empty() {
        let stats = compute_exposure(&sample_levels(), &[]);
        assert!(!stats.above_thresholds.is_empty());
    }

    #[test]
    fn band_percentages_sum_to_100() {
        let stats = compute_exposure(&sample_levels(), &EU_END_THRESHOLDS);
        let total_pct: f64 = stats.bands.iter().map(|b| b.pct).sum();
        assert!((total_pct - 100.0).abs() < 0.5,
            "Band percentages should sum to 100, got {total_pct:.1}");
    }
}
