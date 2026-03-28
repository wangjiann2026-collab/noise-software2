//! CNOSSOS-EU road traffic noise emission model (EU Commission Directive 2015/996).
//!
//! Computes the A-weighted sound power level per metre of road (LW/m) for each
//! vehicle category and time period.
//!
//! # Model structure
//!   LW(f) = LW_rolling(f) + ΔLW_propulsion(f)
//!
//! where both terms depend on speed v and vehicle category.

use serde::{Deserialize, Serialize};

/// Vehicle category per CNOSSOS-EU Table A2.1.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum VehicleCategory {
    /// Cat 1: Passenger cars, light vehicles (≤3.5 t).
    Cat1,
    /// Cat 2: Medium heavy vehicles (3.5–16 t, 2 axles).
    Cat2,
    /// Cat 3: Heavy vehicles (>16 t or >2 axles).
    Cat3,
    /// Cat 4: Powered two-wheelers.
    Cat4,
    /// Cat 5: Open category.
    Cat5,
}

/// Road surface correction ΔLW_road (dB) per octave band [63–8k Hz].
/// Values from CNOSSOS-EU Table A2.4 (reference: dense asphalt 0 dB).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum RoadSurface {
    #[default] DenseAsphalt,
    PorousAsphalt1Layer,
    PorousAsphalt2Layer,
    OptimisedTexture,
    Concrete,
    Cobblestones,
}

impl RoadSurface {
    /// Surface correction ΔLW (dB) per octave band [63, 125, 250, 500, 1k, 2k, 4k, 8k] Hz.
    pub fn correction_db(self) -> [f64; 8] {
        match self {
            Self::DenseAsphalt          => [0.0; 8],
            Self::PorousAsphalt1Layer   => [0.0, 0.0, -1.0, -3.0, -5.0, -5.0, -3.0, -1.0],
            Self::PorousAsphalt2Layer   => [0.0, 0.0, -2.0, -4.0, -7.0, -7.0, -4.0, -2.0],
            Self::OptimisedTexture      => [0.0, 0.0, -1.0, -2.0, -3.0, -3.0, -2.0, -1.0],
            Self::Concrete              => [1.0, 1.0, 1.0, 1.5, 2.0, 2.5, 2.0, 1.0],
            Self::Cobblestones          => [3.0, 3.0, 3.0, 3.0, 3.0, 3.0, 3.0, 3.0],
        }
    }
}

/// Emission coefficients per vehicle category and octave band.
/// Source: CNOSSOS-EU Annex II Tables A2.1 – A2.3.
struct EmissionCoeffs {
    // Rolling noise: LW_rolling = AR + BR·log10(v/vref)
    ar: [f64; 8],
    br: [f64; 8],
    // Propulsion noise: LW_prop = AP + BP·(v − vref)/vref
    ap: [f64; 8],
    bp: [f64; 8],
}

const VREF: f64 = 70.0; // reference speed (km/h)

impl EmissionCoeffs {
    fn for_category(cat: VehicleCategory) -> Self {
        match cat {
            VehicleCategory::Cat1 => Self {
                ar: [79.7, 85.7, 84.5, 90.2, 97.3, 93.9, 84.1, 74.3],
                br: [30.0, 41.5, 38.9, 25.7, 32.5, 37.2, 39.0, 40.0],
                ap: [94.5, 89.2, 88.0, 85.9, 84.2, 86.9, 83.3, 76.1],
                bp: [-1.3,  7.2,  7.7,  8.0,  8.0,  8.0,  8.0,  8.0],
            },
            VehicleCategory::Cat2 => Self {
                ar: [84.0, 88.7, 91.5, 96.7, 97.4, 90.9, 83.8, 80.5],
                br: [30.0, 35.8, 32.6, 23.8, 30.1, 36.2, 38.3, 40.0],
                ap: [99.2, 97.8, 99.6, 99.7, 98.5, 97.6, 90.2, 83.6],
                bp: [-4.4,  4.8,  4.3,  4.3,  4.3,  4.3,  4.3,  4.3],
            },
            VehicleCategory::Cat3 => Self {
                ar: [87.0, 91.7, 94.1, 100.7, 100.8, 94.3, 87.1, 82.5],
                br: [30.0, 33.5, 31.3, 25.4, 31.8, 37.1, 38.6, 40.0],
                ap: [104.0, 100.1, 101.1, 101.0, 100.2, 99.4, 92.3, 86.8],
                bp: [-4.4,  4.8,  4.3,  4.3,  4.3,  4.3,  4.3,  4.3],
            },
            VehicleCategory::Cat4 => Self {
                ar: [84.0, 83.0, 83.0, 83.0, 86.0, 88.0, 87.0, 78.0],
                br: [41.0, 41.0, 41.0, 41.0, 41.0, 41.0, 41.0, 41.0],
                ap: [97.0, 94.0, 94.0, 92.0, 91.0, 92.0, 88.0, 76.0],
                bp: [0.0; 8],
            },
            VehicleCategory::Cat5 => Self {
                // Cat5 uses Cat1 coefficients as default.
                ar: [79.7, 85.7, 84.5, 90.2, 97.3, 93.9, 84.1, 74.3],
                br: [30.0, 41.5, 38.9, 25.7, 32.5, 37.2, 39.0, 40.0],
                ap: [94.5, 89.2, 88.0, 85.9, 84.2, 86.9, 83.3, 76.1],
                bp: [-1.3,  7.2,  7.7,  8.0,  8.0,  8.0,  8.0,  8.0],
            },
        }
    }
}

