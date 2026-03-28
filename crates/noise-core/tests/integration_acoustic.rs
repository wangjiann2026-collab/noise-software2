//! Integration tests for the full acoustic pipeline:
//! CNOSSOS-EU emission → ISO 9613-2 propagation → grid superposition.

use nalgebra::Point3;
use noise_core::{
    engine::{
        propagation::{PropagationConfig, PropagationModel},
        ground_effect::GroundPath,
        diffraction::DiffractionEdge,
    },
    sources::{
        cnossos_road::{vehicle_emission, total_road_emission, VehicleCategory, RoadSurface},
        cnossos_rail::{train_emission, TrainType, RailRoughness, TrackType},
        superposition::{combine_dba, combine_bands, ReceiverResult},
    },
    grid::{
        calculator::{GridCalculator, CalculatorConfig, SourceSpec, BarrierSpec},
        horizontal::HorizontalGrid,
    },
};
use approx::assert_abs_diff_eq;

// ─── Road → Propagation ───────────────────────────────────────────────────────

#[test]
fn road_emission_then_propagation_gives_reasonable_lp() {
    // Compute emission for a mixed urban road.
    let flows = vec![
        (VehicleCategory::Cat1, 50.0, 800.0),
        (VehicleCategory::Cat3, 50.0,  20.0),
    ];
    let lw_per_m = total_road_emission(&flows, 0.0, RoadSurface::DenseAsphalt);

    // Propagate to a receiver 50 m away at 4 m height.
    let model = PropagationModel::new(PropagationConfig::default());
    let source   = Point3::new(0.0, 0.0, 0.5);
    let receiver = Point3::new(50.0, 0.0, 4.0);
    let lp = model.lp_simple(&lw_per_m, &source, &receiver, 0.5);

    // Urban road at 50 m: typically 50–75 dB(A).
    assert!(lp > 40.0 && lp < 85.0, "unexpected Lp = {lp:.1} dB(A)");
}

#[test]
fn porous_asphalt_reduces_noise_at_receiver() {
    let flows = vec![(VehicleCategory::Cat1, 80.0, 1000.0)];
    let lw_dense  = total_road_emission(&flows, 0.0, RoadSurface::DenseAsphalt);
    let lw_porous = total_road_emission(&flows, 0.0, RoadSurface::PorousAsphalt2Layer);

    let model    = PropagationModel::new(PropagationConfig::default());
    let source   = Point3::new(0.0, 0.0, 0.5);
    let receiver = Point3::new(50.0, 0.0, 4.0);
    let lp_dense  = model.lp_simple(&lw_dense,  &source, &receiver, 0.5);
    let lp_porous = model.lp_simple(&lw_porous, &source, &receiver, 0.5);

    assert!(lp_porous < lp_dense, "porous asphalt ({lp_porous:.1}) should be quieter ({lp_dense:.1})");
}

// ─── Railway → Propagation ───────────────────────────────────────────────────

#[test]
fn railway_emission_then_propagation_gives_reasonable_lp() {
    let em = train_emission(TrainType::Passenger, 120.0, 6.0, RailRoughness::Smooth, TrackType::Ballasted);
    let model    = PropagationModel::new(PropagationConfig::default());
    let source   = Point3::new(0.0, 0.0, 0.5);
    let receiver = Point3::new(100.0, 0.0, 4.0);
    let lp = model.lp_simple(&em.lw_per_m_db, &source, &receiver, 0.5);
    assert!(lp > 30.0 && lp < 90.0, "unexpected Lp = {lp:.1} dB(A)");
}

#[test]
fn slab_track_quieter_at_receiver() {
    let flows = vec![(TrainType::Passenger, 120.0, 6.0)];
    let lw_ballasted = noise_core::sources::cnossos_rail::total_track_emission(&flows, RailRoughness::Smooth, TrackType::Ballasted);
    let lw_slab      = noise_core::sources::cnossos_rail::total_track_emission(&flows, RailRoughness::Smooth, TrackType::Slab);
    let model    = PropagationModel::new(PropagationConfig::default());
    let source   = Point3::new(0.0, 0.0, 0.5);
    let receiver = Point3::new(50.0, 0.0, 4.0);
    let lp_b = model.lp_simple(&lw_ballasted, &source, &receiver, 0.5);
    let lp_s = model.lp_simple(&lw_slab,      &source, &receiver, 0.5);
    assert!(lp_s < lp_b, "slab ({lp_s:.1}) should be quieter than ballasted ({lp_b:.1})");
}

