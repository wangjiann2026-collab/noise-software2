//! Complete ISO 9613-2 sound propagation model.
//!
//! Computes excess attenuation from source to receiver:
//!
//!   A_total = A_div + A_atm + A_gr + A_bar + A_misc
//!
//! where:
//!   A_div  = geometric divergence (spherical spreading)
//!   A_atm  = atmospheric absorption
//!   A_gr   = ground effect (§7.3)
//!   A_bar  = barrier/diffraction attenuation (§7.4)
//!   A_misc = miscellaneous (foliage, housing zones)

use serde::{Deserialize, Serialize};

use super::diffraction::{barrier_attenuation_db, BarrierPath, DiffractionEdge};
use super::ground_effect::{ground_attenuation_db, GroundPath, OCTAVE_BANDS};
use nalgebra::Point3;

// ─── Atmospheric conditions ───────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AtmosphericConditions {
    pub temperature_c: f64,
    pub humidity_pct: f64,
    pub pressure_pa: f64,
}

impl Default for AtmosphericConditions {
    fn default() -> Self {
        Self { temperature_c: 20.0, humidity_pct: 70.0, pressure_pa: 101_325.0 }
    }
}

impl AtmosphericConditions {
    /// Atmospheric absorption coefficient α (dB/m) per octave band.
    /// ISO 9613-1 simplified formula.
    pub fn alpha_db_per_m(&self, f: f64) -> f64 {
        let t = self.temperature_c + 273.15;
        let h = self.humidity_pct;

        let f_ro = 24.0 + 4.04e4 * h * (0.02 + h) / (0.391 + h);
        let f_rn = t.powf(-0.5) * (9.0 + 280.0 * h
            * (-4.17 * ((t / 293.15).powf(-1.0 / 3.0) - 1.0)).exp());

        8.686 * f * f * (
            1.84e-11 * (t / 293.15).powf(0.5)
            + t.powf(-5.0 / 2.0) * (
                0.01275 * (-2239.1 / t).exp() / (f_ro + f * f / f_ro)
                + 0.1068  * (-3352.0 / t).exp() / (f_rn + f * f / f_rn)
            )
        )
    }

    /// A-weighted correction (dB) for each octave band.
    pub const A_WEIGHTS: [f64; 8] = [-26.2, -16.1, -8.6, -3.2, 0.0, 1.2, 1.0, -1.1];
}

// ─── Propagation model ────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub enum ModelStandard {
    #[default] Iso9613_2,
    CnossosEu,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PropagationConfig {
    pub standard: ModelStandard,
    pub atmosphere: AtmosphericConditions,
}

impl Default for PropagationConfig {
    fn default() -> Self {
        Self { standard: ModelStandard::default(), atmosphere: AtmosphericConditions::default() }
    }
}

/// Per-band attenuation breakdown for one propagation path.
#[derive(Debug, Clone)]
pub struct AttenuationBreakdown {
    pub a_div:  [f64; 8],   // geometric spreading
    pub a_atm:  [f64; 8],   // atmospheric absorption
    pub a_gr:   [f64; 8],   // ground effect
    pub a_bar:  [f64; 8],   // barrier diffraction
    pub a_misc: [f64; 8],   // foliage / housing
    pub a_total: [f64; 8],  // sum of all terms
}

impl AttenuationBreakdown {
    /// Convert A-weighted sum of Lw_source − A_total to Lp (SPL, dBA).
    pub fn apply_to_lw(&self, lw_db: &[f64; 8]) -> f64 {
        let sum: f64 = lw_db
            .iter()
            .zip(self.a_total.iter())
            .zip(AtmosphericConditions::A_WEIGHTS.iter())
            .map(|((&lw, &a), &aw)| 10f64.powf((lw - a + aw) / 10.0))
            .sum();
        if sum <= 0.0 { return -f64::INFINITY; }
        10.0 * sum.log10()
    }
}

/// Full ISO 9613-2 propagation model.
pub struct PropagationModel {
    pub config: PropagationConfig,
}

impl PropagationModel {
    pub fn new(config: PropagationConfig) -> Self {
        Self { config }
    }

