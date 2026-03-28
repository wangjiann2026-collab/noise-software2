use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};
use nalgebra::Point3;
use noise_core::engine::ray_tracer::{RayTracer, RayTracerConfig};

fn bench_direct_path(c: &mut Criterion) {
    let config = RayTracerConfig {
        max_reflection_order: 0,
        ..Default::default()
    };
    let tracer = RayTracer::new(config).unwrap();
    let src = Point3::new(0.0, 0.0, 0.5);
    let rcv = Point3::new(100.0, 50.0, 4.0);

    c.bench_function("direct_path_computation", |b| {
        b.iter(|| {
            tracer.compute_paths(black_box(&src), black_box(&rcv), &[]).unwrap()
        })
    });
}

fn bench_parallel_grid(c: &mut Criterion) {
    use noise_core::parallel::{ParallelScheduler, SchedulerConfig};
    use noise_core::engine::ray_tracer::{RayTracer, RayTracerConfig};

    let sched = ParallelScheduler::new(SchedulerConfig::default());
    let config = RayTracerConfig { max_reflection_order: 0, ..Default::default() };
    let tracer = std::sync::Arc::new(RayTracer::new(config).unwrap());
    let src = Point3::new(0.0, 0.0, 0.5);

    let mut group = c.benchmark_group("parallel_grid");
    for size in [100usize, 1_000, 10_000].iter() {
        let receivers: Vec<Point3<f64>> = (0..*size)
            .map(|i| Point3::new(i as f64 * 5.0, 0.0, 4.0))
            .collect();

        group.bench_with_input(BenchmarkId::from_parameter(size), size, |b, _| {
            let t = tracer.clone();
            b.iter(|| {
                sched.map(
                    receivers.clone(),
                    |rcv| t.compute_paths(&src, &rcv, &[]).unwrap().len(),
                    None,
                )
            });
        });
    }
    group.finish();
}

criterion_group!(benches, bench_direct_path, bench_parallel_grid);
criterion_main!(benches);
