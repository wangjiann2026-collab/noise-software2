//! Parallel grid noise level calculator.
//!
//! Drives the propagation model over every receiver point in a
//! `HorizontalGrid` (or `VerticalGrid` / `FacadeGrid`) using Rayon for
//! data-parallel execution.
//!
//! # Design
//! - Each receiver is independent → trivially parallel with `par_iter`.
//! - Progress is reported through an optional callback (thread-safe).
//! - Results are stored in `grid.results` as `Vec<f32>` (row-major).

use rayon::prelude::*;
use nalgebra::Point3;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use crate::engine::propagation::{PropagationConfig, PropagationModel};
use crate::engine::diffraction::DiffractionEdge;
use crate::engine::ground_effect::GroundPath;
use crate::grid::horizontal::HorizontalGrid;
use crate::spatial::SourceCuller;

/// A single noise source as seen by the calculator.
#[derive(Debug, Clone)]
pub struct SourceSpec {
    /// Unique source identifier.
    pub id: u64,
    /// 3D position of the source.
    pub position: Point3<f64>,
    /// Per-octave-band sound power (dB), 8 bands [63–8 kHz].
    pub lw_db: [f64; 8],
    /// Ground factor at the source (0 = hard, 1 = soft).
    pub g_source: f64,
}

/// Barriers that attenuate propagation paths.
#[derive(Debug, Clone)]
pub struct BarrierSpec {
    /// Diffracting edge geometry.
    pub edge: DiffractionEdge,
}

/// Configuration for the grid calculator.
#[derive(Debug, Clone)]
pub struct CalculatorConfig {
    /// Propagation model configuration (atmosphere, standard).
    pub propagation: PropagationConfig,
    /// Ground factor at receiver locations (0 = hard, 1 = soft).
    pub g_receiver: f64,
    /// Ground factor for the middle of the propagation path.
    pub g_middle: f64,
    /// Maximum horizontal distance (m) beyond which sources are skipped.
    ///
    /// `None` = no culling (all sources computed for every receiver).
    /// Setting this to e.g. 2 000 m greatly reduces computation for large grids
    /// with many sources by skipping geometrically impossible contributions.
    pub max_source_range_m: Option<f64>,
}

impl Default for CalculatorConfig {
    fn default() -> Self {
        Self {
            propagation: PropagationConfig::default(),
            g_receiver: 0.5,
            g_middle: 0.5,
            max_source_range_m: None,
        }
    }
}

/// Parallel grid noise level calculator.
pub struct GridCalculator {
    config: CalculatorConfig,
}

impl GridCalculator {
    pub fn new(config: CalculatorConfig) -> Self {
        Self { config }
    }

    /// Compute noise levels at all receivers in `grid`.
    ///
    /// `sources`  — list of noise sources
    /// `barriers` — list of barrier edges (applied to all paths)
    /// `progress` — optional callback `fn(completed, total)` called after each receiver
    ///
    /// Fills `grid.results` and returns the peak A-weighted SPL found.
    /// When [`CalculatorConfig::max_source_range_m`] is set, a spatial hash
    /// index is built once and used to skip sources beyond that distance.
    pub fn calculate(
        &self,
        grid: &mut HorizontalGrid,
        sources: &[SourceSpec],
        barriers: &[BarrierSpec],
        progress: Option<Arc<dyn Fn(usize, usize) + Send + Sync>>,
    ) -> f64 {
        let n = grid.point_count();
        let model = PropagationModel::new(self.config.propagation.clone());

        // Collect receiver points eagerly (needed for parallel indexing).
        let receivers: Vec<Point3<f64>> = grid.receiver_points().collect();
        let barrier_edges: Vec<DiffractionEdge> = barriers.iter().map(|b| b.edge.clone()).collect();

        // Build spatial index once for the whole grid when range culling is on.
        let culler = self.config.max_source_range_m.map(|range| {
            let positions: Vec<Point3<f64>> = sources.iter().map(|s| s.position).collect();
            SourceCuller::new(&positions, range)
        });

        let completed = Arc::new(AtomicUsize::new(0));

        // Parallel computation: each receiver → combined SPL from all sources.
        let results: Vec<f32> = receivers
            .par_iter()
            .map(|receiver| {
                let lp = match &culler {
                    Some(c) => {
                        let nearby = c.query(receiver);
                        let nearby_sources: Vec<&SourceSpec> =
                            nearby.iter().map(|&i| &sources[i]).collect();
                        self.receiver_lp_slice(&model, receiver, &nearby_sources, &barrier_edges)
                    }
                    None => self.receiver_lp(&model, receiver, sources, &barrier_edges),
                };
                let done = completed.fetch_add(1, Ordering::Relaxed) + 1;
                if let Some(cb) = &progress {
                    cb(done, n);
                }
                lp as f32
            })
            .collect();

        let peak = results.iter().cloned().fold(f32::NEG_INFINITY, f32::max);
        grid.results = results;
        peak as f64
    }