    /// Compute complete attenuation from `source` to `receiver`.
    ///
    /// - `ground`: ground path parameters
    /// - `barriers`: list of diffracting edges (empty = no barriers)
    /// - `a_misc_db`: additional miscellaneous attenuation (dB), per band
    pub fn compute(
        &self,
        source: &Point3<f64>,
        receiver: &Point3<f64>,
        ground: &GroundPath,
        barriers: &[DiffractionEdge],
        a_misc_db: Option<&[f64; 8]>,
    ) -> AttenuationBreakdown {
        let d = (receiver - source).norm().max(1.0);

        // A_div: geometric spreading
        let a_div_val = 20.0 * d.log10() + 11.0;
        let a_div = [a_div_val; 8];

        // A_atm: atmospheric absorption per band
        let a_atm: [f64; 8] = std::array::from_fn(|i| {
            self.config.atmosphere.alpha_db_per_m(OCTAVE_BANDS[i]) * d
        });

        // A_gr: ground effect
        let a_gr = ground_attenuation_db(ground);

        // A_bar: dominant barrier (highest attenuation wins for single-diffraction)
        let a_bar = self.dominant_barrier_attenuation(source, receiver, barriers);

        // A_misc
        let a_misc: [f64; 8] = a_misc_db.copied().unwrap_or([0.0; 8]);

        // A_total
        let a_total: [f64; 8] = std::array::from_fn(|i| {
            a_div[i] + a_atm[i] + a_gr[i] + a_bar[i] + a_misc[i]
        });

        AttenuationBreakdown { a_div, a_atm, a_gr, a_bar, a_misc, a_total }
    }

    fn dominant_barrier_attenuation(
        &self,
        source: &Point3<f64>,
        receiver: &Point3<f64>,
        barriers: &[DiffractionEdge],
    ) -> [f64; 8] {
        if barriers.is_empty() { return [0.0; 8]; }
        // Select the barrier producing the highest insertion loss (conservative/dominant).
        let mut best = [0.0f64; 8];
        for edge in barriers {
            let path = BarrierPath {
                source: *source,
                receiver: *receiver,
                edge: edge.clone(),
                speed_of_sound: self.speed_of_sound(),
            };
            let a = barrier_attenuation_db(&path);
            for i in 0..8 {
                if a[i] > best[i] { best[i] = a[i]; }
            }
        }
        best
    }

    fn speed_of_sound(&self) -> f64 {
        let t = self.config.atmosphere.temperature_c + 273.15;
        20.05 * t.sqrt()
    }

    /// Simple point-to-point SPL (dBA) with default ground, no barriers.
    pub fn lp_simple(
        &self,
        lw_db: &[f64; 8],
        source: &Point3<f64>,
        receiver: &Point3<f64>,
        g_factor: f64,
    ) -> f64 {
        let d = (receiver - source).norm().max(1.0);
        let ground = GroundPath {
            source_height_m: source.z,
            receiver_height_m: receiver.z,
            distance_m: d,
            g_source: g_factor,
            g_receiver: g_factor,
            g_middle: g_factor,
        };
        let breakdown = self.compute(source, receiver, &ground, &[], None);
        breakdown.apply_to_lw(lw_db)
    }
}

// ─── Decibel utilities ────────────────────────────────────────────────────────

/// Energy summation: L_total = 10·log₁₀(Σ 10^(Lᵢ/10)).
pub fn energy_sum(levels: &[f64]) -> f64 {
    let sum: f64 = levels.iter().map(|&l| 10f64.powf(l / 10.0)).sum();
    if sum <= 0.0 { return -f64::INFINITY; }
    10.0 * sum.log10()
}

