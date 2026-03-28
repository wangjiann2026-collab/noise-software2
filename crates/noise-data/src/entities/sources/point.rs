use nalgebra::Point3;
use serde::{Deserialize, Serialize};

/// A stationary point noise source.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PointSource {
    pub id: u64,
    pub name: String,
    pub position: Point3<f64>,
    /// Sound power level per octave band [63, 125, 250, 500, 1k, 2k, 4k, 8k] Hz (dBW).
    pub lw_db: [f64; 8],
    /// A-weighted total sound power level (dBA re 1 pW). Stored for quick reporting.
    pub lwa_db: f64,
    /// Directivity: None = omnidirectional.
    pub directivity_index_db: Option<[f64; 8]>,
}

impl PointSource {
    pub fn omnidirectional(id: u64, name: impl Into<String>, pos: Point3<f64>, lw_db: [f64; 8]) -> Self {
        // A-weighting corrections (dB) for octave bands 63–8000 Hz.
        const A_WEIGHTS: [f64; 8] = [-26.2, -16.1, -8.6, -3.2, 0.0, 1.2, 1.0, -1.1];
        let lwa = 10.0 * lw_db
            .iter()
            .zip(A_WEIGHTS.iter())
            .map(|(&lw, &a)| 10f64.powf((lw + a) / 10.0))
            .sum::<f64>()
            .log10();
        Self { id, name: name.into(), position: pos, lw_db, lwa_db: lwa, directivity_index_db: None }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lwa_computed_on_construction() {
        let src = PointSource::omnidirectional(1, "Fan", Point3::origin(), [80.0; 8]);
        // Flat 80 dB spectrum → LwA ≈ 87 dB (1–4 kHz bands dominate with +A-weights).
        assert!(src.lwa_db > 80.0, "Expected LwA > 80.0, got {}", src.lwa_db);
        assert!(src.lwa_db < 100.0);

        // Source dominated by low frequencies → LwA < Lw_low.
        let low_freq_src = PointSource::omnidirectional(2, "Hum", Point3::origin(), {
            let mut lw = [0.0f64; 8];
            lw[0] = 100.0; // 63 Hz: A-weight = -26.2 dB
            lw
        });
        assert!(low_freq_src.lwa_db < 100.0);
    }
}
