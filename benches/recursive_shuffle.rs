use crate::util::MAX_THREADS;
#[allow(unused_imports)] // `util` needs `Cown` to be imported, but this is not recognized while compiling.
use bonkers::{Cown, OsThreads, Runner, SimpleThreadPool};
use criterion::measurement::WallTime;
use criterion::{criterion_group, criterion_main, AxisScale, BenchmarkGroup, BenchmarkId, Criterion, PlotConfiguration, Throughput};
use std::sync::Arc;
use std::time::Duration;

#[allow(unused)]
mod util;

#[path = "../src/tests/util.rs"]
#[allow(unused)]
mod test_util;

const MAX_DEPTH: u32 = 5;

criterion_group!(linear, os, simple, rayon, tp);
criterion_main!(linear);

fn run<R: Runner, F: Fn(usize) -> R>(c: &mut Criterion, name: &str, new: F) {
    let mut group = c.benchmark_group(name);
    group.warm_up_time(Duration::from_millis(500));
    group.plot_config(PlotConfiguration::default().summary_scale(AxisScale::Logarithmic));
    for threads in 1..=MAX_THREADS {
        let runner = new(threads);
        for max_depth in 1..=MAX_DEPTH {
            group.throughput(Throughput::Elements(size(max_depth)));
            let benchmark_id = BenchmarkId::new(
                if threads == 1 { "1-thread".to_string() } else { format!("{threads}-threads") },
                size(max_depth),
            );
            bench(&mut group, max_depth as _, &runner, benchmark_id);
        }
    }
    group.finish();
}

fn size(depth: u32) -> u64 {
    (1..=depth).map(|d| 4u64.pow(d)).sum()
}

fn bench<R: Runner>(group: &mut BenchmarkGroup<WallTime>, max_depth: usize, runner: &R, benchmark_id: BenchmarkId) {
    group.bench_with_input(benchmark_id, &max_depth, |b, &max_depth| {
        b.iter(|| {
            let runner = runner.clone();
            test_util::recursive_shuffle(runner, max_depth)
        })
    });
}

fn os(c: &mut Criterion) {
    let runner = Arc::new(OsThreads::new());
    let mut group = c.benchmark_group("os_recursive_shuffle");
    group.warm_up_time(Duration::from_millis(500));
    group.plot_config(PlotConfiguration::default().summary_scale(AxisScale::Logarithmic));
    for max_depth in 1..=MAX_DEPTH {
        group.throughput(Throughput::Elements(size(max_depth)));
        bench(&mut group, max_depth as _, &runner, BenchmarkId::from_parameter(size(max_depth)));
    }
    group.finish();
}

fn simple(c: &mut Criterion) {
    run(c, "simple_recursive_shuffle", |threads| {
        Arc::new(SimpleThreadPool::with_threads(threads.try_into().unwrap()))
    });
}

fn rayon(c: &mut Criterion) {
    run(c, "rayon_recursive_shuffle", |threads| {
        Arc::new(rayon_core::ThreadPoolBuilder::new().num_threads(threads).build().unwrap())
    });
}

fn tp(c: &mut Criterion) {
    run(c, "tp_recursive_shuffle", threadpool::ThreadPool::new);
}
