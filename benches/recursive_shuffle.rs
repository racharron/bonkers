use crate::util::MAX_THREADS;
use bonkers::Mut;
#[allow(unused_imports)] // `util` needs `Cown` to be imported, but this is not recognized while compiling.
#[allow(unused_imports)] // `util` needs `Cown` to be imported, but this is not recognized while compiling.
use bonkers::{Cown, OsThreads, Runner, SimpleThreadPool};
use criterion::measurement::WallTime;
use criterion::{criterion_group, criterion_main, AxisScale, BenchmarkGroup, BenchmarkId, Criterion, PlotConfiguration, Throughput};
use rand::prelude::{SliceRandom, SmallRng};
use rand::{Rng, SeedableRng};
use std::sync::mpsc::{channel, Sender};
use std::sync::Arc;
use std::time::Duration;

#[allow(unused)]
mod util;

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
            recursive_shuffle(runner, max_depth)
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

pub fn recursive_shuffle(runner: impl Runner, max_depth: usize) {
    const WIDTH: usize = 4;
    const COUNT: usize = 64;
    struct List {
        local: usize,
        previous: Option<Arc<List>>,
    }
    impl List {
        pub fn id(&self) -> usize {
            if let Some(previous) = &self.previous {
                previous.id() * (WIDTH + 1) + self.local
            } else {
                self.local
            }
        }
    }
    fn recurse(
        runner: impl Runner,
        cowns: Arc<[Arc<Cown<Vec<usize>>>]>,
        sender: Sender<()>,
        mut rng: SmallRng,
        depth: usize,
        max_depth: usize,
        previous: Option<Arc<List>>,
    ) {
        for i in 1..=WIDTH {
            let ident = Arc::new(List {
                local: i,
                previous: previous.clone(),
            });
            let id = ident.id();
            let len = (rng.gen_range(1..=(COUNT / 2).pow(2)) as f32).sqrt() as usize;
            let vec = cowns.choose_multiple(&mut rng, len).cloned().collect::<Vec<_>>();
            if depth == max_depth {
                let sender = sender.clone();
                runner.when(Mut(vec), move |mut cowns| {
                    for cown in &mut cowns {
                        cown.push(id);
                    }
                    sender.send(()).unwrap();
                });
            } else {
                let sender = sender.clone();
                let cowns = cowns.clone();
                let sender = sender.clone();
                let rng = SmallRng::from_rng(&mut rng).unwrap();
                runner.when(Mut(vec), {
                    let runner = runner.clone();
                    move |mut current| {
                        recurse(runner, cowns.clone(), sender.clone(), rng, depth + 1, max_depth, Some(ident));
                        for cown in &mut current {
                            cown.push(id);
                        }
                        sender.send(()).unwrap();
                    }
                })
            }
        }
    }
    let cowns = (0..COUNT).map(|_| Arc::new(Cown::new(Vec::<usize>::new()))).collect::<Vec<_>>();
    let (sender, receiver) = channel();
    recurse(runner, cowns.into(), sender, SmallRng::seed_from_u64(123), 1, max_depth, None);
    for _ in 0..(1..=max_depth).map(|d| WIDTH.pow(d as _)).sum() {
        receiver.recv().unwrap();
    }
}
