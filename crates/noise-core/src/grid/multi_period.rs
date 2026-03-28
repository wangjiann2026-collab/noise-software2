//! Multi-period grid calculation for EU Directive 2002/49/EC composite metrics.
//!
//! Runs three separate grid calculations (day / evening / night) and combines
//! them cell-by-cell using the standard time-weighted energy formula.
//!
//! ## Lden formula (EU Directive 2002/49/EC)
//! ```text
//! Lden = 10 · log₁₀ [
//!     (12·10^(Ld/10) + 4·10^((Le+5)/10) + 8·10^((Ln+10)/10)) / 24
//! ]
//! ```
//!
//! ## Usage
//! ```rust
//! use noise_core::grid::{CalculatorConfig, SourceSpec, GridCalculator};
//! use noise_core::grid::multi_period::{MultiPeriodConfig, MultiPeriodGridCalculator};
//! use noise_core::grid::HorizontalGrid;
//! use nalgebra::Point3;
//!
//! let src = SourceSpec { id: 1, position: Point3::new(50.0, 0.0, 0.5),
//!     lw_db: [80.0; 8], g_source: 0.0 };
//!
//! let config = MultiPeriodConfig::default();
//! let calc = MultiPeriodGridCalculator::new(CalculatorConfig::default(), config);
//!
//! let mut grid = HorizontalGrid::new(1, "lden", Point3::new(0.0, 0.0, 0.0),
//!     10.0, 10.0, 5, 5, 4.0);
//! calc.calculate_lden(&mut grid, &[src], &[]);
//! assert!(!grid.results.is_empty());
//! ```

use super::{BarrierSpec, CalculatorConfig, GridCalculator, HorizontalGrid, SourceSpec};

// ─── Multi-period configuration ──────────────────────────────────────────────

/// EU 2002/49/EC time-period parameters.
#[derive(Debug, Clone)]
pub struct MultiPeriodConfig {
    /// Day period duration (hours). Default: 12.
    pub day_hours: f64,
    /// Evening period duration (hours). Default: 4.
    pub evening_hours: f64,
    /// Night period duration (hours). Default: 8.
    pub night_hours: f64,
    /// Evening penalty (dB). Default: 5 (EU).
    pub evening_penalty_db: f64,
    /// Night penalty (dB). Default: 10 (EU).
    pub night_penalty_db: f64,
    /// Sound-power offset applied to sources during evening (dB relative to day).
    /// Negative = quieter evening traffic.  Default: 0.
    pub evening_source_offset_db: f64,
    /// Sound-power offset applied to sources during night (dB relative to day).
    /// Default: 0.  Typical road: −3 to −5 dB for reduced flow.
    pub night_source_offset_db: f64,
}

impl Default for MultiPeriodConfig {
    fn default() -> Self {
        Self {
            day_hours:               12.0,
            evening_hours:            4.0,
            night_hours:              8.0,
            evening_penalty_db:       5.0,
            night_penalty_db:        10.0,
            evening_source_offset_db: 0.0,
            night_source_offset_db:   0.0,
        }
    }
}

// ─── Calculator ───────────────────────────────────────────────────────────────

/// Grid calculator that computes composite noise metrics (Lden / Ldn) by
/// running three separate propagation calculations and combining per-cell.
pub struct MultiPeriodGridCalculator {
    calc_config: CalculatorConfig,
    period_config: MultiPeriodConfig,
}

impl MultiPeriodGridCalculator {
    pub fn new(calc_config: CalculatorConfig, period_config: MultiPeriodConfig) -> Self {
        Self { calc_config, period_config }
    }

