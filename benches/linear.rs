use bonkers::{Mut, OsThreads, Ref, Runner, SimpleThreadPool, Cown};
use criterion::measurement::WallTime;
use criterion::{criterion_group, criterion_main, AxisScale, BenchmarkGroup, BenchmarkId, Criterion, PlotConfiguration, Throughput};
use std::sync::mpsc::channel;
use std::sync::Arc;
use std::time::Duration;

#[allow(unused)]
mod util;

criterion_group!(linear, os, simple, rayon, tp);
criterion_main!(linear);

fn bench<R: Runner>(group: &mut BenchmarkGroup<WallTime>, size: usize, runner: &R, benchmark_id: BenchmarkId) {
    let cown = Arc::new(Cown::new(2 * size));
    group.bench_with_input(benchmark_id, &size, |b, &size| {
        let (sender, receiver) = channel();
        b.iter(|| {
            for _ in 0..size {
                runner.when(Mut(cown.clone()), move |cown| *cown += 1)
            }
            let sender = sender.clone();
            runner.when(Ref(cown.clone()), move |_| sender.send(()).unwrap());
            receiver.recv().unwrap();
        })
    });
}

fn os(c: &mut Criterion) {
    let pool = Arc::new(OsThreads::new());
    let mut group = c.benchmark_group("os_linear");
    for size in util::SIZES {
        group.warm_up_time(Duration::from_millis(500));
        group.throughput(Throughput::Elements(size as u64));
        group.plot_config(PlotConfiguration::default().summary_scale(AxisScale::Logarithmic));
        let runner = pool.clone();
        let benchmark_id = BenchmarkId::from_parameter(size);
        bench(&mut group, size, &runner, benchmark_id);
    }
    group.finish();
}

fn simple(c: &mut Criterion) {
    util::run(
        c,
        "simple_linear",
        |threads| Arc::new(SimpleThreadPool::with_threads(threads.try_into().unwrap())),
        bench,
    );
}

fn rayon(c: &mut Criterion) {
    util::run(
        c,
        "rayon_linear",
        |threads| Arc::new(rayon_core::ThreadPoolBuilder::new().num_threads(threads).build().unwrap()),
        bench,
    );
}

fn tp(c: &mut Criterion) {
    util::run(c, "tp_linear", threadpool::ThreadPool::new, bench);
}
