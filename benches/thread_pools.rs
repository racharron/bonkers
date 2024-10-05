use std::sync::Arc;
use std::sync::mpsc::channel;
use std::time::Duration;
use bonkers::{OsThreads, SimpleThreadPool, ThreadPool};
use criterion::{criterion_group, criterion_main, AxisScale, BenchmarkId, Criterion, PlotConfiguration, Throughput};

const SIZES: [usize; 11] = [1, 2, 4, 8, 16, 32, 64, 128, 256, 512, 1024];
const MAX_THREADS: usize = 8;

criterion_group!(thread_pools, os_nop, simple_nop, rayon_nop, tp_nop);
criterion_main!(thread_pools);

fn run<TP: ThreadPool>(c: &mut Criterion, pool: TP, name: &str) {
    let pool = Arc::new(pool);
    let mut group = c.benchmark_group(name);
    for size in SIZES {
        group.warm_up_time(Duration::from_millis(500));
        group.throughput(Throughput::Elements(size as u64));
        group.plot_config(PlotConfiguration::default().summary_scale(AxisScale::Logarithmic));
        let pool = pool.clone();
        group.bench_with_input(BenchmarkId::from_parameter(size), &size, move |b, &size| {
            let pool = pool.clone();
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
    group.finish();
}

fn os_nop(c: &mut Criterion) {
    let pool = OsThreads::new();
    run(c, pool, "os_nop");
}

fn simple_nop(c: &mut Criterion) {
    for threads in 1..=MAX_THREADS {
        let pool = SimpleThreadPool::with_threads(threads.try_into().unwrap());
        run(c, pool, &format!("simple_nop[{threads}]"));
    }
}

fn rayon_nop(c: &mut Criterion) {
    for threads in 1..=MAX_THREADS {
        let pool = rayon_core::ThreadPoolBuilder::new().num_threads(threads).build().unwrap();
        run(c, pool, &format!("rayon_nop[{threads}]"));
    }
}

fn tp_nop(c: &mut Criterion) {
    for threads in 1..=MAX_THREADS {
        let pool = threadpool::ThreadPool::new(threads);
        run(c, pool, &format!("tp_nop[{threads}]"));
    }
}
