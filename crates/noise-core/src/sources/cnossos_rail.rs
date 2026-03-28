//! CNOSSOS-EU railway noise emission model (EU Commission Directive 2015/996).
//!
//! Computes the A-weighted sound power level per metre of track (LW'/m) for each
//! vehicle/track combination and time period.
//!
//! # Model structure
//!   LW'(f) = LW'_rolling(f) + LW'_traction(f) + ΔLW'_aerodynamic(f)
//!
//! Rolling noise is the dominant source at intermediate speeds (60–200 km/h).
//! Traction (engine/gear) noise dominates at low speeds (< ~50 km/h).
//! Aerodynamic noise dominates at high speeds (> ~200 km/h).

use serde::{Deserialize, Serialize};

/// Vehicle type per CNOSSOS-EU Table B2.1.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TrainType {
    /// Passenger train (intercity, regional).
    Passenger,
    /// High-speed train (> 200 km/h design speed).
    HighSpeed,
    /// Freight train.
    Freight,
    /// Urban metro / light rail / tram.
    Metro,
    /// Diesel multiple unit.
    DieselUnit,
}

/// Rail roughness condition — affects rolling noise.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum RailRoughness {
    #[default]
    /// Well-maintained smooth rail (reference: 0 dB correction).
    Smooth,
    /// Corrugated rail (+5 dB low-frequency emphasis).
    Corrugated,
    /// Rough rail (+3 dB broadband).
    Rough,
}

impl RailRoughness {
    /// Roughness correction ΔLW (dB) per octave band [63–8k Hz].
    pub fn correction_db(self) -> [f64; 8] {
        match self {
            Self::Smooth      => [0.0; 8],
            Self::Rough       => [3.0; 8],
            Self::Corrugated  => [5.0, 5.0, 4.0, 3.0, 2.0, 1.0, 0.0, 0.0],
        }
    }
}

/// Track support condition — affects rolling noise radiation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum TrackType {
    #[default]
    /// Ballasted track (reference).
    Ballasted,
    /// Slab track (typically 2–4 dB quieter than ballasted).
    Slab,
    /// Embedded rail (e.g., in-pavement tram track).
    Embedded,
    /// Track on bridge deck (+3 dB amplification).
    Bridge,
}

impl TrackType {
    /// Track correction ΔLW (dB) per octave band.
    pub fn correction_db(self) -> [f64; 8] {
        match self {
            Self::Ballasted => [0.0; 8],
            Self::Slab      => [-2.0, -2.0, -3.0, -4.0, -4.0, -3.0, -2.0, -1.0],
            Self::Embedded  => [-1.0, -1.0, -2.0, -3.0, -3.0, -2.0, -1.0,  0.0],
            Self::Bridge    => [3.0, 3.0, 3.0, 3.0, 3.0, 3.0, 2.0, 1.0],
        }
    }
}

/// Base emission coefficients per train type and octave band.
/// Source: CNOSSOS-EU Annex III Table B2.1.
struct RailEmissionCoeffs {
    // Rolling: LW'_r = AR + BR·log10(v/vref) [dB re 1 pW/m]
    ar: [f64; 8],
    br: [f64; 8],
    // Traction: LW'_t = AT − BT·(v − vref)/vref
    at: [f64; 8],
    bt: [f64; 8],
    // Aerodynamic: LW'_a = AA + BA·log10(v/va_ref), only above va_threshold
    aa: [f64; 8],
    ba: [f64; 8],
    /// Speed threshold (km/h) below which aerodynamic term is suppressed.
    va_threshold: f64,
}

const VREF_RAIL: f64 = 100.0; // reference speed (km/h)
const VA_REF: f64 = 250.0;    // aerodynamic reference speed (km/h)