/// Per-vehicle-category emission result.
#[derive(Debug, Clone)]
pub struct VehicleEmission {
    pub category: VehicleCategory,
    pub lw_per_m_db: [f64; 8], // Sound power per metre of road, per octave band
    pub lwa_per_m_db: f64,     // A-weighted total (dB(A)/m)
}

/// A-weighting corrections (dB) for bands [63, 125, 250, 500, 1k, 2k, 4k, 8k] Hz.
const A_WEIGHTS: [f64; 8] = [-26.2, -16.1, -8.6, -3.2, 0.0, 1.2, 1.0, -1.1];

/// Compute emission for a single vehicle category.
///
/// # Parameters
/// - `cat`: vehicle category
/// - `speed_kmh`: mean speed (km/h), clamped to [20, 150]
/// - `flow_veh_per_h`: vehicle flow (vehicles/hour)
/// - `gradient_pct`: road gradient (%), positive = uphill
/// - `surface`: road surface type
pub fn vehicle_emission(
    cat: VehicleCategory,
    speed_kmh: f64,
    flow_veh_per_h: f64,
    gradient_pct: f64,
    surface: RoadSurface,
) -> VehicleEmission {
    let v = speed_kmh.clamp(20.0, 150.0);
    let c = EmissionCoeffs::for_category(cat);
    let surf_corr = surface.correction_db();

    let mut lw = [0.0f64; 8];
    for i in 0..8 {
        // Rolling component.
        let lw_r = c.ar[i] + c.br[i] * (v / VREF).log10();
        // Propulsion component.
        let lw_p = c.ap[i] + c.bp[i] * (v - VREF) / VREF;
        // Dominant: energy sum of rolling + propulsion.
        let lw_total = 10.0 * (10f64.powf(lw_r / 10.0) + 10f64.powf(lw_p / 10.0)).log10();
        // Gradient correction (simplified: +0.5 dB per 1% grade for Cat2/3).
        let grad_corr = match cat {
            VehicleCategory::Cat2 | VehicleCategory::Cat3 => gradient_pct.abs() * 0.5,
            _ => 0.0,
        };
        // Flow: add 10·log10(Q/1000) correction to convert per-vehicle to per-metre.
        // Eq: LW_road/m = LW_vehicle + 10·log10(Q/v) − 10·log10(1000)
        let q = flow_veh_per_h.max(1.0);
        let flow_corr = 10.0 * (q / v).log10();
        lw[i] = lw_total + grad_corr + flow_corr + surf_corr[i];
    }

    // A-weighted total.
    let lwa = 10.0 * lw.iter().zip(A_WEIGHTS.iter())
        .map(|(&l, &a)| 10f64.powf((l + a) / 10.0))
        .sum::<f64>()
        .log10();

    VehicleEmission { category: cat, lw_per_m_db: lw, lwa_per_m_db: lwa }
}

