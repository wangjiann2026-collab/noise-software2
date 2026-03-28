//! Standard noise evaluation metrics.
//!
//! Implements: Ld, Ln, Le, Ldn, Lden, Lde, Len, L10, L1hmax per EU Directive 2002/49/EC.

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Standard evaluation metric types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum EvalMetric {
    /// Day level: 07:00–19:00 (12 h).
    Ld,
    /// Evening level: 19:00–23:00 (4 h).
    Le,
    /// Night level: 23:00–07:00 (8 h).
    Ln,
    /// Day-night level (FHWA, USA): Ln + 10 dB penalty.
    Ldn,
    /// Day-evening-night level (EU Directive 2002/49/EC).
    Lden,
    /// Day-evening combined.
    Lde,
    /// Evening-night combined.
    Len,
    /// Statistical level exceeded 10% of the time.
    L10,
    /// Maximum 1-hour equivalent level.
    L1hMax,
}

impl EvalMetric {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Ld => "Ld",
            Self::Le => "Le",
            Self::Ln => "Ln",
            Self::Ldn => "Ldn",
            Self::Lden => "Lden",
            Self::Lde => "Lde",
            Self::Len => "Len",
            Self::L10 => "L10",
            Self::L1hMax => "L1hmax",
        }
    }
}

/// Result of a noise metric calculation at a single receiver point.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricResult {
    pub metric: EvalMetric,
    /// Calculated level (dBA).
    pub level_dba: f64,
}

#[derive(Debug, Error)]
pub enum MetricsError {
    #[error("Period hours must sum to 24, got {0}")]
    InvalidPeriodHours(f64),
    #[error("Insufficient data for L10 calculation (need ≥ 10 samples)")]
    InsufficientData,
}

/// Input for standard metrics calculation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PeriodLevels {
    /// Equivalent continuous A-weighted level during day period (dBA).
    pub leq_day_dba: f64,
    /// Equivalent continuous A-weighted level during evening period (dBA).
    pub leq_evening_dba: f64,
    /// Equivalent continuous A-weighted level during night period (dBA).
    pub leq_night_dba: f64,
    /// Day period hours (EU default: 12h).
    pub day_hours: f64,
    /// Evening period hours (EU default: 4h).
    pub evening_hours: f64,
    /// Night period hours (EU default: 8h).
    pub night_hours: f64,
    /// Evening penalty (dB), EU default: 5 dB.
    pub evening_penalty_db: f64,
    /// Night penalty (dB), EU default: 10 dB.
    pub night_penalty_db: f64,
}

impl Default for PeriodLevels {
    fn default() -> Self {
        Self {
            leq_day_dba: 0.0,
            leq_evening_dba: 0.0,
            leq_night_dba: 0.0,
            day_hours: 12.0,
            evening_hours: 4.0,
            night_hours: 8.0,
            evening_penalty_db: 5.0,
            night_penalty_db: 10.0,
        }
    }
}

/// Calculator for all standard noise metrics.
pub struct NoiseMetrics;

impl NoiseMetrics {
    /// Lden = 10·log10[ (12·10^(Ld/10) + 4·10^((Le+5)/10) + 8·10^((Ln+10)/10)) / 24 ]
    pub fn lden(p: &PeriodLevels) -> Result<f64, MetricsError> {
        let total = p.day_hours + p.evening_hours + p.night_hours;
        if (total - 24.0).abs() > 0.5 {
            return Err(MetricsError::InvalidPeriodHours(total));
        }
        let sum = p.day_hours * db_to_linear(p.leq_day_dba)
            + p.evening_hours * db_to_linear(p.leq_evening_dba + p.evening_penalty_db)
            + p.night_hours * db_to_linear(p.leq_night_dba + p.night_penalty_db);
        Ok(linear_to_db(sum / 24.0))
    }

    /// Ldn = 10·log10[ (15·10^(Ld/10) + 9·10^((Ln+10)/10)) / 24 ]
    pub fn ldn(leq_day: f64, leq_night: f64) -> f64 {
        let sum = 15.0 * db_to_linear(leq_day) + 9.0 * db_to_linear(leq_night + 10.0);
        linear_to_db(sum / 24.0)
    }