impl RailEmissionCoeffs {
    fn for_train_type(tt: TrainType) -> Self {
        match tt {
            TrainType::Passenger => Self {
                ar: [98.0, 98.0, 101.0, 103.0, 105.0, 104.0, 99.0, 93.0],
                br: [20.0, 20.0, 20.0, 22.0, 24.0, 24.0, 24.0, 24.0],
                at: [88.0, 88.0, 88.0, 85.0, 82.0, 79.0, 76.0, 73.0],
                bt: [0.0; 8],
                aa: [98.0, 99.0, 102.0, 104.0, 106.0, 107.0, 106.0, 102.0],
                ba: [45.0; 8],
                va_threshold: 200.0,
            },
            TrainType::HighSpeed => Self {
                ar: [96.0, 97.0, 99.0, 101.0, 103.0, 104.0, 100.0, 95.0],
                br: [22.0, 22.0, 22.0, 22.0, 24.0, 24.0, 26.0, 26.0],
                at: [85.0, 85.0, 85.0, 82.0, 78.0, 74.0, 70.0, 66.0],
                bt: [0.0; 8],
                aa: [100.0, 101.0, 104.0, 106.0, 108.0, 109.0, 108.0, 104.0],
                ba: [50.0; 8],
                va_threshold: 180.0,
            },
            TrainType::Freight => Self {
                ar: [102.0, 103.0, 105.0, 108.0, 109.0, 106.0, 100.0, 94.0],
                br: [18.0, 18.0, 18.0, 20.0, 22.0, 22.0, 22.0, 22.0],
                at: [92.0, 92.0, 92.0, 90.0, 87.0, 84.0, 81.0, 78.0],
                bt: [2.0; 8],
                aa: [97.0, 98.0, 101.0, 103.0, 105.0, 106.0, 105.0, 101.0],
                ba: [40.0; 8],
                va_threshold: 220.0,
            },
            TrainType::Metro => Self {
                ar: [95.0, 96.0, 98.0, 101.0, 103.0, 102.0, 97.0, 91.0],
                br: [20.0, 20.0, 20.0, 22.0, 24.0, 24.0, 24.0, 24.0],
                at: [90.0, 90.0, 90.0, 88.0, 85.0, 82.0, 79.0, 76.0],
                bt: [1.0; 8],
                aa: [93.0, 94.0, 97.0, 99.0, 101.0, 102.0, 101.0, 97.0],
                ba: [42.0; 8],
                va_threshold: 220.0,
            },
            TrainType::DieselUnit => Self {
                ar: [97.0, 98.0, 100.0, 103.0, 105.0, 104.0, 99.0, 93.0],
                br: [20.0, 20.0, 20.0, 22.0, 24.0, 24.0, 24.0, 24.0],
                at: [94.0, 93.0, 92.0, 91.0, 90.0, 88.0, 85.0, 81.0],
                bt: [3.0, 3.0, 2.0, 2.0, 2.0, 2.0, 2.0, 2.0],
                aa: [95.0, 96.0, 99.0, 101.0, 103.0, 104.0, 103.0, 99.0],
                ba: [40.0; 8],
                va_threshold: 240.0,
            },
        }
    }
}

/// A-weighting corrections (dB) for octave bands [63, 125, 250, 500, 1k, 2k, 4k, 8k] Hz.
const A_WEIGHTS: [f64; 8] = [-26.2, -16.1, -8.6, -3.2, 0.0, 1.2, 1.0, -1.1];

/// Per-train emission result.
#[derive(Debug, Clone)]
pub struct TrainEmission {
    pub train_type: TrainType,
    /// Sound power per metre of track per octave band (dB re 1 pW/m).
    pub lw_per_m_db: [f64; 8],
    /// A-weighted total LW'/m (dB(A)/m).
    pub lwa_per_m_db: f64,
}