/// Total road emission combining all vehicle categories.
///
/// Returns combined LW/m per octave band (energy sum of all categories).
pub fn total_road_emission(
    flows: &[(VehicleCategory, f64, f64)], // (category, speed_kmh, flow_veh/h)
    gradient_pct: f64,
    surface: RoadSurface,
) -> [f64; 8] {
    let mut combined = [0.0f64; 8];
    for &(cat, speed, flow) in flows {
        let em = vehicle_emission(cat, speed, flow, gradient_pct, surface);
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
        let e50 = vehicle_emission(VehicleCategory::Cat1, 50.0, 1000.0, 0.0, RoadSurface::DenseAsphalt);
        let e90 = vehicle_emission(VehicleCategory::Cat1, 90.0, 1000.0, 0.0, RoadSurface::DenseAsphalt);
        assert!(e90.lwa_per_m_db > e50.lwa_per_m_db,
            "90 km/h ({:.1}) should be louder than 50 km/h ({:.1})",
            e90.lwa_per_m_db, e50.lwa_per_m_db);
    }

    #[test]
    fn higher_flow_increases_emission() {
        let e_low  = vehicle_emission(VehicleCategory::Cat1, 70.0, 100.0, 0.0, RoadSurface::DenseAsphalt);
        let e_high = vehicle_emission(VehicleCategory::Cat1, 70.0, 1000.0, 0.0, RoadSurface::DenseAsphalt);
        // 10× flow → +10 dB.
        assert_abs_diff_eq!(e_high.lwa_per_m_db - e_low.lwa_per_m_db, 10.0, epsilon = 1.0);
    }

    #[test]
    fn heavy_vehicles_louder_than_passenger() {
        let cat1 = vehicle_emission(VehicleCategory::Cat1, 80.0, 100.0, 0.0, RoadSurface::DenseAsphalt);
        let cat3 = vehicle_emission(VehicleCategory::Cat3, 80.0, 100.0, 0.0, RoadSurface::DenseAsphalt);
        assert!(cat3.lwa_per_m_db > cat1.lwa_per_m_db,
            "Cat3 ({:.1}) should be louder than Cat1 ({:.1})",
            cat3.lwa_per_m_db, cat1.lwa_per_m_db);
    }

    #[test]
    fn porous_asphalt_quieter_than_dense() {
        let dense  = vehicle_emission(VehicleCategory::Cat1, 80.0, 1000.0, 0.0, RoadSurface::DenseAsphalt);
        let porous = vehicle_emission(VehicleCategory::Cat1, 80.0, 1000.0, 0.0, RoadSurface::PorousAsphalt2Layer);
        assert!(porous.lwa_per_m_db < dense.lwa_per_m_db,
            "porous ({:.1}) should be quieter than dense ({:.1})",
            porous.lwa_per_m_db, dense.lwa_per_m_db);
    }

    #[test]
    fn gradient_increases_heavy_vehicle_emission() {
        let flat  = vehicle_emission(VehicleCategory::Cat3, 60.0, 500.0, 0.0, RoadSurface::DenseAsphalt);
        let steep = vehicle_emission(VehicleCategory::Cat3, 60.0, 500.0, 6.0, RoadSurface::DenseAsphalt);
        assert!(steep.lwa_per_m_db > flat.lwa_per_m_db);
    }

    #[test]
    fn total_emission_two_categories_louder_than_one() {
        let flows = vec![
            (VehicleCategory::Cat1, 70.0, 1000.0),
            (VehicleCategory::Cat3, 70.0, 50.0),
        ];
        let combined = total_road_emission(&flows, 0.0, RoadSurface::DenseAsphalt);
        let single = {
            let e = vehicle_emission(VehicleCategory::Cat1, 70.0, 1000.0, 0.0, RoadSurface::DenseAsphalt);
            e.lwa_per_m_db
        };
        let combined_a: f64 = 10.0 * combined.iter().zip([-26.2f64, -16.1, -8.6, -3.2, 0.0, 1.2, 1.0, -1.1].iter())
            .map(|(&l, &a)| 10f64.powf((l + a) / 10.0)).sum::<f64>().log10();
        assert!(combined_a > single);
    }

    #[test]
    fn emission_in_reasonable_range() {
        // Typical urban road: LW/m ≈ 80–100 dB(A)/m.
        let e = vehicle_emission(VehicleCategory::Cat1, 50.0, 1000.0, 0.0, RoadSurface::DenseAsphalt);
        assert!(e.lwa_per_m_db > 60.0 && e.lwa_per_m_db < 120.0,
            "unexpected value: {}", e.lwa_per_m_db);
    }
}
