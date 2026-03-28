//! Acoustic diffraction over barriers — Maekawa / ISO 9613-2 §7.4.
//!
//! # Model
//! For a thin rigid barrier the attenuation A_bar is computed from the
//! Fresnel number N:
//!
//!   δ = d_s + d_r − d_direct   (path length difference, m)
//!   N = 2δ/λ                    (Fresnel number)
//!   A_bar = 10·log₁₀(3 + 20·N)  for N > −0.19
//!
//! Multiple diffracting edges and ground reflections are handled by
//! superposing contributions.

use nalgebra::Point3;
use serde::{Deserialize, Serialize};

use super::ground_effect::OCTAVE_BANDS;

/// A single diffracting edge (top of barrier or building corner).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiffractionEdge {
    /// 3D position of the edge point closest to the direct path.
    pub point: Point3<f64>,
    /// Height of the edge above the ground (m). Used for ground correction.
    pub height_m: f64,
}

/// Input geometry for a barrier diffraction calculation.
#[derive(Debug, Clone)]
pub struct BarrierPath {
    pub source:   Point3<f64>,
    pub receiver: Point3<f64>,
    pub edge:     DiffractionEdge,
    /// Speed of sound (m/s).
    pub speed_of_sound: f64,
}

impl BarrierPath {
    /// Signed path length difference δ = ±(d_s + d_r − d_direct).
    ///
    /// Positive when the edge is above the direct source–receiver ray (shadow zone).
    /// Negative when the edge is below the ray (illuminated zone).
    pub fn path_length_diff(&self) -> f64 {
        let d_s = (self.edge.point - self.source).norm();
        let d_r = (self.receiver - self.edge.point).norm();
        let d_direct = (self.receiver - self.source).norm();
        let magnitude = d_s + d_r - d_direct;

        // Determine sign: project edge onto the source→receiver ray and compare
        // the edge height against the ray height at the same point.
        let ray_dir = self.receiver - self.source;
        let d_total = ray_dir.norm();
        if d_total < 1e-9 { return magnitude; }
        // Scalar projection of (edge - source) onto the ray direction.
        let t = (self.edge.point - self.source).dot(&ray_dir) / (d_total * d_total);
        let t_clamped = t.clamp(0.0, 1.0);
        // Interpolated point on direct ray.
        let ray_point = self.source + ray_dir * t_clamped;
        // Height of edge above the ray at that point.
        let height_above_ray = self.edge.point.z - ray_point.z;

        if height_above_ray >= 0.0 { magnitude } else { -magnitude }
    }

    /// Fresnel number N(f) = 2δ/λ = 2δ·f/c.
    pub fn fresnel_number(&self, f: f64) -> f64 {
        let delta = self.path_length_diff();
        2.0 * delta * f / self.speed_of_sound
    }

    /// Whether the barrier is in the shadow zone (δ > 0, edge above direct ray).
    pub fn is_in_shadow(&self) -> bool {
        self.path_length_diff() > 0.0
    }
}

/// Compute barrier insertion loss A_bar (dB) per octave band.
///
/// Returns `[0.0; 8]` if the path is not in the shadow zone (δ ≤ 0).
pub fn barrier_attenuation_db(path: &BarrierPath) -> [f64; 8] {
    let mut a_bar = [0.0f64; 8];
    let delta = path.path_length_diff();
    // If source–receiver direct path is not blocked, no barrier insertion loss.
    if delta <= 0.0 { return a_bar; }

    for (i, &f) in OCTAVE_BANDS.iter().enumerate() {
        a_bar[i] = maekawa_db(path.fresnel_number(f));
    }
    a_bar
}

/// Maekawa (1968) diffraction formula:
///   A_bar = 10·log₁₀(3 + 20·N)  (dB), valid for N > −0.19.
/// Clamped to [0, 30] dB per ISO 9613-2.
pub fn maekawa_db(n: f64) -> f64 {
    if n <= -0.19 { return 0.0; }
    let a = 10.0 * (3.0 + 20.0 * n).max(1.0).log10();
    a.clamp(0.0, 30.0)
}