    /// Compute A-weighted SPL at a single receiver from all sources (full slice).
    fn receiver_lp(
        &self,
        model: &PropagationModel,
        receiver: &Point3<f64>,
        sources: &[SourceSpec],
        barrier_edges: &[DiffractionEdge],
    ) -> f64 {
        let mut total_linear = 0.0f64;
        for src in sources {
            let d = (receiver - src.position).norm().max(1.0);
            let ground = GroundPath {
                source_height_m:   src.position.z,
                receiver_height_m: receiver.z,
                distance_m: d,
                g_source:   src.g_source,
                g_receiver: self.config.g_receiver,
                g_middle:   self.config.g_middle,
            };
            let breakdown = model.compute(&src.position, receiver, &ground, barrier_edges, None);
            let lp = breakdown.apply_to_lw(&src.lw_db);
            if lp.is_finite() {
                total_linear += 10f64.powf(lp / 10.0);
            }
        }
        if total_linear <= 0.0 { -f64::INFINITY } else { 10.0 * total_linear.log10() }
    }

    /// Same as `receiver_lp` but takes a pre-filtered `&[&SourceSpec]` slice
    /// (used by the spatial culling path).
    fn receiver_lp_slice(
        &self,
        model: &PropagationModel,
        receiver: &Point3<f64>,
        sources: &[&SourceSpec],
        barrier_edges: &[DiffractionEdge],
    ) -> f64 {
        let mut total_linear = 0.0f64;
        for src in sources {
            let d = (receiver - src.position).norm().max(1.0);
            let ground = GroundPath {
                source_height_m:   src.position.z,
                receiver_height_m: receiver.z,
                distance_m: d,
                g_source:   src.g_source,
                g_receiver: self.config.g_receiver,
                g_middle:   self.config.g_middle,
            };
            let breakdown = model.compute(&src.position, receiver, &ground, barrier_edges, None);
            let lp = breakdown.apply_to_lw(&src.lw_db);
            if lp.is_finite() {
                total_linear += 10f64.powf(lp / 10.0);
            }
        }
        if total_linear <= 0.0 { -f64::INFINITY } else { 10.0 * total_linear.log10() }
    }

    /// Simple single-source calculation for a list of arbitrary receiver points.
    /// Returns one A-weighted SPL per receiver.
    pub fn calculate_points(
        &self,
        receivers: &[Point3<f64>],
        sources: &[SourceSpec],
        barriers: &[BarrierSpec],
    ) -> Vec<f64> {
        let model = PropagationModel::new(self.config.propagation.clone());
        let barrier_edges: Vec<DiffractionEdge> = barriers.iter().map(|b| b.edge.clone()).collect();

        let culler = self.config.max_source_range_m.map(|range| {
            let positions: Vec<Point3<f64>> = sources.iter().map(|s| s.position).collect();
            SourceCuller::new(&positions, range)
        });

        receivers.par_iter()
            .map(|rcv| match &culler {
                Some(c) => {
                    let nearby = c.query(rcv);
                    let nearby_sources: Vec<&SourceSpec> =
                        nearby.iter().map(|&i| &sources[i]).collect();
                    self.receiver_lp_slice(&model, rcv, &nearby_sources, &barrier_edges)
                }
                None => self.receiver_lp(&model, rcv, sources, &barrier_edges),
            })
            .collect()
    }
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use approx::assert_abs_diff_eq;

    fn source(x: f64, y: f64, z: f64, lw: f64) -> SourceSpec {
        SourceSpec {
            id: 1,
            position: Point3::new(x, y, z),
            lw_db: [lw; 8],
            g_source: 0.5,
        }
    }

    fn make_grid() -> HorizontalGrid {
        HorizontalGrid::new(1, "test", Point3::new(0.0, 0.0, 0.0), 10.0, 10.0, 5, 5, 4.0)
    }

    #[test]
    fn calculate_fills_results_vector() {
        let calc = GridCalculator::new(CalculatorConfig::default());
        let mut grid = make_grid();
        let src = source(25.0, 25.0, 0.5, 100.0);
        calc.calculate(&mut grid, &[src], &[], None);
        assert_eq!(grid.results.len(), grid.point_count());
    }

    #[test]
    fn results_are_finite_and_positive() {
        let calc = GridCalculator::new(CalculatorConfig::default());
        let mut grid = make_grid();
        let src = source(25.0, 25.0, 0.5, 100.0);
        calc.calculate(&mut grid, &[src], &[], None);
        for &v in &grid.results {
            assert!(v.is_finite() && v > 0.0, "expected positive finite level, got {v}");
        }
    }

