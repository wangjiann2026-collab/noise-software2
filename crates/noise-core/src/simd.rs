//! SIMD-accelerated octave-band arithmetic.
//!
//! The 8 ISO 9613-2 octave bands [63, 125, 250, 500, 1k, 2k, 4k, 8k Hz] map
//! naturally to 8-wide SIMD registers: on x86-64 with AVX2 two 256-bit
//! registers hold all 8 `f64` values simultaneously.
//!
//! # Dispatch strategy
//! A runtime CPUID check (`is_x86_feature_detected!`) selects the AVX2 path
//! when available; otherwise the scalar fallback is used.  No nightly or
//! additional crates required.
//!
//! # Hot path
//! [`energy_sum_bands`] is the inner loop of `AttenuationBreakdown::apply_to_lw`.
//! The AVX2 path vectorises the band arithmetic `(Lw − A_total + A_weights) / 10`;
//! the subsequent `10^x` call remains scalar (no native AVX2 `exp`), but the
//! loads, FMAs and stores are vectorised.

use std::ops::{Add, AddAssign, Mul, Sub};

// ─── OctaveBands ─────────────────────────────────────────────────────────────

/// Ergonomic newtype for an 8-element octave-band array.
///
/// Implements arithmetic operators so band operations compose naturally:
/// ```
/// use noise_core::simd::OctaveBands;
/// let a = OctaveBands::splat(30.0);
/// let b = OctaveBands::splat(1.0);
/// let c = a - b; // [29.0; 8]
/// ```
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct OctaveBands(pub [f64; 8]);

impl OctaveBands {
    #[inline] pub fn new(v: [f64; 8]) -> Self { Self(v) }
    #[inline] pub fn splat(v: f64) -> Self { Self([v; 8]) }
    #[inline] pub fn as_array(&self) -> &[f64; 8] { &self.0 }
    #[inline] pub fn into_array(self) -> [f64; 8] { self.0 }

    /// Component-wise maximum.
    #[inline]
    pub fn max_bands(self, other: OctaveBands) -> OctaveBands {
        OctaveBands(std::array::from_fn(|i| self.0[i].max(other.0[i])))
    }

    /// Component-wise minimum.
    #[inline]
    pub fn min_bands(self, other: OctaveBands) -> OctaveBands {
        OctaveBands(std::array::from_fn(|i| self.0[i].min(other.0[i])))
    }

    /// Scalar multiply of each band.
    #[inline]
    pub fn scale(self, s: f64) -> OctaveBands {
        OctaveBands(self.0.map(|v| v * s))
    }

    /// Sum of all 8 bands.
    #[inline]
    pub fn horizontal_sum(self) -> f64 {
        self.0.iter().sum()
    }
}

impl Add for OctaveBands {
    type Output = Self;
    #[inline]
    fn add(self, rhs: Self) -> Self {
        Self(std::array::from_fn(|i| self.0[i] + rhs.0[i]))
    }
}

impl Sub for OctaveBands {
    type Output = Self;
    #[inline]
    fn sub(self, rhs: Self) -> Self {
        Self(std::array::from_fn(|i| self.0[i] - rhs.0[i]))
    }
}

impl Mul<f64> for OctaveBands {
    type Output = Self;
    #[inline]
    fn mul(self, rhs: f64) -> Self {
        Self(self.0.map(|v| v * rhs))
    }
}

impl AddAssign for OctaveBands {
    #[inline]
    fn add_assign(&mut self, rhs: Self) {
        for i in 0..8 {
            self.0[i] += rhs.0[i];
        }
    }
}

impl From<[f64; 8]> for OctaveBands {
    fn from(v: [f64; 8]) -> Self { Self(v) }
}

impl From<OctaveBands> for [f64; 8] {
    fn from(b: OctaveBands) -> Self { b.0 }
}

// ─── Energy summation ────────────────────────────────────────────────────────

/// Compute the A-weighted energy sum across all octave bands.
///
/// Equivalent to `Σ 10^((lw[i] − a_total[i] + a_weights[i]) / 10)`.
///
/// This is the hot inner loop of `AttenuationBreakdown::apply_to_lw`.
/// On x86-64 with AVX2 the band arithmetic is vectorised; `10^x` uses
/// the scalar `powf` per element (no hardware exp available in AVX2).
///
/// # Arguments
/// * `lw`       – per-band sound power (dB)
/// * `a_total`  – per-band total attenuation (dB)
/// * `a_weights`– A-weighting corrections (dB)
#[inline]
pub fn energy_sum_bands(lw: &[f64; 8], a_total: &[f64; 8], a_weights: &[f64; 8]) -> f64 {
    #[cfg(target_arch = "x86_64")]
    {
        if is_x86_feature_detected!("avx2") {
            // SAFETY: feature check above guarantees AVX2 is available.
            return unsafe { energy_sum_bands_avx2(lw, a_total, a_weights) };
        }
    }
    energy_sum_bands_scalar(lw, a_total, a_weights)
}

#[inline(always)]
fn energy_sum_bands_scalar(lw: &[f64; 8], a_total: &[f64; 8], a_weights: &[f64; 8]) -> f64 {
    let mut sum = 0.0f64;
    for i in 0..8 {
        sum += 10f64.powf((lw[i] - a_total[i] + a_weights[i]) * 0.1);
    }
    sum
}