    /// Compute the EU Directive Lden grid in-place.
    ///
    /// 1. Day calculation — sources at nominal Lw.
    /// 2. Evening calculation — sources adjusted by `evening_source_offset_db`.
    /// 3. Night calculation — sources adjusted by `night_source_offset_db`.
    /// 4. Combine per cell: Lden = EU energy-weighted average with penalties.
    pub fn calculate_lden(
        &self,
        grid: &mut HorizontalGrid,
        sources: &[SourceSpec],
        barriers: &[BarrierSpec],
    ) {
        let [day, evening, night] = self.run_three_periods(grid, sources, barriers);
        let pc = &self.period_config;
        let total_h = pc.day_hours + pc.evening_hours + pc.night_hours;

        grid.results = (0..day.len())
            .map(|i| {
                let ld = day[i] as f64;
                let le = evening[i] as f64;
                let ln = night[i] as f64;
                if !ld.is_finite() && !le.is_finite() && !ln.is_finite() {
                    return f32::NEG_INFINITY;
                }
                let sum = pc.day_hours     * pow10(ld / 10.0)
                    + pc.evening_hours * pow10((le + pc.evening_penalty_db) / 10.0)
                    + pc.night_hours   * pow10((ln + pc.night_penalty_db)   / 10.0);
                (10.0 * (sum / total_h).log10()) as f32
            })
            .collect();
    }

    /// Compute the FHWA Ldn grid in-place.
    ///
    /// Ldn = 10·log₁₀[ (15·10^(Ld/10) + 9·10^((Ln+10)/10)) / 24 ]
    ///
    /// Only the day and night calculations are run.
    pub fn calculate_ldn(
        &self,
        grid: &mut HorizontalGrid,
        sources: &[SourceSpec],
        barriers: &[BarrierSpec],
    ) {
        let [day, _evening, night] = self.run_three_periods(grid, sources, barriers);

        grid.results = (0..day.len())
            .map(|i| {
                let ld = day[i] as f64;
                let ln = night[i] as f64;
                if !ld.is_finite() && !ln.is_finite() {
                    return f32::NEG_INFINITY;
                }
                let sum = 15.0 * pow10(ld / 10.0)
                    + 9.0  * pow10((ln + 10.0) / 10.0);
                (10.0 * (sum / 24.0).log10()) as f32
            })
            .collect();
    }

    // ── Private helpers ───────────────────────────────────────────────────────

    /// Run day / evening / night and return [day_levels, evening_levels, night_levels].
    fn run_three_periods(
        &self,
        grid: &HorizontalGrid,
        sources: &[SourceSpec],
        barriers: &[BarrierSpec],
    ) -> [Vec<f32>; 3] {
        let day_sources = sources.to_vec();
        let evening_sources = apply_offset(sources, self.period_config.evening_source_offset_db);
        let night_sources   = apply_offset(sources, self.period_config.night_source_offset_db);

        let run = |srcs: Vec<SourceSpec>| -> Vec<f32> {
            let mut g = grid.clone();
            GridCalculator::new(self.calc_config.clone())
                .calculate(&mut g, &srcs, barriers, None);
            g.results
        };

        let day_levels     = run(day_sources);
        let evening_levels = run(evening_sources);
        let night_levels   = run(night_sources);
        [day_levels, evening_levels, night_levels]
    }
}

/// Apply a uniform dB offset to all sources' octave-band Lw.
fn apply_offset(sources: &[SourceSpec], offset_db: f64) -> Vec<SourceSpec> {
    if offset_db.abs() < 1e-9 {
        return sources.to_vec();
    }
    sources.iter().map(|s| {
        let mut lw = s.lw_db;
        for v in lw.iter_mut() { *v += offset_db; }
        SourceSpec { lw_db: lw, ..*s }
    }).collect()
}

