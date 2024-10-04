use bonkers::{OsThreads, SimpleThreadPool, ThreadPool};

fn main() {
    divan::main();
}

#[divan::bench(
    args = [1, 2, 4, 8, 16, 32, 64, 128, 256, 512, 1024, 2048, 4096]
)]
fn os_nop(bencher: divan::Bencher, n: usize) {
    let pool = OsThreads::new();
    bencher
        .bench(move || {
            for _ in 0..n {
                pool.run(|| {});
            }
        });
}

#[divan::bench(
    consts = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10],
    args = [1, 2, 4, 8, 16, 32, 64, 128, 256, 512, 1024, 2048, 4096]
)]
fn simple_nop<const THREADS: usize>(bencher: divan::Bencher, n: usize) {
    let pool = SimpleThreadPool::with_threads(THREADS.try_into().unwrap());
    bencher
        .bench(move || {
            for _ in 0..n {
                pool.run(|| {});
            }
        });
}

#[divan::bench(
    consts = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10],
    args = [1, 2, 4, 8, 16, 32, 64, 128, 256, 512, 1024, 2048, 4096]
)]
fn rayon_nop<const THREADS: usize>(bencher: divan::Bencher, n: usize) {
    let pool = rayon_core::ThreadPoolBuilder::new().num_threads(THREADS).build().unwrap();
    bencher
        .bench(move || {
            for _ in 0..n {
                pool.run(|| {});
            }
        });
}

#[divan::bench(
    consts = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10],
    args = [1, 2, 4, 8, 16, 32, 64, 128, 256, 512, 1024, 2048, 4096]
)]
fn tp_nop<const THREADS: usize>(bencher: divan::Bencher, n: usize) {
    let pool = threadpool::ThreadPool::new(THREADS);
    bencher
        .bench(move || {
            for _ in 0..n {
                pool.run(|| {});
            }
        });
}