    /// Leq combination of two periods.
    pub fn combine(leq1: f64, hours1: f64, leq2: f64, hours2: f64) -> f64 {
        let sum = hours1 * db_to_linear(leq1) + hours2 * db_to_linear(leq2);
        linear_to_db(sum / (hours1 + hours2))
    }

    /// Statistical L10: level exceeded 10% of the time.
    /// Requires a sorted sample vector (ascending).
    pub fn l10(sorted_samples_dba: &[f64]) -> Result<f64, MetricsError> {
        if sorted_samples_dba.len() < 10 {
            return Err(MetricsError::InsufficientData);
        }
        let idx = (sorted_samples_dba.len() as f64 * 0.90).ceil() as usize;
        Ok(sorted_samples_dba[idx.min(sorted_samples_dba.len() - 1)])
    }

    /// Compute all standard metrics from period levels.
    pub fn compute_all(p: &PeriodLevels) -> Result<Vec<MetricResult>, MetricsError> {
        let lden = Self::lden(p)?;
        let ldn = Self::ldn(p.leq_day_dba, p.leq_night_dba);
        let lde = Self::combine(p.leq_day_dba, p.day_hours, p.leq_evening_dba, p.evening_hours);
        let len = Self::combine(p.leq_evening_dba, p.evening_hours, p.leq_night_dba, p.night_hours);

        Ok(vec![
            MetricResult { metric: EvalMetric::Ld,   level_dba: p.leq_day_dba },
            MetricResult { metric: EvalMetric::Le,   level_dba: p.leq_evening_dba },
            MetricResult { metric: EvalMetric::Ln,   level_dba: p.leq_night_dba },
            MetricResult { metric: EvalMetric::Lden, level_dba: lden },
            MetricResult { metric: EvalMetric::Ldn,  level_dba: ldn },
            MetricResult { metric: EvalMetric::Lde,  level_dba: lde },
            MetricResult { metric: EvalMetric::Len,  level_dba: len },
        ])
    }
}

#[inline]
fn db_to_linear(db: f64) -> f64 {
    10f64.powf(db / 10.0)
}

#[inline]
fn linear_to_db(linear: f64) -> f64 {
    if linear <= 0.0 { return -f64::INFINITY; }
    10.0 * linear.log10()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn default_periods(ld: f64, le: f64, ln: f64) -> PeriodLevels {
        PeriodLevels { leq_day_dba: ld, leq_evening_dba: le, leq_night_dba: ln, ..Default::default() }
    }

    #[test]
    fn lden_equal_periods_applies_penalties() {
        // When all period levels are equal, evening+5dB and night+10dB penalties
        // make Lden > Ld.
        let p = default_periods(60.0, 60.0, 60.0);
        let lden = NoiseMetrics::lden(&p).unwrap();
        assert!(lden > 60.0, "Lden {lden} should exceed Ld 60.0 due to penalties");
    }

    #[test]
    fn lden_known_value() {
        // Reference calculation: Ld=65, Le=55, Ln=50
        // Day:     12 * 10^(65/10)   = 12 * 3_162_277.66 = 37_947_331.9
        // Evening:  4 * 10^((55+5)/10)= 4  * 1_000_000    =  4_000_000.0
        // Night:    8 * 10^((50+10)/10)= 8  * 1_000_000    =  8_000_000.0
        // sum/24 = 49_947_331.9 / 24 = 2_081_138.8
        // Lden = 10 * log10(2_081_138.8) ≈ 63.18 dB
        let p = default_periods(65.0, 55.0, 50.0);
        let lden = NoiseMetrics::lden(&p).unwrap();
        assert!((lden - 63.18).abs() < 0.05, "Got Lden = {lden:.3}");
    }

    #[test]
    fn l10_returns_90th_percentile() {
        let samples: Vec<f64> = (0..100).map(|i| i as f64).collect();
        let l10 = NoiseMetrics::l10(&samples).unwrap();
        assert!(l10 >= 90.0 && l10 <= 91.0, "Got {l10}");
    }

    #[test]
    fn l10_insufficient_data_errors() {
        assert!(matches!(NoiseMetrics::l10(&[60.0; 5]), Err(MetricsError::InsufficientData)));
    }

    #[test]
    fn combine_equal_levels_returns_same() {
        let result = NoiseMetrics::combine(60.0, 6.0, 60.0, 6.0);
        assert!((result - 60.0).abs() < 0.01, "Got {result}");
    }
}