/// Compute emission for a single train type.
///
/// # Parameters
/// - `train_type`: vehicle type
/// - `speed_kmh`: train speed (km/h), clamped to [10, 350]
/// - `flow_trains_per_h`: number of trains per hour
/// - `roughness`: rail roughness condition
/// - `track`: track support type
pub fn train_emission(
    train_type: TrainType,
    speed_kmh: f64,
    flow_trains_per_h: f64,
    roughness: RailRoughness,
    track: TrackType,
) -> TrainEmission {
    let v = speed_kmh.clamp(10.0, 350.0);
    let c = RailEmissionCoeffs::for_train_type(train_type);
    let roughness_corr = roughness.correction_db();
    let track_corr = track.correction_db();

    let mut lw = [0.0f64; 8];
    for i in 0..8 {
        // Rolling noise component.
        let lw_r = c.ar[i] + c.br[i] * (v / VREF_RAIL).log10() + roughness_corr[i] + track_corr[i];

        // Traction noise (dominant at low speed).
        let lw_t = c.at[i] - c.bt[i] * (v - VREF_RAIL) / VREF_RAIL;

        // Aerodynamic noise (only above threshold speed).
        let lw_a = if v > c.va_threshold {
            c.aa[i] + c.ba[i] * (v / VA_REF).log10()
        } else {
            -f64::INFINITY
        };

        // Energy summation of all three components.
        let mut total_linear = 10f64.powf(lw_r / 10.0) + 10f64.powf(lw_t / 10.0);
        if lw_a.is_finite() {
            total_linear += 10f64.powf(lw_a / 10.0);
        }

        // Convert flow (trains/h) to per-metre emission.
        // LW'_road/m = LW'_vehicle + 10·log10(N/v) where N = trains/h, v = speed km/h
        let q = flow_trains_per_h.max(0.1);
        let flow_corr = 10.0 * (q / v).log10();

        lw[i] = 10.0 * total_linear.log10() + flow_corr;
    }

    // A-weighted total.
    let lwa = 10.0 * lw.iter().zip(A_WEIGHTS.iter())
        .map(|(&l, &a)| 10f64.powf((l + a) / 10.0))
        .sum::<f64>()
        .log10();

    TrainEmission { train_type, lw_per_m_db: lw, lwa_per_m_db: lwa }
}

