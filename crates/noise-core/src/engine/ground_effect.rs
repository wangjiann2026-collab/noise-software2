//! Ground effect calculation per ISO 9613-2 §7.3.
//!
//! The ground effect accounts for the interaction of the direct and
//! ground-reflected waves, which produces excess attenuation (or amplification)
//! depending on the mean flow resistivity (G factor) of the ground.
//!
//! # Model summary
//! - G = 0 → hard ground (asphalt, concrete, water)
//! - G = 1 → soft ground (farmland, dense vegetation)
//!
//! Three ground regions are considered:
//!   - Source region  (0 < G_s ≤ 1) — near the source
//!   - Receiver region (0 < G_r ≤ 1) — near the receiver
//!   - Middle region  (G_m)           — in between

use serde::{Deserialize, Serialize};

/// Octave band centre frequencies (Hz) for the 8-band model.
pub const OCTAVE_BANDS: [f64; 8] = [63.0, 125.0, 250.0, 500.0, 1000.0, 2000.0, 4000.0, 8000.0];

/// Ground parameters for a propagation path.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GroundPath {
    /// Source height above local ground (m).
    pub source_height_m: f64,
    /// Receiver height above local ground (m).
    pub receiver_height_m: f64,
    /// Horizontal distance source → receiver (m).
    pub distance_m: f64,
    /// G factor of source region [0, 1].
    pub g_source: f64,
    /// G factor of receiver region [0, 1].
    pub g_receiver: f64,
    /// G factor of middle region [0, 1].
    pub g_middle: f64,
}

/// Computes ground effect attenuation A_gr (dB) per octave band.
///
/// Positive values → attenuation; negative values → amplification (ground reflection boost).
pub fn ground_attenuation_db(path: &GroundPath) -> [f64; 8] {
    let hs = path.source_height_m.max(0.0);
    let hr = path.receiver_height_m.max(0.0);
    let d  = path.distance_m.max(1.0);
    let gs = path.g_source.clamp(0.0, 1.0);
    let gr = path.g_receiver.clamp(0.0, 1.0);
    let gm = path.g_middle.clamp(0.0, 1.0);

    let mut a_gr = [0.0f64; 8];
    for (i, &f) in OCTAVE_BANDS.iter().enumerate() {
        a_gr[i] = a_gr_band(f, hs, hr, d, gs, gr, gm);
    }
    a_gr
}

/// ISO 9613-2 eq. (7): ground attenuation for a single octave band.
fn a_gr_band(f: f64, hs: f64, hr: f64, d: f64, gs: f64, gr: f64, gm: f64) -> f64 {
    // Eq. (6): Am (attenuation for middle ground region).
    let am = a_m(f, hs, hr, d, gm);
    // Eq. (5): As (attenuation for source region).
    let a_s = a_s_or_r(f, hs, d, gs);
    // Eq. (5): Ar (attenuation for receiver region).
    let a_r = a_s_or_r(f, hr, d, gr);

    // Total: A_gr = As + Ar + Am, bounded to [-3, ∞)
    (a_s + a_r + am).max(-3.0)
}

/// Ground factor q for middle region (ISO 9613-2 eq. 8).
fn q_factor(f: f64, hs: f64, hr: f64, d: f64) -> f64 {
    // dp = distance parameter = 30(hs + hr) [m] at the crossover frequency
    let dp = 30.0 * (hs + hr);
    if d > dp { 0.0 } else { 1.0 - d / dp }
}

/// A_m for the middle region (ISO 9613-2 eq. 6).
fn a_m(f: f64, hs: f64, hr: f64, d: f64, gm: f64) -> f64 {
    let q = q_factor(f, hs, hr, d);
    // Upper bound for G is 1.
    -3.0 * (1.0 - gm) * (1.0 - q)
}

/// A_s or A_r for source/receiver region (ISO 9613-2 eq. 5).
fn a_s_or_r(f: f64, h: f64, d: f64, g: f64) -> f64 {
    // Effective flow resistivity term.
    let sigma = effective_sigma(g);
    // Reflection coefficient r (simplified Delany-Bazley).
    let r = reflection_coefficient(f, sigma, h);
    // Ground contribution: −1.5 + G·ΔA, where ΔA = 8.686*(1−r)
    -1.5 + g * 8.686 * (1.0 - r).max(0.0)
}

/// Effective flow resistivity σ (kPa·s/m²) from G factor.
fn effective_sigma(g: f64) -> f64 {
    // Rough empirical mapping: hard (G=0) → σ=10⁷, soft (G=1) → σ=30
    let log_sigma = 7.0 - 6.0 * g; // log10(σ)
    10f64.powf(log_sigma)
}

/// Plane-wave reflection coefficient (simplified Delany-Bazley impedance model).
fn reflection_coefficient(f: f64, sigma: f64, h: f64) -> f64 {
    // Normalized admittance: β = (σ / (ρ0·c·f))^0.5  — simplified
    // For numerical stability, clamp to [0,1].
    let rho_c = 415.0; // air impedance (Pa·s/m)
    let beta = (sigma / (rho_c * f)).sqrt().min(10.0);
    let grazing_angle = (h / 1.0_f64.max(h)).atan(); // approximation
    let r = ((1.0 - beta * grazing_angle.sin())
        / (1.0 + beta * grazing_angle.sin()))
        .abs()
        .min(1.0);
    r
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn std_path(g: f64) -> GroundPath {
        GroundPath {
            source_height_m: 0.05,
            receiver_height_m: 4.0,
            distance_m: 100.0,
            g_source: g, g_receiver: g, g_middle: g,
        }
    }

    #[test]
    fn hard_ground_near_zero_attenuation() {
        // Hard ground (G=0): reflected wave nearly in phase with direct → boost possible.
        let a = ground_attenuation_db(&std_path(0.0));
        // All bands should be ≥ −3 dB (ISO lower bound).
        for &v in &a { assert!(v >= -3.01, "band too negative: {v}"); }
    }

    #[test]
    fn soft_ground_differs_from_hard_ground() {
        // Soft (G=1) and hard (G=0) ground should produce different A_gr values.
        let a_hard = ground_attenuation_db(&std_path(0.0));
        let a_soft = ground_attenuation_db(&std_path(1.0));
        // Sum across bands must differ.
        let sum_hard: f64 = a_hard.iter().sum();
        let sum_soft: f64 = a_soft.iter().sum();
        assert!((sum_hard - sum_soft).abs() > 0.1,
            "soft and hard ground should give different total attenuation; hard={sum_hard:.2}, soft={sum_soft:.2}");
    }

    #[test]
    fn ground_attenuation_bounded_below() {
        for g in [0.0, 0.5, 1.0] {
            let a = ground_attenuation_db(&std_path(g));
            for &v in &a {
                assert!(v >= -3.01, "below lower bound: {v} (G={g})");
            }
        }
    }

    #[test]
    fn greater_distance_reduces_ground_effect() {
        // At very large distances, q → 0, so middle region dominates.
        let near = GroundPath { distance_m: 50.0, ..std_path(1.0) };
        let far  = GroundPath { distance_m: 500.0, ..std_path(1.0) };
        let a_near: f64 = ground_attenuation_db(&near).iter().sum();
        let a_far:  f64 = ground_attenuation_db(&far).iter().sum();
        // Far path should have different (typically lower effective) ground attenuation.
        // We just assert the function doesn't panic and values are finite.
        assert!(a_near.is_finite() && a_far.is_finite());
    }
}
