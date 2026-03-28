//! Criterion benchmarks for the ISO 9613-2 propagation model.
//!
//! Run with:
//! ```text
//! cargo bench -p noise-core --bench propagation_bench
//! ```

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};
use nalgebra::Point3;
use noise_core::engine::propagation::{PropagationConfig, PropagationModel};
use noise_core::engine::ground_effect::GroundPath;
use noise_core::engine::diffraction::DiffractionEdge;
use noise_core::simd::{energy_sum_bands, avx2_available};

const LW: [f64; 8] = [90.0, 92.0, 94.0, 96.0, 94.0, 92.0, 88.0, 82.0];
const A_WEIGHTS: [f64; 8] = [-26.2, -16.1, -8.6, -3.2, 0.0, 1.2, 1.0, -1.1];

fn bench_propagation_compute(c: &mut Criterion) {
    let model = PropagationModel::new(PropagationConfig::default());
    let src = Point3::new(0.0, 0.0, 0.5);

    let mut group = c.benchmark_group("propagation_compute");
    for dist in [10.0f64, 100.0, 500.0, 2000.0] {
        let rcv = Point3::new(dist, 0.0, 4.0);
        let ground = GroundPath {
            source_height_m: 0.5,
            receiver_height_m: 4.0,
            distance_m: dist,
            g_source: 0.5,
            g_receiver: 0.5,
            g_middle: 0.5,
        };
        group.bench_with_input(
            BenchmarkId::new("no_barrier", format!("{dist:.0}m")),
            &dist,
            |b, _| {
                b.iter(|| {
                    model.compute(
                        black_box(&src),
                        black_box(&rcv),
                        black_box(&ground),
                        black_box(&[]),
                        None,
                    )
                })
            },
        );
    }
    group.finish();
}

fn bench_propagation_with_barrier(c: &mut Criterion) {
    let model = PropagationModel::new(PropagationConfig::default());
    let src = Point3::new(0.0, 0.0, 0.5);
    let rcv = Point3::new(200.0, 0.0, 4.0);
    let ground = GroundPath {
        source_height_m: 0.5,
        receiver_height_m: 4.0,
        distance_m: 200.0,
        g_source: 0.5,
        g_receiver: 0.5,
        g_middle: 0.5,
    };
    let barriers = vec![
        DiffractionEdge { point: Point3::new(100.0, 0.0, 5.0), height_m: 5.0 },
    ];

    c.bench_function("propagation_with_barrier_200m", |b| {
        b.iter(|| {
            model.compute(
                black_box(&src),
                black_box(&rcv),
                black_box(&ground),
                black_box(&barriers),
                None,
            )
        })
    });
}

fn bench_apply_to_lw(c: &mut Criterion) {
    let model = PropagationModel::new(PropagationConfig::default());
    let src = Point3::new(0.0, 0.0, 0.5);
    let rcv = Point3::new(100.0, 0.0, 4.0);
    let ground = GroundPath {
        source_height_m: 0.5, receiver_height_m: 4.0, distance_m: 100.0,
        g_source: 0.5, g_receiver: 0.5, g_middle: 0.5,
    };
    let breakdown = model.compute(&src, &rcv, &ground, &[], None);

    c.bench_function("apply_to_lw_simd", |b| {
        b.iter(|| breakdown.apply_to_lw(black_box(&LW)))
    });
}

fn bench_energy_sum_bands(c: &mut Criterion) {
    let a_total = [31.0f64, 31.5, 32.0, 32.5, 33.0, 33.5, 34.0, 34.5];

    let mut group = c.benchmark_group("energy_sum_bands");

    group.bench_function("dispatch", |b| {
        b.iter(|| energy_sum_bands(black_box(&LW), black_box(&a_total), black_box(&A_WEIGHTS)))
    });

    if avx2_available() {
        group.bench_function("avx2_available", |b| {
            b.iter(|| energy_sum_bands(black_box(&LW), black_box(&a_total), black_box(&A_WEIGHTS)))
        });
    }

    group.finish();
}

fn bench_lp_simple_batch(c: &mut Criterion) {
    let model = PropagationModel::new(PropagationConfig::default());
    let src = Point3::new(0.0, 0.0, 0.5);
    let receivers: Vec<Point3<f64>> = (1..=100)
        .map(|i| Point3::new(i as f64 * 10.0, 0.0, 4.0))
        .collect();

    c.bench_function("lp_simple_100_receivers", |b| {
        b.iter(|| {
            let _: Vec<f64> = receivers
                .iter()
                .map(|rcv| model.lp_simple(black_box(&LW), black_box(&src), black_box(rcv), 0.5))
                .collect();
        })
    });
}

criterion_group!(
    benches,
    bench_propagation_compute,
    bench_propagation_with_barrier,
    bench_apply_to_lw,
    bench_energy_sum_bands,
    bench_lp_simple_batch,
);
criterion_main!(benches);
