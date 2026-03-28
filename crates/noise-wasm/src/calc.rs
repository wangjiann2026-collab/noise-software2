//! Single-point SPL calculation binding.
//!
//! Wraps `noise_core` propagation to compute the A-weighted sound pressure
//! level at a receiver point from a single point source.

use noise_core::engine::{
    PropagationConfig, PropagationModel,
    AttenuationBreakdown,
};
use noise_core::engine::ground_effect::GroundPath;
use nalgebra::Point3;
use serde::{Deserialize, Serialize};

/// Input parameters for a single-point SPL calculation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SplInput {
    /// Source X coordinate (m).
    pub sx: f64,
    /// Source Y coordinate (m).
    pub sy: f64,
    /// Source Z coordinate (m, height above ground).
    pub sz: f64,
    /// Source sound power level per octave band (8 bands, 63–8k Hz), dB re 1 pW.
    pub lw_db: [f64; 8],
    /// Ground absorption at source (0 = hard, 1 = soft).
    pub g_source: f64,
    /// Receiver X coordinate (m).
    pub rx: f64,
    /// Receiver Y coordinate (m).
    pub ry: f64,
    /// Receiver Z coordinate (m, height above ground).
    pub rz: f64,
    /// Ground absorption at receiver (0 = hard, 1 = soft).
    pub g_receiver: f64,
    /// Ground absorption in the middle zone (0 = hard, 1 = soft).
    pub g_middle: f64,
}

impl Default for SplInput {
    fn default() -> Self {
        Self {
            sx: 0.0, sy: 0.0, sz: 0.5,
            lw_db: [80.0; 8],
            g_source: 0.0,
            rx: 0.0, ry: 50.0, rz: 4.0,
            g_receiver: 0.0,
            g_middle: 0.5,
        }
    }
}

/// Compute the A-weighted SPL (dBA) at a receiver from a single source.
///
/// Uses ISO 9613-2 propagation: geometric spreading + atmospheric absorption
/// + ground effect (no barriers).
///
/// # Example
/// ```
/// use noise_wasm::calc::{SplInput, calculate_spl};
/// let input = SplInput {
///     sx: 0.0, sy: 0.0, sz: 0.5,
///     lw_db: [80.0; 8],
///     g_source: 0.0,
///     rx: 0.0, ry: 50.0, rz: 4.0,
///     g_receiver: 0.0,
///     g_middle: 0.5,
/// };
/// let spl = calculate_spl(&input);
/// assert!(spl > 30.0 && spl < 70.0, "spl = {spl:.1}");
/// ```
pub fn calculate_spl(input: &SplInput) -> f64 {
    let model = PropagationModel::new(PropagationConfig::default());
    let source   = Point3::new(input.sx, input.sy, input.sz);
    let receiver = Point3::new(input.rx, input.ry, input.rz);
    let d = (receiver - source).norm().max(1.0);
    let ground = GroundPath {
        source_height_m:   input.sz,
        receiver_height_m: input.rz,
        distance_m: d,
        g_source:   input.g_source,
        g_receiver: input.g_receiver,
        g_middle:   input.g_middle,
    };
    let bd = model.compute(&source, &receiver, &ground, &[], None);
    bd.apply_to_lw(&input.lw_db)
}

/// Compute SPL from flat coordinate arguments (convenience form for JS callers).
///
/// # Arguments
/// `sx,sy,sz` — source position; `lw_db` — flat A-weighted source power (dBA);
/// `rx,ry,rz` — receiver position.
pub fn calculate_spl_simple(
    sx: f64, sy: f64, sz: f64,
    lw_db: f64,
    rx: f64, ry: f64, rz: f64,
) -> f64 {
    let lw_bands = [lw_db; 8];
    let input = SplInput {
        sx, sy, sz,
        lw_db: lw_bands,
        g_source: 0.0,
        rx, ry, rz,
        g_receiver: 0.0,
        g_middle: 0.5,
    };
    calculate_spl(&input)
}

