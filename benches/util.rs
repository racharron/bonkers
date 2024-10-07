use std::sync::Arc;
use std::sync::mpsc::{channel, Sender};
use criterion::measurement::WallTime;
use criterion::{AxisScale, BenchmarkGroup, BenchmarkId, Criterion, PlotConfiguration, Throughput};
use std::time::Duration;
use rand::prelude::SmallRng;
use rand::{Rng, SeedableRng};
use rand::seq::SliceRandom;
use bonkers::{Cown, Mut, Runner};

pub const SIZES: [usize; 11] = [1, 2, 4, 8, 16, 32, 64, 128, 256, 512, 1024];
pub const MAX_THREADS: usize = 8;

pub fn run<T, NEW, BENCH>(c: &mut Criterion, name: &str, new: NEW, bench: BENCH)
where
    NEW: Fn(usize) -> T,
    BENCH: Fn(&mut BenchmarkGroup<WallTime>, usize, &T, BenchmarkId),
{
    let mut group = c.benchmark_group(name);
    group.warm_up_time(Duration::from_millis(500));
    group.plot_config(PlotConfiguration::default().summary_scale(AxisScale::Logarithmic));
    for threads in 1..=MAX_THREADS {
        let pool = new(threads);
        for size in SIZES {
            group.throughput(Throughput::Elements(size as u64));
            let benchmark_id = BenchmarkId::new(if threads == 1 { "1-thread".to_string() } else { format!("{threads}-threads") }, size);
            bench(&mut group, size, &pool, benchmark_id);
        }
    }
    group.finish();
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
