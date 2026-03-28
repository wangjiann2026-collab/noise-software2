//! Noise metric computations for WASM exposure.
//!
//! Wraps `noise_core::sources::superposition::combine_dba` and adds
//! the EU Lden / US Ldn formula.

/// Combine multiple A-weighted levels by incoherent energy summation.
///
/// Returns the total level in dBA.
///
/// # Arguments
/// * `levels` — slice of individual source levels (dBA).
///
/// # Example
/// ```
/// use noise_wasm::metrics::combine_levels;
/// let total = combine_levels(&[60.0, 60.0]);
/// assert!((total - 63.01).abs() < 0.1, "got {total:.2}");
/// ```
pub fn combine_levels(levels: &[f64]) -> f64 {
    noise_core::sources::combine_dba(levels)
}

/// Compute EU day-evening-night level (Lden) from Ld, Le, Ln.
///
/// Formula: Lden = 10·log10( (12·10^(Ld/10) + 4·10^((Le+5)/10) + 8·10^((Ln+10)/10)) / 24 )
///
/// # Example
/// ```
/// use noise_wasm::metrics::lden_from_ld_le_ln;
/// let lden = lden_from_ld_le_ln(62.0, 58.0, 52.0);
/// assert!(lden > 60.0 && lden < 70.0, "lden = {lden:.2}");
/// ```
pub fn lden_from_ld_le_ln(ld: f64, le: f64, ln: f64) -> f64 {
    let d = 12.0 * 10f64.powf(ld / 10.0);
    let e =  4.0 * 10f64.powf((le + 5.0) / 10.0);
    let n =  8.0 * 10f64.powf((ln + 10.0) / 10.0);
    10.0 * ((d + e + n) / 24.0).log10()
}

/// Compute US day-night level (Ldn) from Ld and Ln.
///
/// Formula: Ldn = 10·log10( (15·10^(Ld/10) + 9·10^((Ln+10)/10)) / 24 )
///
/// # Example
/// ```
/// use noise_wasm::metrics::ldn_from_ld_ln;
/// let ldn = ldn_from_ld_ln(62.0, 52.0);
/// assert!(ldn > 58.0 && ldn < 72.0, "ldn = {ldn:.2}");
/// ```
pub fn ldn_from_ld_ln(ld: f64, ln: f64) -> f64 {
    let d = 15.0 * 10f64.powf(ld / 10.0);
    let n =  9.0 * 10f64.powf((ln + 10.0) / 10.0);
    10.0 * ((d + n) / 24.0).log10()
}

// ─── wasm-bindgen exports ─────────────────────────────────────────────────────

#[cfg(feature = "wasm")]
mod wasm_exports {
    use super::*;
    use wasm_bindgen::prelude::*;

    /// Combine A-weighted levels (comma-separated string) by energy sum.
    ///
    /// Accepts e.g. `"60.0,65.0,58.0"` and returns the combined dBA value.
    #[wasm_bindgen(js_name = combineLevels)]
    pub fn wasm_combine_levels(levels_csv: &str) -> f64 {
        let levels: Vec<f64> = levels_csv
            .split(',')
            .filter_map(|s| s.trim().parse().ok())
            .collect();
        combine_levels(&levels)
    }

    /// EU Lden from day, evening, night levels.
    #[wasm_bindgen(js_name = ldenFromLdLeLn)]
    pub fn wasm_lden(ld: f64, le: f64, ln: f64) -> f64 {
        lden_from_ld_le_ln(ld, le, ln)
    }

    /// US Ldn from day and night levels.
    #[wasm_bindgen(js_name = ldnFromLdLn)]
    pub fn wasm_ldn(ld: f64, ln: f64) -> f64 {
        ldn_from_ld_ln(ld, ln)
    }
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn combine_two_equal_levels_adds_3db() {
        let total = combine_levels(&[60.0, 60.0]);
        assert!((total - 63.01).abs() < 0.1, "got {total:.2}");
    }

    #[test]
    fn combine_single_level_unchanged() {
        let total = combine_levels(&[55.0]);
        assert!((total - 55.0).abs() < 0.01);
    }

    #[test]
    fn combine_empty_slice_returns_neg_inf() {
        let total = combine_levels(&[]);
        assert!(total.is_nan() || total < 0.0,
            "expected no-data sentinel, got {total}");
    }

    #[test]
    fn lden_equal_periods_above_ln() {
        // With equal Ld=Le=Ln=60: evening+5 and night+10 penalties increase Lden.
        let lden = lden_from_ld_le_ln(60.0, 60.0, 60.0);
        assert!(lden > 60.0, "lden={lden:.2}");
    }

    #[test]
    fn lden_known_values() {
        // Ld=62, Le=58, Ln=52 → roughly 63.x dBA Lden
        let lden = lden_from_ld_le_ln(62.0, 58.0, 52.0);
        assert!(lden > 60.0 && lden < 68.0, "lden={lden:.2}");
    }

    #[test]
    fn ldn_known_values() {
        let ldn = ldn_from_ld_ln(62.0, 52.0);
        assert!(ldn > 58.0 && ldn < 72.0, "ldn={ldn:.2}");
    }

    #[test]
    fn ldn_night_penalty_raises_level() {
        // Same Ld, with night level raised by 10 dB should raise Ldn.
        let ldn1 = ldn_from_ld_ln(62.0, 42.0);
        let ldn2 = ldn_from_ld_ln(62.0, 52.0);
        assert!(ldn2 > ldn1, "ldn2={ldn2:.2} ldn1={ldn1:.2}");
    }
}
