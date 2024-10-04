use std::sync::Arc;
use bonkers::{Cown, OsThreads, SimpleThreadPool, Runner};

fn main() {
    divan::main();
}

#[divan::bench(
    args = [1, 2, 4, 8, 16, 32, 64, 128, 256, 512, 1024, 2048, 4096]
)]
fn os(bencher: divan::Bencher, n: usize) {
    let pool = Arc::new(OsThreads::new());
    let cown = Arc::new(Cown::new(0));
    bencher
        .with_inputs(|| cown.clone())
        .bench_values(move |cown| {
            for i in 0..n {
                pool.when(cown.clone(), move |mut cown| *cown += i);
            }
            cown
        });
}

#[divan::bench(
    consts = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10],
    args = [1, 2, 4, 8, 16, 32, 64, 128, 256, 512, 1024, 2048, 4096]
)]
fn simple<const THREADS: usize>(bencher: divan::Bencher, n: usize) {
    let pool = Arc::new(SimpleThreadPool::with_threads(THREADS.try_into().unwrap()));
    let cown = Arc::new(Cown::new(0));
    bencher
        .with_inputs(|| cown.clone())
        .bench_values(move |cown| {
            for i in 0..n {
                pool.when(cown.clone(), move |mut cown| *cown += i);
            }
            cown.clone()
        });
}

#[divan::bench(
    consts = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10],
    args = [1, 2, 4, 8, 16, 32, 64, 128, 256, 512, 1024, 2048, 4096]
)]
fn rayon<const THREADS: usize>(bencher: divan::Bencher, n: usize) {
    let pool = Arc::new(rayon_core::ThreadPoolBuilder::new().num_threads(THREADS).build().unwrap());
    let cown = Arc::new(Cown::new(0));
    bencher
        .with_inputs(|| cown.clone())
        .bench_values(move |cown| {
            for i in 0..n {
                pool.when(cown.clone(), move |mut cown| *cown += i);
            }
            cown.clone()
        });
}

#[divan::bench(
    consts = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10],
    args = [1, 2, 4, 8, 16, 32, 64, 128, 256, 512, 1024, 2048, 4096]
)]
fn tp<const THREADS: usize>(bencher: divan::Bencher, n: usize) {
    let pool = Arc::new(threadpool::ThreadPool::new(THREADS));
    let cown = Arc::new(Cown::new(0));
    bencher
        .with_inputs(|| cown.clone())
        .bench_values(move |cown| {
            for i in 0..n {
                pool.when(cown.clone(), move |mut cown| *cown += i);
            }
            cown.clone()
        });
}