/// Leq for a period: Leq = 10·log₁₀((1/T)·Σ tᵢ·10^(Lᵢ/10)).
pub fn leq(levels: &[f64], durations: &[f64]) -> f64 {
    assert_eq!(levels.len(), durations.len());
    let total_t: f64 = durations.iter().sum();
    if total_t <= 0.0 { return -f64::INFINITY; }
    let sum: f64 = levels
        .iter()
        .zip(durations.iter())
        .map(|(&l, &t)| t * 10f64.powf(l / 10.0))
        .sum();
    10.0 * (sum / total_t).log10()
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use approx::assert_abs_diff_eq;

    fn model() -> PropagationModel {
        PropagationModel::new(PropagationConfig::default())
    }

    fn flat_lw(val: f64) -> [f64; 8] { [val; 8] }

    fn ground(hs: f64, hr: f64, d: f64, g: f64) -> GroundPath {
        GroundPath { source_height_m: hs, receiver_height_m: hr,
            distance_m: d, g_source: g, g_receiver: g, g_middle: g }
    }

    #[test]
    fn geometric_spreading_doubles_at_double_distance() {
        // A_div increases by 6 dB when distance doubles.
        // Place receivers at exactly 10 m and 20 m (same z as source to avoid diagonal).
        let m = model();
        let src = Point3::new(0.0, 0.0, 0.5);
        let r1  = Point3::new(10.0, 0.0, 0.5);
        let r2  = Point3::new(20.0, 0.0, 0.5);
        let b1 = m.compute(&src, &r1, &ground(0.5, 0.5, 10.0, 0.5), &[], None);
        let b2 = m.compute(&src, &r2, &ground(0.5, 0.5, 20.0, 0.5), &[], None);
        let diff = b2.a_div[4] - b1.a_div[4];
        assert_abs_diff_eq!(diff, 6.02, epsilon = 0.02);
    }

    #[test]
    fn atmospheric_absorption_increases_with_frequency() {
        let atm = AtmosphericConditions::default();
        let a500  = atm.alpha_db_per_m(500.0);
        let a4000 = atm.alpha_db_per_m(4000.0);
        assert!(a4000 > a500, "4kHz should absorb more than 500Hz");
    }

    #[test]
    fn higher_lw_gives_higher_lp() {
        let m = model();
        let src = Point3::new(0.0, 0.0, 0.5);
        let rcv = Point3::new(100.0, 0.0, 4.0);
        let lp_80 = m.lp_simple(&flat_lw(80.0), &src, &rcv, 0.5);
        let lp_90 = m.lp_simple(&flat_lw(90.0), &src, &rcv, 0.5);
        assert_abs_diff_eq!(lp_90 - lp_80, 10.0, epsilon = 0.5);
    }

    #[test]
    fn lp_decreases_with_distance() {
        let m = model();
        let src = Point3::new(0.0, 0.0, 0.5);
        let lp_50  = m.lp_simple(&flat_lw(100.0), &src, &Point3::new(50.0, 0.0, 4.0), 0.5);
        let lp_100 = m.lp_simple(&flat_lw(100.0), &src, &Point3::new(100.0, 0.0, 4.0), 0.5);
        assert!(lp_50 > lp_100, "SPL should decrease with distance");
    }

    #[test]
    fn barrier_reduces_spl() {
        let m = model();
        let src = Point3::new(0.0, 0.0, 0.5);
        let rcv = Point3::new(100.0, 0.0, 4.0);
        let g   = ground(0.5, 4.0, 100.0, 0.5);
        let lp_no_barrier = m.compute(&src, &rcv, &g, &[], None)
            .apply_to_lw(&flat_lw(100.0));
        let edge = DiffractionEdge { point: Point3::new(50.0, 0.0, 6.0), height_m: 6.0 };
        let lp_with_barrier = m.compute(&src, &rcv, &g, &[edge], None)
            .apply_to_lw(&flat_lw(100.0));
        assert!(lp_with_barrier < lp_no_barrier,
            "barrier should reduce SPL: {lp_no_barrier:.1} → {lp_with_barrier:.1}");
    }

    #[test]
    fn energy_sum_two_equal_levels_adds_3db() {
        let result = energy_sum(&[60.0, 60.0]);
        assert_abs_diff_eq!(result, 63.01, epsilon = 0.01);
    }

    #[test]
    fn leq_constant_level_returns_same() {
        let result = leq(&[65.0, 65.0], &[1.0, 1.0]);
        assert_abs_diff_eq!(result, 65.0, epsilon = 0.01);
    }

    #[test]
    fn leq_weighted_by_duration() {
        // 1h at 70 dB + 3h at 60 dB → Leq lower than 70 but higher than 60.
        let result = leq(&[70.0, 60.0], &[1.0, 3.0]);
        assert!(result > 60.0 && result < 70.0, "got {result}");
    }

    #[test]
    fn attenuation_breakdown_apply_to_lw() {
        let bd = AttenuationBreakdown {
            a_div:  [30.0; 8],
            a_atm:  [1.0; 8],
            a_gr:   [0.0; 8],
            a_bar:  [0.0; 8],
            a_misc: [0.0; 8],
            a_total: [31.0; 8],
        };
        let lw = [90.0; 8]; // 90 − 31 = 59 per band, then A-weighted sum
        let lp = bd.apply_to_lw(&lw);
        // Should be in a reasonable dB range.
        assert!(lp > 40.0 && lp < 80.0, "lp={lp}");
    }
}