/// Atmospheric absorption coefficient (dB/m) for a given octave-band
/// centre frequency using `noise_core`'s ISO 9613-1 implementation.
pub fn iso9613_atmospheric(freq_hz: f64, temp_c: f64, humidity_pct: f64) -> f64 {
    use noise_core::engine::PropagationConfig;
    use noise_core::engine::propagation::AtmosphericConditions;
    let atm = AtmosphericConditions {
        temperature_c: temp_c,
        humidity_pct,
        pressure_pa: 101_325.0,
    };
    atm.alpha_db_per_m(freq_hz)
}

// ─── wasm-bindgen exports ─────────────────────────────────────────────────────

#[cfg(feature = "wasm")]
mod wasm_exports {
    use super::*;
    use wasm_bindgen::prelude::*;

    /// Compute SPL (dBA) at a receiver from a single source.
    ///
    /// All coordinates in metres; `lw_db` is flat A-weighted source power.
    #[wasm_bindgen(js_name = calculateSpl)]
    pub fn wasm_calculate_spl(
        sx: f64, sy: f64, sz: f64,
        lw_db: f64,
        rx: f64, ry: f64, rz: f64,
    ) -> f64 {
        calculate_spl_simple(sx, sy, sz, lw_db, rx, ry, rz)
    }

    /// Atmospheric absorption (dB/m) for a given frequency.
    #[wasm_bindgen(js_name = iso9613Atmospheric)]
    pub fn wasm_iso9613_atmospheric(freq_hz: f64, temp_c: f64, humidity_pct: f64) -> f64 {
        iso9613_atmospheric(freq_hz, temp_c, humidity_pct)
    }
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spl_decreases_with_distance() {
        let near = calculate_spl_simple(0.0, 0.0, 0.5, 80.0, 0.0, 10.0, 4.0);
        let far  = calculate_spl_simple(0.0, 0.0, 0.5, 80.0, 0.0, 100.0, 4.0);
        assert!(near > far, "SPL near={near:.1} should exceed far={far:.1}");
    }

    #[test]
    fn spl_doubles_distance_drops_6db() {
        let d1 = calculate_spl_simple(0.0, 0.0, 0.5, 80.0, 0.0, 20.0, 0.5);
        let d2 = calculate_spl_simple(0.0, 0.0, 0.5, 80.0, 0.0, 40.0, 0.5);
        // Should be ~6 dB per doubling of distance (point source).
        assert!((d1 - d2 - 6.0).abs() < 3.0, "Δ = {:.1}, expected ~6", d1 - d2);
    }

    #[test]
    fn spl_struct_and_simple_agree() {
        let input = SplInput {
            sx: 0.0, sy: 0.0, sz: 0.5,
            lw_db: [75.0; 8],
            g_source: 0.0,
            rx: 0.0, ry: 50.0, rz: 4.0,
            g_receiver: 0.0,
            g_middle: 0.5,
        };
        let via_struct = calculate_spl(&input);
        let via_simple = calculate_spl_simple(0.0, 0.0, 0.5, 75.0, 0.0, 50.0, 4.0);
        assert!((via_struct - via_simple).abs() < 0.1,
            "struct={via_struct:.2} simple={via_simple:.2}");
    }

    #[test]
    fn spl_in_reasonable_range() {
        let spl = calculate_spl(&SplInput::default());
        assert!(spl > 20.0 && spl < 80.0, "spl = {spl:.1}");
    }

    #[test]
    fn atmospheric_absorption_positive() {
        let a = iso9613_atmospheric(1000.0, 20.0, 70.0);
        assert!(a >= 0.0);
    }

    #[test]
    fn atmospheric_absorption_increases_with_frequency() {
        let a_low  = iso9613_atmospheric(125.0,  20.0, 70.0);
        let a_high = iso9613_atmospheric(4000.0, 20.0, 70.0);
        assert!(a_high > a_low,
            "high-freq {a_high:.4} should exceed low-freq {a_low:.4}");
    }
}