#[inline]
fn pow10(x: f64) -> f64 {
    10f64.powf(x)
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use nalgebra::Point3;

    fn test_grid() -> HorizontalGrid {
        HorizontalGrid::new(1, "test", Point3::new(0.0, 0.0, 0.0),
            10.0, 10.0, 3, 3, 4.0)
    }

    fn test_source() -> SourceSpec {
        SourceSpec { id: 1, position: Point3::new(15.0, 15.0, 0.5),
            lw_db: [80.0; 8], g_source: 0.0 }
    }

    #[test]
    fn lden_produces_non_empty_results() {
        let calc = MultiPeriodGridCalculator::new(
            CalculatorConfig::default(),
            MultiPeriodConfig::default(),
        );
        let mut grid = test_grid();
        calc.calculate_lden(&mut grid, &[test_source()], &[]);
        assert_eq!(grid.results.len(), 9);
        assert!(grid.results.iter().any(|&v| v.is_finite() && v > 0.0));
    }

    #[test]
    fn ldn_produces_non_empty_results() {
        let calc = MultiPeriodGridCalculator::new(
            CalculatorConfig::default(),
            MultiPeriodConfig::default(),
        );
        let mut grid = test_grid();
        calc.calculate_ldn(&mut grid, &[test_source()], &[]);
        assert_eq!(grid.results.len(), 9);
        assert!(grid.results.iter().any(|&v| v.is_finite() && v > 0.0));
    }

    #[test]
    fn lden_higher_than_ld_with_penalties() {
        // When evening and night levels equal day level, Lden > Ld due to penalties.
        let config = MultiPeriodConfig {
            evening_source_offset_db: 0.0, // same as day
            night_source_offset_db:   0.0,
            ..Default::default()
        };
        let calc = MultiPeriodGridCalculator::new(CalculatorConfig::default(), config);
        let mut g_lden = test_grid();
        calc.calculate_lden(&mut g_lden, &[test_source()], &[]);

        let mut g_ld = test_grid();
        GridCalculator::new(CalculatorConfig::default())
            .calculate(&mut g_ld, &[test_source()], &[], None);

        // Find a finite receiver
        for (lden, ld) in g_lden.results.iter().zip(g_ld.results.iter()) {
            if lden.is_finite() && ld.is_finite() && *ld > 0.0 {
                assert!(*lden > *ld,
                    "Lden {lden:.1} should exceed Ld {ld:.1}");
                return;
            }
        }
    }

    #[test]
    fn night_offset_reduces_lden() {
        // More negative night offset → lower Lden.
        let config_strong = MultiPeriodConfig {
            night_source_offset_db: -10.0,
            ..Default::default()
        };
        let config_zero = MultiPeriodConfig::default(); // night offset = 0

        let calc_strong = MultiPeriodGridCalculator::new(CalculatorConfig::default(), config_strong);
        let calc_zero   = MultiPeriodGridCalculator::new(CalculatorConfig::default(), config_zero);

        let src = test_source();
        let mut g_strong = test_grid();
        let mut g_zero   = test_grid();
        calc_strong.calculate_lden(&mut g_strong, &[src.clone()], &[]);
        calc_zero.calculate_lden(&mut g_zero, &[src], &[]);

        for (s, z) in g_strong.results.iter().zip(g_zero.results.iter()) {
            if s.is_finite() && z.is_finite() && *z > 0.0 {
                assert!(*s <= *z + 0.1,
                    "stronger night reduction should give lower Lden: {s:.1} vs {z:.1}");
                return;
            }
        }
    }

    #[test]
    fn apply_offset_zero_is_identity() {
        let sources = vec![test_source()];
        let result = apply_offset(&sources, 0.0);
        assert_eq!(result[0].lw_db, sources[0].lw_db);
    }

    #[test]
    fn apply_offset_shifts_all_bands() {
        let sources = vec![test_source()]; // lw_db = [80.0; 8]
        let result = apply_offset(&sources, -3.0);
        for &v in &result[0].lw_db {
            assert!((v - 77.0).abs() < 1e-9);
        }
    }

    #[test]
    fn lden_no_sources_gives_neg_infinity_or_zero() {
        let calc = MultiPeriodGridCalculator::new(
            CalculatorConfig::default(), MultiPeriodConfig::default(),
        );
        let mut grid = test_grid();
        calc.calculate_lden(&mut grid, &[], &[]);
        for &v in &grid.results {
            assert!(!v.is_finite() || v == 0.0,
                "empty sources should give non-finite or zero levels, got {v}");
        }
    }
}