/// AVX2 path: vectorise `(Lw − A + Aw) × 0.1`; scalar for `10^x`.
#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2")]
unsafe fn energy_sum_bands_avx2(lw: &[f64; 8], a_total: &[f64; 8], a_weights: &[f64; 8]) -> f64 {
    use std::arch::x86_64::*;

    let scale = _mm256_set1_pd(0.1);

    // Low 4 bands.
    let lw_lo   = _mm256_loadu_pd(lw.as_ptr());
    let a_lo    = _mm256_loadu_pd(a_total.as_ptr());
    let aw_lo   = _mm256_loadu_pd(a_weights.as_ptr());
    // (Lw + Aw - A) * 0.1
    let diff_lo = _mm256_mul_pd(_mm256_sub_pd(_mm256_add_pd(lw_lo, aw_lo), a_lo), scale);

    // High 4 bands.
    let lw_hi   = _mm256_loadu_pd(lw.as_ptr().add(4));
    let a_hi    = _mm256_loadu_pd(a_total.as_ptr().add(4));
    let aw_hi   = _mm256_loadu_pd(a_weights.as_ptr().add(4));
    let diff_hi = _mm256_mul_pd(_mm256_sub_pd(_mm256_add_pd(lw_hi, aw_hi), a_hi), scale);

    // Store and apply 10^x (scalar, no hardware exp).
    let mut lo = [0.0f64; 4];
    let mut hi = [0.0f64; 4];
    _mm256_storeu_pd(lo.as_mut_ptr(), diff_lo);
    _mm256_storeu_pd(hi.as_mut_ptr(), diff_hi);

    let mut sum = 0.0f64;
    for &v in &lo { sum += 10f64.powf(v); }
    for &v in &hi { sum += 10f64.powf(v); }
    sum
}

/// Returns `true` if AVX2 is available at runtime on this CPU.
pub fn avx2_available() -> bool {
    #[cfg(target_arch = "x86_64")]
    { is_x86_feature_detected!("avx2") }
    #[cfg(not(target_arch = "x86_64"))]
    { false }
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use approx::assert_abs_diff_eq;

    const A_WEIGHTS: [f64; 8] = [-26.2, -16.1, -8.6, -3.2, 0.0, 1.2, 1.0, -1.1];

    #[test]
    fn octave_bands_add() {
        let a = OctaveBands::splat(10.0);
        let b = OctaveBands::splat(5.0);
        assert_eq!((a + b).0, [15.0; 8]);
    }

    #[test]
    fn octave_bands_sub() {
        let a = OctaveBands::splat(10.0);
        let b = OctaveBands::splat(3.0);
        assert_eq!((a - b).0, [7.0; 8]);
    }

    #[test]
    fn octave_bands_scale() {
        let a = OctaveBands::splat(20.0);
        assert_eq!(a.scale(0.5).0, [10.0; 8]);
    }

    #[test]
    fn octave_bands_max_min() {
        let a = OctaveBands::new([1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0]);
        let b = OctaveBands::new([8.0, 7.0, 6.0, 5.0, 4.0, 3.0, 2.0, 1.0]);
        let max = a.max_bands(b);
        let min = a.min_bands(b);
        for v in max.0 { assert!(v >= 4.0 && v <= 8.0); }
        for v in min.0 { assert!(v >= 1.0 && v <= 5.0); }
    }

    #[test]
    fn octave_bands_add_assign() {
        let mut a = OctaveBands::splat(1.0);
        a += OctaveBands::splat(2.0);
        assert_eq!(a.0, [3.0; 8]);
    }

    #[test]
    fn energy_sum_scalar_matches_manual() {
        let lw     = [90.0f64; 8];
        let a_tot  = [30.0f64; 8];
        // Manual: 8 × 10^((90-30+aw[i])/10)
        let expected: f64 = A_WEIGHTS.iter()
            .map(|&aw| 10f64.powf((90.0 - 30.0 + aw) * 0.1))
            .sum();
        let got = energy_sum_bands(&lw, &a_tot, &A_WEIGHTS);
        assert_abs_diff_eq!(got, expected, epsilon = 1e-9);
    }

    #[test]
    fn energy_sum_zero_attenuation() {
        let lw    = [60.0f64; 8];
        let a_tot = [0.0f64; 8];
        let aw    = [0.0f64; 8];
        // Each band: 10^(60/10) = 10^6; 8 bands → 8e6
        let got = energy_sum_bands(&lw, &a_tot, &aw);
        assert_abs_diff_eq!(got, 8.0e6, epsilon = 1.0);
    }

    #[test]
    fn energy_sum_avx2_matches_scalar_when_available() {
        // If AVX2 is present, both paths should agree.
        if !avx2_available() { return; }
        let lw    = [80.0, 82.0, 84.0, 86.0, 88.0, 90.0, 88.0, 84.0];
        let a_tot = [25.0, 26.0, 27.0, 28.0, 29.0, 30.0, 31.0, 32.0];
        let scalar = energy_sum_bands_scalar(&lw, &a_tot, &A_WEIGHTS);
        let simd   = unsafe { energy_sum_bands_avx2(&lw, &a_tot, &A_WEIGHTS) };
        assert_abs_diff_eq!(scalar, simd, epsilon = 1e-9);
    }

    #[test]
    fn from_into_roundtrip() {
        let arr = [1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0];
        let bands = OctaveBands::from(arr);
        let back: [f64; 8] = bands.into();
        assert_eq!(arr, back);
    }

    #[test]
    fn horizontal_sum() {
        let a = OctaveBands::splat(2.5);
        assert_abs_diff_eq!(a.horizontal_sum(), 20.0, epsilon = 1e-10);
    }
}