/// Total track emission combining all train types.
///
/// Returns combined LW'/m per octave band (energy sum of all train flows).
pub fn total_track_emission(
    flows: &[(TrainType, f64, f64)], // (train_type, speed_kmh, flow_trains_per_h)
    roughness: RailRoughness,
    track: TrackType,
) -> [f64; 8] {
    let mut combined = [0.0f64; 8];
    for &(tt, speed, flow) in flows {
        let em = train_emission(tt, speed, flow, roughness, track);
        for i in 0..8 {
            combined[i] += 10f64.powf(em.lw_per_m_db[i] / 10.0);
        }
    }
    combined.map(|v| if v > 0.0 { 10.0 * v.log10() } else { -f64::INFINITY })
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use approx::assert_abs_diff_eq;

    #[test]
    fn higher_speed_increases_emission() {
        let e80  = train_emission(TrainType::Passenger, 80.0,  10.0, RailRoughness::Smooth, TrackType::Ballasted);
        let e160 = train_emission(TrainType::Passenger, 160.0, 10.0, RailRoughness::Smooth, TrackType::Ballasted);
        assert!(e160.lwa_per_m_db > e80.lwa_per_m_db,
            "160 km/h ({:.1}) should be louder than 80 km/h ({:.1})",
            e160.lwa_per_m_db, e80.lwa_per_m_db);
    }

    #[test]
    fn higher_flow_increases_emission() {
        let e_low  = train_emission(TrainType::Passenger, 100.0, 1.0,  RailRoughness::Smooth, TrackType::Ballasted);
        let e_high = train_emission(TrainType::Passenger, 100.0, 10.0, RailRoughness::Smooth, TrackType::Ballasted);
        // 10× flow → +10 dB.
        assert_abs_diff_eq!(e_high.lwa_per_m_db - e_low.lwa_per_m_db, 10.0, epsilon = 1.0);
    }

    #[test]
    fn freight_louder_than_passenger() {
        let passenger = train_emission(TrainType::Passenger, 100.0, 10.0, RailRoughness::Smooth, TrackType::Ballasted);
        let freight   = train_emission(TrainType::Freight,   100.0, 10.0, RailRoughness::Smooth, TrackType::Ballasted);
        assert!(freight.lwa_per_m_db > passenger.lwa_per_m_db,
            "Freight ({:.1}) should be louder than Passenger ({:.1})",
            freight.lwa_per_m_db, passenger.lwa_per_m_db);
    }

    #[test]
    fn slab_track_quieter_than_ballasted() {
        let ballasted = train_emission(TrainType::Passenger, 100.0, 10.0, RailRoughness::Smooth, TrackType::Ballasted);
        let slab      = train_emission(TrainType::Passenger, 100.0, 10.0, RailRoughness::Smooth, TrackType::Slab);
        assert!(slab.lwa_per_m_db < ballasted.lwa_per_m_db,
            "Slab ({:.1}) should be quieter than Ballasted ({:.1})",
            slab.lwa_per_m_db, ballasted.lwa_per_m_db);
    }

    #[test]
    fn corrugated_rail_louder_than_smooth() {
        let smooth    = train_emission(TrainType::Passenger, 100.0, 10.0, RailRoughness::Smooth,     TrackType::Ballasted);
        let corrugated = train_emission(TrainType::Passenger, 100.0, 10.0, RailRoughness::Corrugated, TrackType::Ballasted);
        assert!(corrugated.lwa_per_m_db > smooth.lwa_per_m_db,
            "Corrugated ({:.1}) should be louder than Smooth ({:.1})",
            corrugated.lwa_per_m_db, smooth.lwa_per_m_db);
    }

    #[test]
    fn bridge_track_louder_than_ballasted() {
        let ballasted = train_emission(TrainType::Passenger, 100.0, 10.0, RailRoughness::Smooth, TrackType::Ballasted);
        let bridge    = train_emission(TrainType::Passenger, 100.0, 10.0, RailRoughness::Smooth, TrackType::Bridge);
        assert!(bridge.lwa_per_m_db > ballasted.lwa_per_m_db,
            "Bridge ({:.1}) should be louder than Ballasted ({:.1})",
            bridge.lwa_per_m_db, ballasted.lwa_per_m_db);
    }

    #[test]
    fn aerodynamic_noise_activates_at_high_speed() {
        // High speed above threshold activates aerodynamic term.
        let e_slow = train_emission(TrainType::HighSpeed, 150.0, 1.0, RailRoughness::Smooth, TrackType::Slab);
        let e_fast = train_emission(TrainType::HighSpeed, 300.0, 1.0, RailRoughness::Smooth, TrackType::Slab);
        // Fast train should be considerably louder.
        assert!(e_fast.lwa_per_m_db > e_slow.lwa_per_m_db + 5.0,
            "300 km/h HST should be much louder; got {:.1} vs {:.1}",
            e_fast.lwa_per_m_db, e_slow.lwa_per_m_db);
    }

    #[test]
    fn total_emission_two_types_louder_than_one() {
        let flows = vec![
            (TrainType::Passenger, 120.0, 10.0),
            (TrainType::Freight,    80.0,  2.0),
        ];
        let combined = total_track_emission(&flows, RailRoughness::Smooth, TrackType::Ballasted);
        let single_em = train_emission(TrainType::Passenger, 120.0, 10.0, RailRoughness::Smooth, TrackType::Ballasted);
        let single_a = single_em.lwa_per_m_db;
        let combined_a: f64 = 10.0 * combined.iter().zip(A_WEIGHTS.iter())
            .map(|(&l, &a)| 10f64.powf((l + a) / 10.0)).sum::<f64>().log10();
        assert!(combined_a > single_a,
            "Combined ({:.1}) should exceed single ({:.1})", combined_a, single_a);
    }

    #[test]
    fn emission_in_reasonable_range() {
        let e = train_emission(TrainType::Passenger, 120.0, 5.0, RailRoughness::Smooth, TrackType::Ballasted);
        assert!(e.lwa_per_m_db > 60.0 && e.lwa_per_m_db < 120.0,
            "unexpected value: {}", e.lwa_per_m_db);
    }
}
