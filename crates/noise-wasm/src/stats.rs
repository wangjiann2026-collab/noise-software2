//! Grid statistics binding.
//!
//! Computes min/max/mean/count from a flat noise-level grid,
//! suitable for display in a web map legend.

use serde::{Deserialize, Serialize};

/// Summary statistics for a noise-level grid.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GridStats {
    /// Minimum finite level (dBA).
    pub min_db: f64,
    /// Maximum finite level (dBA).
    pub max_db: f64,
    /// Mean of finite levels (dBA).
    pub mean_db: f64,
    /// Number of finite (non-NODATA) cells.
    pub count: usize,
    /// Total cells including NODATA.
    pub total: usize,
    /// Percentage of cells with level ≥ 55 dBA (typical WHO limit).
    pub exceed_55_pct: f64,
    /// Percentage of cells with level ≥ 65 dBA (WHO night limit).
    pub exceed_65_pct: f64,
}

/// Compute statistics from a flat noise-level grid.
///
/// Values ≤ 0 or non-finite are treated as NODATA.
///
/// # Example
/// ```
/// use noise_wasm::stats::grid_stats;
/// let levels = vec![55.0f32, 60.0, 65.0, 70.0, -9999.0];
/// let s = grid_stats(&levels).unwrap();
/// assert_eq!(s.count, 4);
/// assert!((s.min_db - 55.0).abs() < 0.01);
/// assert!((s.max_db - 70.0).abs() < 0.01);
/// ```
pub fn grid_stats(levels: &[f32]) -> Option<GridStats> {
    let finite: Vec<f64> = levels.iter()
        .copied()
        .filter(|&v| v.is_finite() && v > 0.0)
        .map(|v| v as f64)
        .collect();

    if finite.is_empty() {
        return None;
    }

    let count  = finite.len();
    let total  = levels.len();
    let min_db = finite.iter().copied().fold(f64::INFINITY, f64::min);
    let max_db = finite.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    let mean_db = finite.iter().sum::<f64>() / count as f64;

    let exceed_55 = finite.iter().filter(|&&v| v >= 55.0).count();
    let exceed_65 = finite.iter().filter(|&&v| v >= 65.0).count();

    Some(GridStats {
        min_db:  (min_db  * 100.0).round() / 100.0,
        max_db:  (max_db  * 100.0).round() / 100.0,
        mean_db: (mean_db * 100.0).round() / 100.0,
        count,
        total,
        exceed_55_pct: exceed_55 as f64 / count as f64 * 100.0,
        exceed_65_pct: exceed_65 as f64 / count as f64 * 100.0,
    })
}

// ─── wasm-bindgen exports ─────────────────────────────────────────────────────

#[cfg(feature = "wasm")]
mod wasm_exports {
    use super::*;
    use wasm_bindgen::prelude::*;

    /// Compute grid statistics from a flat Float32Array.
    ///
    /// Returns a JSON string containing `GridStats`, or `null` if all NODATA.
    #[wasm_bindgen(js_name = gridStats)]
    pub fn wasm_grid_stats(levels: &[f32]) -> JsValue {
        match grid_stats(levels) {
            Some(s) => serde_wasm_bindgen::to_value(&s).unwrap_or(JsValue::NULL),
            None    => JsValue::NULL,
        }
    }
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Vec<f32> {
        vec![55.0, 60.0, 65.0, 70.0, f32::NEG_INFINITY, -9999.0, 0.0]
    }

    #[test]
    fn count_excludes_nodata() {
        let s = grid_stats(&sample()).unwrap();
        assert_eq!(s.count, 4);
        assert_eq!(s.total, 7);
    }

    #[test]
    fn min_max_correct() {
        let s = grid_stats(&sample()).unwrap();
        assert!((s.min_db - 55.0).abs() < 0.01);
        assert!((s.max_db - 70.0).abs() < 0.01);
    }

    #[test]
    fn mean_correct() {
        let s = grid_stats(&sample()).unwrap();
        let expected = (55.0 + 60.0 + 65.0 + 70.0) / 4.0;
        assert!((s.mean_db - expected).abs() < 0.01);
    }

    #[test]
    fn exceedance_55_pct() {
        let s = grid_stats(&sample()).unwrap();
        // All 4 finite values ≥ 55 → 100%
        assert!((s.exceed_55_pct - 100.0).abs() < 0.01);
    }

    #[test]
    fn exceedance_65_pct() {
        let s = grid_stats(&sample()).unwrap();
        // 65 and 70 ≥ 65 → 2/4 = 50%
        assert!((s.exceed_65_pct - 50.0).abs() < 0.01);
    }

    #[test]
    fn all_nodata_returns_none() {
        let levels = vec![f32::NEG_INFINITY, -9999.0, 0.0];
        assert!(grid_stats(&levels).is_none());
    }

    #[test]
    fn uniform_grid() {
        let levels = vec![60.0f32; 100];
        let s = grid_stats(&levels).unwrap();
        assert_eq!(s.count, 100);
        assert!((s.min_db - 60.0).abs() < 0.01);
        assert!((s.max_db - 60.0).abs() < 0.01);
        assert!((s.mean_db - 60.0).abs() < 0.01);
    }
}
