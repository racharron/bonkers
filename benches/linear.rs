use std::sync::Arc;
use std::time::Duration;
use bonkers::{Cown, OsThreads, SimpleThreadPool, Runner};
use criterion::{criterion_group, criterion_main, AxisScale, BenchmarkId, Criterion, PlotConfiguration, Throughput};

const SIZES: [usize; 11] = [1, 2, 4, 8, 16, 32, 64, 128, 256, 512, 1024];
const MAX_THREADS: usize = 8;

criterion_group!(linear, os, simple, rayon, tp);
criterion_main!(linear);

fn run<R: Runner>(c: &mut Criterion, runner: R, name: &str) {
    let mut group = c.benchmark_group(name);
    for size in SIZES {
        group.warm_up_time(Duration::from_millis(500));
        group.throughput(Throughput::Elements(size as u64));
        group.plot_config(PlotConfiguration::default().summary_scale(AxisScale::Logarithmic));
        let runner = runner.clone();
        let cown = Arc::new(Cown::new(2 * size));
        group.bench_with_input(BenchmarkId::from_parameter(size), &size, |b, &size| {
            b.iter(|| for _ in 0..size { runner.when(cown.clone(), move |mut cown| *cown += 1) })
        });
    }
    group.finish();
}


fn os(c: &mut Criterion) {
    let pool = Arc::new(OsThreads::new());
    run(c, pool, "os_linear");
}


fn simple(c: &mut Criterion) {
    for threads in 1..=MAX_THREADS {
        let pool = Arc::new(SimpleThreadPool::with_threads(threads.try_into().unwrap()));
        run(c, pool, &format!("simple_linear[{threads}]"))
    }
}


fn rayon(c: &mut Criterion) {
    for threads in 1..=MAX_THREADS {
        let pool = Arc::new(rayon_core::ThreadPoolBuilder::new().num_threads(threads).build().unwrap());
        run(c, pool, &format!("rayon_linear[{threads}]"))
    }
}


fn tp(c: &mut Criterion) {
    for threads in 1..=MAX_THREADS {
        let pool = Arc::new(threadpool::ThreadPool::new(threads));
        run(c, pool, &format!("tp_linear[{threads}]"))
    }
}