/// ISO 9613-2 §7.4 correction K_met for meteorological conditions.
/// Returns additional attenuation (dB) to add to A_bar.
///
/// `c0`  = concave/convex correction factor (0.0 for no correction).
pub fn k_met_correction(delta: f64, d_s: f64, d_r: f64, c0: f64) -> f64 {
    if delta < 0.0 { return 0.0; }
    let k_met = c0 * (1.0 / d_s + 1.0 / d_r).sqrt();
    k_met * delta
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use approx::assert_abs_diff_eq;

    #[test]
    fn fresnel_zero_gives_minimum_attenuation() {
        // N=0 → 10·log10(3+0) ≈ 4.77 dB
        let a = maekawa_db(0.0);
        assert_abs_diff_eq!(a, 4.77, epsilon = 0.01);
    }

    #[test]
    fn maekawa_increases_with_n() {
        let a1 = maekawa_db(1.0);
        let a5 = maekawa_db(5.0);
        assert!(a5 > a1, "attenuation should increase with N");
    }

    #[test]
    fn maekawa_capped_at_30db() {
        // Very large N → capped at 30 dB.
        let a = maekawa_db(1e6);
        assert!((a - 30.0).abs() < 0.1, "expected 30 dB cap, got {a}");
    }

    #[test]
    fn negative_n_gives_zero() {
        assert_eq!(maekawa_db(-0.5), 0.0);
    }

    #[test]
    fn path_in_shadow_gives_positive_attenuation() {
        // Source at origin, receiver behind barrier.
        let path = BarrierPath {
            source:   Point3::new(0.0, 0.0, 0.5),
            receiver: Point3::new(100.0, 0.0, 4.0),
            edge: DiffractionEdge { point: Point3::new(50.0, 0.0, 5.0), height_m: 5.0 },
            speed_of_sound: 343.0,
        };
        assert!(path.is_in_shadow(), "receiver should be in shadow");
        let a = barrier_attenuation_db(&path);
        // All 8 bands should have positive attenuation.
        for (i, &v) in a.iter().enumerate() {
            assert!(v > 0.0, "band {i} attenuation should be positive, got {v}");
        }
    }

    #[test]
    fn path_not_in_shadow_gives_zero() {
        // Edge below the direct path → no shadow.
        let path = BarrierPath {
            source:   Point3::new(0.0, 0.0, 0.5),
            receiver: Point3::new(100.0, 0.0, 4.0),
            edge: DiffractionEdge { point: Point3::new(50.0, 0.0, 0.1), height_m: 0.1 },
            speed_of_sound: 343.0,
        };
        assert!(!path.is_in_shadow());
        let a = barrier_attenuation_db(&path);
        for &v in &a { assert_eq!(v, 0.0); }
    }

    #[test]
    fn higher_barrier_gives_more_attenuation() {
        let base = BarrierPath {
            source:   Point3::new(0.0, 0.0, 0.5),
            receiver: Point3::new(100.0, 0.0, 4.0),
            edge: DiffractionEdge { point: Point3::new(50.0, 0.0, 3.0), height_m: 3.0 },
            speed_of_sound: 343.0,
        };
        let high = BarrierPath {
            edge: DiffractionEdge { point: Point3::new(50.0, 0.0, 6.0), height_m: 6.0 },
            ..base.clone()
        };
        let a_base: f64 = barrier_attenuation_db(&base).iter().sum();
        let a_high: f64 = barrier_attenuation_db(&high).iter().sum();
        assert!(a_high > a_base, "higher barrier should give more attenuation");
    }

    #[test]
    fn higher_frequency_gives_more_attenuation_for_same_barrier() {
        let path = BarrierPath {
            source:   Point3::new(0.0, 0.0, 0.5),
            receiver: Point3::new(100.0, 0.0, 4.0),
            edge: DiffractionEdge { point: Point3::new(50.0, 0.0, 5.0), height_m: 5.0 },
            speed_of_sound: 343.0,
        };
        let a = barrier_attenuation_db(&path);
        // 4kHz (index 6) should have more attenuation than 63Hz (index 0).
        assert!(a[6] > a[0], "HF ({}) should exceed LF ({}) attenuation", a[6], a[0]);
    }
}
