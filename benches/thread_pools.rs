use bonkers::{OsThreads, SimpleThreadPool, ThreadPool};
use criterion::measurement::WallTime;
use criterion::{criterion_group, criterion_main, AxisScale, BenchmarkGroup, BenchmarkId, Criterion, PlotConfiguration, Throughput};
use std::sync::mpsc::channel;
use std::time::Duration;
use util::SIZES;

mod util;

criterion_group!(thread_pools, os_nop, simple_nop, rayon_nop, tp_nop);
criterion_main!(thread_pools);

fn bench<TP: ThreadPool>(group: &mut BenchmarkGroup<WallTime>, size: usize, pool: &TP, benchmark_id: BenchmarkId) {
    group.bench_with_input(benchmark_id, &size, move |b, &size| {
        b.iter(move || {
            let (sender, receiver) = channel();
            for _ in 0..size {
                let sender = sender.clone();
                pool.run(move || sender.send(()).unwrap());
            }
            for _ in 0..size {
                receiver.recv().unwrap();
            }
        })
    });
}

fn os_nop(c: &mut Criterion) {
    let pool = OsThreads::new();
    let mut group = c.benchmark_group("os_nop");
    group.warm_up_time(Duration::from_millis(500));
    group.plot_config(PlotConfiguration::default().summary_scale(AxisScale::Logarithmic));
    for size in SIZES {
        group.throughput(Throughput::Elements(size as u64));
        let benchmark_id = BenchmarkId::from_parameter(size);
        bench(&mut group, size, &pool, benchmark_id);
    }
    group.finish();
}

fn simple_nop(c: &mut Criterion) {
    util::run(c, "simple_nop", |threads| SimpleThreadPool::with_threads(threads.try_into().unwrap()), bench);
}

fn rayon_nop(c: &mut Criterion) {
    util::run(
        c,
        "rayon_nop",
        |threads| rayon_core::ThreadPoolBuilder::new().num_threads(threads).build().unwrap(),
        bench,
    );
}

fn tp_nop(c: &mut Criterion) {
    util::run(c, "tp_nop", threadpool::ThreadPool::new, bench);
}
