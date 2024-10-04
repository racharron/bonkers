use crate::{Cown, Runner};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{channel, Sender};
use std::sync::{Arc, MutexGuard};
use rand::prelude::SliceRandom;
use rand::{Rng, SeedableRng};
use rand::rngs::SmallRng;

pub fn when_none_simple(runner: impl Runner) {
    let (sender, receiver) = channel();
    runner.when((), move |()| {
        sender.send(()).unwrap();
    });
    receiver.recv().unwrap();
}

pub fn when_none_loop(runner: impl Runner) {
    let mut receivers = Vec::new();
    let mut values = Vec::new();
    for i in 0..10 {
        let value = 1_000 - i;
        values.push(value);
        let (sender, receiver) = channel();
        receivers.push(receiver);
        runner.when((), move |()| sender.send(value).unwrap())
    }
    for (receiver, value) in receivers.into_iter().zip(values) {
        assert_eq!(receiver.recv().unwrap(), value);
    }
}

pub fn sequential_1(runner: impl Runner) {
    const COUNT: usize = 10;
    let cown = Arc::new(Cown::new(Vec::with_capacity(COUNT)));
    for i in 0..COUNT {
        runner.when(cown.clone(), move |mut cown| {
            cown.push(i);
        });
    }
    let (sender, receiver) = channel();
    runner.when(cown.clone(), move |mut guard| {
        for (i, v) in guard.drain(..).enumerate() {
            assert_eq!(i, v);
        }
        sender.send(()).unwrap();
    });
    receiver.recv().unwrap();
}

pub fn indirect_seq(runner: impl Runner) {
    let [a, b, c, d] = [0; 4].map(Cown::new).map(Arc::new);
    let counter = Arc::new(AtomicUsize::new(0));
    let (sender, receiver) = channel();
    runner.when((a.clone(), b.clone()), {
        let counter = counter.clone();
        move |(mut a, mut b)| {
            assert_eq!(*a, 0);
            *a += 1;
            assert_eq!(*b, 0);
            *b += 1;
            assert_eq!(counter.fetch_add(1, Ordering::AcqRel), 0);
        }
    });
    runner.when((b.clone(), c.clone()), {
        let counter = counter.clone();
        move |(mut b, mut c)| {
            assert_eq!(*b, 1);
            *b += 1;
            assert_eq!(*c, 0);
            *c += 1;
            assert_eq!(counter.fetch_add(1, Ordering::AcqRel), 1);
        }
    });
    runner.when((c.clone(), d.clone()), {
        let counter = counter.clone();
        move |(mut c, mut d)| {
            assert_eq!(*c, 1);
            *c += 1;
            assert_eq!(*d, 0);
            *d += 1;
            assert_eq!(counter.fetch_add(1, Ordering::AcqRel), 2);
        }
    });
    runner.when((a, b, c, d), move |(a, b, c, d)| {
        assert_eq!(*a, 1);
        assert_eq!(*b, 2);
        assert_eq!(*c, 2);
        assert_eq!(*d, 1);
        assert_eq!(counter.load(Ordering::Acquire), 3);
        sender.send(()).unwrap();
    });
    receiver.recv().unwrap();
}

pub fn triangle(runner: impl Runner) {
    let [top, left, right] = [0; 3].map(Cown::new).map(Arc::new);
    let counter = Arc::new(AtomicUsize::new(0));
    let (sender, receiver) = channel();
    runner.when(top.clone(), {
        let counter = counter.clone();
        move |mut top| {
            assert_eq!(counter.fetch_add(1, Ordering::AcqRel), 0);
            assert_eq!(*top, 0);
            *top += 1;
        }
    });
    let sides = {
        let counter = counter.clone();
        move |(mut top, mut side): (MutexGuard<i32>, MutexGuard<i32>)| {
            assert!([1, 2].contains(&counter.fetch_add(1, Ordering::AcqRel)));
            assert!([1, 2].contains(&*top));
            assert_eq!(*side, 0);
            *top += 1;
            *side += 1;
        }
    };
    runner.when((top.clone(), left.clone()), sides.clone());
    runner.when((top.clone(), right.clone()), sides);
    runner.when((left.clone(), right.clone()), {
        let counter = counter.clone();
        move |(mut left, mut right)| {
            assert_eq!(counter.fetch_add(1, Ordering::AcqRel), 3);
            assert_eq!(*left, 1);
            assert_eq!(*right, 1);
            *left += 1;
            *right += 1;
        }
    });
    runner.when((top, right, left), {
        let counter = counter.clone();
        move |(top, left, right)| {
            assert_eq!(counter.load(Ordering::Acquire), 4);
            assert_eq!(*top, 3);
            assert_eq!(*left, 2);
            assert_eq!(*right, 2);
            sender.send(()).unwrap();
        }
    });
    receiver.recv().unwrap();
}

pub fn recursive_sequential(runner: impl Runner, max_depth: usize) {
    fn inner(runner: impl Runner, cown: Arc<Cown<usize>>, sender: Sender<()>, max_depth: usize, depth: usize) {
        if depth == max_depth {
            runner.when(cown, move |c| {
                assert_eq!(*c, 100 + depth);
                sender.send(()).unwrap();
            })
        } else {
            runner.when(cown.clone(), {
                let runner = runner.clone();
                move |mut c| {
                    assert_eq!(*c, 100 + depth);
                    *c += 1;
                    inner(runner, cown, sender, max_depth, depth + 1);
                }
            });
        }
    }
    let cown = Arc::new(Cown::new(100));
    let (sender, receiver) = channel();
    inner(runner, cown, sender, max_depth - 1, 0);
    receiver.recv().unwrap();
}

pub fn recursive_shuffle<>(runner: impl Runner, max_depth: usize) {
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
        previous: Option<Arc<List>>
    ) {
        for i in 1..=WIDTH {
            let ident = Arc::new(List { local: i, previous: previous.clone() });
            let id = ident.id();
            let len = (rng.gen_range(1..=(COUNT/2).pow(2)) as f32).sqrt() as usize;
            let vec = cowns.choose_multiple(&mut rng, len).cloned().collect::<Vec<_>>();
            if depth == max_depth {
                let sender = sender.clone();
                runner.when(vec, move |cowns| {
                    for mut cown in cowns {
                        cown.push(id);
                    }
                    sender.send(()).unwrap();
                });
            } else {
                let sender = sender.clone();
                let cowns = cowns.clone();
                let rng = SmallRng::from_rng(&mut rng).unwrap();
                runner.when(vec, {
                    let runner = runner.clone();
                    move |mut current| {
                        recurse(runner, cowns.clone(), sender, rng, depth + 1, max_depth, Some(ident));
                        for cown in &mut current {
                            cown.push(id);
                        }
                    }
                })
            }
        }
    }
    let cowns = (0..COUNT).map(|_| Arc::new(Cown::new(Vec::<usize>::new()))).collect::<Vec<_>>();
    let (sender, receiver) = channel();
    recurse(runner, cowns.into(), sender, SmallRng::seed_from_u64(123), 1, max_depth, None);
    for _ in 0..WIDTH.pow(max_depth as _) {
        receiver.recv().unwrap();
    }
}