// ─── Multi-source superposition ───────────────────────────────────────────────

#[test]
fn two_equal_sources_gives_plus_3db() {
    let total = combine_dba(&[65.0, 65.0]);
    assert_abs_diff_eq!(total, 68.01, epsilon = 0.02);
}

#[test]
fn receiver_result_aggregates_correctly() {
    let bands = [70.0f64; 8];
    let contribs = vec![
        (1u64, 30.0, bands),
        (2u64, 40.0, bands),
    ];
    let result = ReceiverResult::from_band_contributions(42, contribs);
    assert_eq!(result.receiver_index, 42);
    assert_eq!(result.contributions.len(), 2);
    for i in 0..8 {
        assert_abs_diff_eq!(result.lp_total_bands_db[i], 73.01, epsilon = 0.02);
    }
}

// ─── Grid calculator ─────────────────────────────────────────────────────────

#[test]
fn grid_calculator_full_pipeline() {
    let calc = GridCalculator::new(CalculatorConfig::default());
    let mut grid = HorizontalGrid::new(
        1, "integration", Point3::new(0.0, 0.0, 0.0), 10.0, 10.0, 4, 4, 4.0,
    );
    let src = SourceSpec {
        id: 1,
        position: Point3::new(20.0, 20.0, 0.5),
        lw_db: [95.0; 8],
        g_source: 0.5,
    };
    let peak = calc.calculate(&mut grid, &[src], &[], None);
    assert_eq!(grid.results.len(), 16);
    assert!(peak > 0.0 && peak < 120.0, "peak = {peak}");
    // All levels should be finite.
    for &v in &grid.results {
        assert!(v.is_finite(), "got {v}");
    }
}

#[test]
fn barrier_reduces_grid_levels() {
    let calc = GridCalculator::new(CalculatorConfig::default());

    let src = SourceSpec {
        id: 1,
        position: Point3::new(0.0, 0.0, 0.5),
        lw_db: [100.0; 8],
        g_source: 0.5,
    };
    // Receivers well behind the barrier.
    let receivers: Vec<Point3<f64>> = (1..=5)
        .map(|i| Point3::new(100.0, i as f64 * 5.0, 4.0))
        .collect();
    let barrier = BarrierSpec {
        edge: DiffractionEdge { point: Point3::new(50.0, 0.0, 6.0), height_m: 6.0 },
    };

    let no_barrier = calc.calculate_points(&receivers, &[src.clone()], &[]);
    let with_barrier = calc.calculate_points(&receivers, &[src], &[barrier]);

    // Receivers directly in shadow should have lower levels.
    let first_no  = no_barrier[0];
    let first_bar = with_barrier[0];
    assert!(first_bar < first_no,
        "barrier should reduce: {first_no:.1} → {first_bar:.1}");
}

// ─── Barrier + Road scenario ──────────────────────────────────────────────────

#[test]
fn road_with_barrier_scenario() {
    // Compute road LW/m.
    let flows = vec![
        (VehicleCategory::Cat1, 70.0, 1200.0),
        (VehicleCategory::Cat2, 70.0,   80.0),
    ];
    let lw_per_m = total_road_emission(&flows, 0.0, RoadSurface::DenseAsphalt);

    let model    = PropagationModel::new(PropagationConfig::default());
    let source   = Point3::new(0.0, 0.0, 0.5);
    let receiver = Point3::new(80.0, 0.0, 4.0);
    let d = 80.0_f64;
    let ground   = GroundPath {
        source_height_m:   0.5,
        receiver_height_m: 4.0,
        distance_m: d,
        g_source:   0.0, // road surface = hard
        g_receiver: 0.5,
        g_middle:   0.5,
    };
    let barrier = DiffractionEdge { point: Point3::new(40.0, 0.0, 5.0), height_m: 5.0 };

    let breakdown_no  = model.compute(&source, &receiver, &ground, &[], None);
    let breakdown_bar = model.compute(&source, &receiver, &ground, &[barrier], None);

    let lp_no  = breakdown_no.apply_to_lw(&lw_per_m);
    let lp_bar = breakdown_bar.apply_to_lw(&lw_per_m);

    assert!(lp_bar < lp_no,
        "barrier should reduce SPL: {lp_no:.1} → {lp_bar:.1}");
    // Insertion loss should be at least 3 dB for a 5 m barrier at 40 m.
    assert!(lp_no - lp_bar > 3.0, "IL = {:.1}", lp_no - lp_bar);
}
