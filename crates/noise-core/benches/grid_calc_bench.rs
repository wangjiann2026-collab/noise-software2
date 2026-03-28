//! Criterion benchmarks for the parallel grid calculator.
//!
//! Measures throughput (receivers/sec) at several grid sizes with and without
//! spatial source-range culling.
//!
//! Run with:
//! ```text
//! cargo bench -p noise-core --bench grid_calc_bench
//! ```

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};
use nalgebra::Point3;
use noise_core::grid::calculator::{BarrierSpec, CalculatorConfig, GridCalculator, SourceSpec};
use noise_core::grid::horizontal::HorizontalGrid;

fn make_source(x: f64, y: f64, lw: f64) -> SourceSpec {
    SourceSpec {
        id: 1,
        position: Point3::new(x, y, 0.5),
        lw_db: [lw; 8],
        g_source: 0.5,
    }
}

fn make_grid(n: usize) -> HorizontalGrid {
    // n × n grid, 10 m spacing, 4 m receiver height.
    let side = n as u64;
    HorizontalGrid::new(1, "bench", Point3::new(0.0, 0.0, 0.0), 10.0, 10.0, side, side, 4.0)
}

// ─── Single source, varying grid size ─────────────────────────────────────────

fn bench_grid_single_source(c: &mut Criterion) {
    let calc = GridCalculator::new(CalculatorConfig::default());
    let src = make_source(500.0, 500.0, 100.0);

    let mut group = c.benchmark_group("grid_single_source");
    for n in [5usize, 10, 20] {
        let n_pts = n * n;
        group.bench_with_input(
            BenchmarkId::new("points", n_pts),
            &n,
            |b, &size| {
                b.iter_batched(
                    || make_grid(size),
                    |mut grid| {
                        black_box(calc.calculate(&mut grid, &[src.clone()], &[], None))
                    },
                    criterion::BatchSize::SmallInput,
                )
            },
        );
    }
    group.finish();
}

// ─── Multiple sources ─────────────────────────────────────────────────────────

fn bench_grid_multi_source(c: &mut Criterion) {
    let calc = GridCalculator::new(CalculatorConfig::default());
    let rcv = Point3::new(200.0, 200.0, 4.0);

    let mut group = c.benchmark_group("multi_source_calculate_points");
    for n_src in [1usize, 5, 20] {
        let sources: Vec<SourceSpec> = (0..n_src)
            .map(|i| make_source(i as f64 * 50.0, 0.0, 95.0))
            .collect();

        group.bench_with_input(
            BenchmarkId::new("sources", n_src),
            &n_src,
            |b, _| {
                b.iter(|| {
                    calc.calculate_points(
                        black_box(&[rcv]),
                        black_box(&sources),
                        black_box(&[]),
                    )
                })
            },
        );
    }
    group.finish();
}

// ─── Spatial culling vs no-culling ────────────────────────────────────────────

fn bench_spatial_culling(c: &mut Criterion) {
    // Many sources spread over 5 km × 5 km; receivers in a 200 m × 200 m area.
    let n_src = 50;
    let sources: Vec<SourceSpec> = (0..n_src)
        .map(|i| make_source((i as f64) * 100.0, (i as f64) * 100.0, 90.0))
        .collect();

    let receivers: Vec<Point3<f64>> = (0..100)
        .map(|i| Point3::new((i % 10) as f64 * 20.0, (i / 10) as f64 * 20.0, 4.0))
        .collect();

    let calc_no_cull = GridCalculator::new(CalculatorConfig::default());
    let calc_cull = GridCalculator::new(CalculatorConfig {
        max_source_range_m: Some(500.0),
        ..Default::default()
    });

    let mut group = c.benchmark_group("spatial_culling");

    group.bench_function("no_cull_50src_100rcv", |b| {
        b.iter(|| {
            calc_no_cull.calculate_points(
                black_box(&receivers),
                black_box(&sources),
                black_box(&[]),
            )
        })
    });

    group.bench_function("cull_500m_50src_100rcv", |b| {
        b.iter(|| {
            calc_cull.calculate_points(
                black_box(&receivers),
                black_box(&sources),
                black_box(&[]),
            )
        })
    });

    group.finish();
}

// ─── calculate_points at scale ────────────────────────────────────────────────

fn bench_calculate_points_scale(c: &mut Criterion) {
    let calc = GridCalculator::new(CalculatorConfig::default());
    let src = make_source(0.0, 0.0, 100.0);

    let mut group = c.benchmark_group("calculate_points_scale");
    for n in [100usize, 1_000, 5_000] {
        let receivers: Vec<Point3<f64>> = (0..n)
            .map(|i| Point3::new(i as f64 * 2.0, 0.0, 4.0))
            .collect();

        group.bench_with_input(
            BenchmarkId::from_parameter(n),
            &n,
            |b, _| {
                b.iter(|| {
                    calc.calculate_points(
                        black_box(&receivers),
                        black_box(&[src.clone()]),
                        black_box(&[]),
                    )
                })
            },
        );
    }
    group.finish();
}

criterion_group!(
    benches,
    bench_grid_single_source,
    bench_grid_multi_source,
    bench_spatial_culling,
    bench_calculate_points_scale,
);
criterion_main!(benches);