    #[test]
    fn closer_receivers_are_louder() {
        let calc = GridCalculator::new(CalculatorConfig::default());
        let src = source(0.0, 0.0, 0.5, 100.0);
        let near = Point3::new(10.0, 0.0, 4.0);
        let far  = Point3::new(200.0, 0.0, 4.0);
        let levels = calc.calculate_points(&[near, far], &[src], &[]);
        assert!(levels[0] > levels[1],
            "near ({:.1}) should be louder than far ({:.1})", levels[0], levels[1]);
    }

    #[test]
    fn higher_lw_gives_higher_lp() {
        let calc = GridCalculator::new(CalculatorConfig::default());
        let rcv = Point3::new(100.0, 0.0, 4.0);
        let src80  = source(0.0, 0.0, 0.5, 80.0);
        let src100 = source(0.0, 0.0, 0.5, 100.0);
        let l80  = calc.calculate_points(&[rcv], &[src80], &[])[0];
        let l100 = calc.calculate_points(&[rcv], &[src100], &[])[0];
        assert_abs_diff_eq!(l100 - l80, 20.0, epsilon = 1.0);
    }

    #[test]
    fn two_equal_sources_louder_than_one() {
        let calc = GridCalculator::new(CalculatorConfig::default());
        let rcv = Point3::new(100.0, 0.0, 4.0);
        let src = source(0.0, 0.0, 0.5, 90.0);
        let l1 = calc.calculate_points(&[rcv], &[src.clone()], &[])[0];
        let l2 = calc.calculate_points(&[rcv], &[src.clone(), src], &[])[0];
        assert!(l2 > l1, "two sources ({l2:.1}) should be louder than one ({l1:.1})");
        assert_abs_diff_eq!(l2 - l1, 3.01, epsilon = 0.5);
    }

    #[test]
    fn barrier_reduces_levels() {
        let calc = GridCalculator::new(CalculatorConfig::default());
        let src = source(0.0, 0.0, 0.5, 100.0);
        let rcv = Point3::new(100.0, 0.0, 4.0);

        let barrier = BarrierSpec {
            edge: DiffractionEdge {
                point: Point3::new(50.0, 0.0, 6.0),
                height_m: 6.0,
            },
        };

        let l_no_barrier = calc.calculate_points(&[rcv], &[src.clone()], &[])[0];
        let l_barrier    = calc.calculate_points(&[rcv], &[src], &[barrier])[0];
        assert!(l_barrier < l_no_barrier,
            "barrier should reduce SPL: {l_no_barrier:.1} → {l_barrier:.1}");
    }

    #[test]
    fn no_sources_returns_neg_inf() {
        let calc = GridCalculator::new(CalculatorConfig::default());
        let rcv = Point3::new(100.0, 0.0, 4.0);
        let levels = calc.calculate_points(&[rcv], &[], &[]);
        assert!(levels[0].is_infinite() && levels[0] < 0.0);
    }

    #[test]
    fn spatial_culling_same_result_as_no_culling() {
        // With max_source_range large enough to include all sources, results
        // must match the no-culling path exactly.
        let src = source(0.0, 0.0, 0.5, 90.0);
        let rcv = Point3::new(100.0, 0.0, 4.0);

        let calc_no_cull = GridCalculator::new(CalculatorConfig::default());
        let l_no_cull = calc_no_cull.calculate_points(&[rcv], &[src.clone()], &[])[0];

        let config_cull = CalculatorConfig { max_source_range_m: Some(500.0), ..Default::default() };
        let calc_cull = GridCalculator::new(config_cull);
        let l_cull = calc_cull.calculate_points(&[rcv], &[src], &[])[0];

        assert!((l_no_cull - l_cull).abs() < 0.001,
            "culled ({l_cull:.3}) should match unculled ({l_no_cull:.3})");
    }

    #[test]
    fn spatial_culling_drops_distant_source() {
        // A source 1 000 m away with a 200 m range → treated as no source.
        let far_src = source(1000.0, 0.0, 0.5, 120.0);
        let rcv = Point3::new(0.0, 0.0, 4.0);

        let config = CalculatorConfig { max_source_range_m: Some(200.0), ..Default::default() };
        let calc = GridCalculator::new(config);
        let l = calc.calculate_points(&[rcv], &[far_src], &[])[0];
        assert!(l.is_infinite() && l < 0.0,
            "culled distant source should give -inf, got {l:.1}");
    }

    #[test]
    fn progress_callback_called_correct_times() {
        use std::sync::Mutex;
        let calc = GridCalculator::new(CalculatorConfig::default());
        let mut grid = make_grid();
        let src = source(25.0, 25.0, 0.5, 90.0);
        let n = grid.point_count();
        let count = Arc::new(Mutex::new(0usize));
        let count2 = count.clone();
        let cb: Arc<dyn Fn(usize, usize) + Send + Sync> = Arc::new(move |_done, _total| {
            *count2.lock().unwrap() += 1;
        });
        calc.calculate(&mut grid, &[src], &[], Some(cb));
        assert_eq!(*count.lock().unwrap(), n);
    }
}
