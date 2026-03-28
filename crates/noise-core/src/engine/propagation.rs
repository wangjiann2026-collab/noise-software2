//! Acoustic propagation models.
//!
//! Supports ISO 9613-2 (general outdoor sound propagation) and
//! CNOSSOS-EU (Common Noise Assessment Methods in Europe).

use serde::{Deserialize, Serialize};

/// Available propagation model standards.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum ModelStandard {
    /// ISO 9613-2: General method for outdoor sound propagation.
    #[default]
    Iso9613_2,
    /// CNOSSOS-EU: EU harmonized method for road and railway noise.
    CnossosEu,
}

/// Atmospheric conditions for propagation calculation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AtmosphericConditions {
    /// Temperature (°C).
    pub temperature_c: f64,
    /// Relative humidity (%).
    pub humidity_pct: f64,
    /// Atmospheric pressure (Pa).
    pub pressure_pa: f64,
}

impl Default for AtmosphericConditions {
    fn default() -> Self {
        Self { temperature_c: 20.0, humidity_pct: 70.0, pressure_pa: 101_325.0 }
    }
}

impl AtmosphericConditions {
    /// Atmospheric absorption coefficient α (dB/km) per frequency band.
    /// Based on ISO 9613-1 formula.
    pub fn absorption_db_per_km(&self, frequency_hz: f64) -> f64 {
        // Simplified ISO 9613-1 approximation.
        let t = self.temperature_c + 273.15;
        let h = self.humidity_pct;
        let f = frequency_hz;

        // Oxygen relaxation frequency.
        let f_ro = 24.0 + 4.04e4 * h * (0.02 + h) / (0.391 + h);
        // Nitrogen relaxation frequency.
        let f_rn = t.powf(-0.5) * (9.0 + 280.0 * h * (-4.17 * ((t / 293.15).powf(-1.0 / 3.0) - 1.0)).exp());

        let alpha = 8.686 * f * f
            * (1.84e-11 * (t / 293.15).powf(0.5)
                + t.powf(-5.0 / 2.0)
                    * (0.01275 * (-2239.1 / t).exp() / (f_ro + f * f / f_ro)
                        + 0.1068 * (-3352.0 / t).exp() / (f_rn + f * f / f_rn)));

        alpha * 1000.0 // convert from dB/m to dB/km
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PropagationConfig {
    pub standard: ModelStandard,
    pub atmosphere: AtmosphericConditions,
    pub frequency_bands: Vec<f64>,
}

impl Default for PropagationConfig {
    fn default() -> Self {
        Self {
            standard: ModelStandard::default(),
            atmosphere: AtmosphericConditions::default(),
            frequency_bands: vec![63.0, 125.0, 250.0, 500.0, 1000.0, 2000.0, 4000.0, 8000.0],
        }
    }
}

/// Propagation model computing excess attenuation terms.
pub struct PropagationModel {
    config: PropagationConfig,
}

impl PropagationModel {
    pub fn new(config: PropagationConfig) -> Self {
        Self { config }
    }

    /// Total insertion loss / excess attenuation for a path of `distance_m`.
    /// Returns per-band attenuation in dB (positive = more attenuation).
    pub fn attenuation_db(&self, distance_m: f64) -> Vec<f64> {
        self.config
            .frequency_bands
            .iter()
            .map(|&f| {
                let adiv = 20.0 * distance_m.max(1.0).log10() + 11.0;
                let aatm = self.config.atmosphere.absorption_db_per_km(f) * distance_m / 1000.0;
                adiv + aatm
            })
            .collect()
    }

    pub fn config(&self) -> &PropagationConfig {
        &self.config
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn attenuation_increases_with_distance() {
        let model = PropagationModel::new(PropagationConfig::default());
        let a10 = model.attenuation_db(10.0);
        let a100 = model.attenuation_db(100.0);
        for (near, far) in a10.iter().zip(a100.iter()) {
            assert!(far > near, "far={far} should exceed near={near}");
        }
    }

    #[test]
    fn high_frequency_has_more_atmospheric_absorption() {
        let atm = AtmosphericConditions::default();
        let a_low = atm.absorption_db_per_km(500.0);
        let a_high = atm.absorption_db_per_km(4000.0);
        assert!(a_high > a_low, "4kHz ({a_high}) should absorb more than 500Hz ({a_low})");
    }
}
