use std::sync::Arc;
use std::time::Duration;
use bonkers::{OsThreads, SimpleThreadPool, Runner};
use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};

#[path = "../src/tests/util.rs"]
#[allow(unused)]
mod util;

const MAX_THREADS: usize = 10;

criterion_group!(linear, os, simple, rayon, tp);
criterion_main!(linear);

fn run<R: Runner>(c: &mut Criterion, runner: R, name: &str) {
    let mut group = c.benchmark_group(name);
    const MAX_DEPTH: u32 = 5;
    for depth in 1..=MAX_DEPTH {
        group.warm_up_time(Duration::from_millis(500));
        group.throughput(Throughput::Elements((1..=depth).map(|d| 4u64.pow(d)).sum()));
        let runner = runner.clone();
        group.bench_with_input(BenchmarkId::from_parameter(depth as u64), &depth, |b, &max_depth| {
            b.iter(|| util::recursive_shuffle(runner.clone(), max_depth as usize))
        });
    }
    group.finish();
}


fn os(c: &mut Criterion) {
    let pool = Arc::new(OsThreads::new());
    run(c, pool, "os_recursive_shuffle");
}


fn simple(c: &mut Criterion) {
    for threads in 1..=MAX_THREADS {
        let pool = Arc::new(SimpleThreadPool::with_threads(threads.try_into().unwrap()));
        run(c, pool, &format!("simple_recursive_shuffle[{threads}]"))
    }
}


fn rayon(c: &mut Criterion) {
    for threads in 1..=MAX_THREADS {
        let pool = Arc::new(rayon_core::ThreadPoolBuilder::new().num_threads(threads).build().unwrap());
        run(c, pool, &format!("rayon_recursive_shuffle[{threads}]"))
    }
}


fn tp(c: &mut Criterion) {
    for threads in 1..=MAX_THREADS {
        let pool = Arc::new(threadpool::ThreadPool::new(threads));
        run(c, pool, &format!("tp_recursive_shuffle[{threads}]"))
    }
}
