use super::*;
use crate::thread_pool::MyThreadPool;
use std::num::{NonZero, NonZeroUsize};

macro_rules! select_test {
    ($( $n:ident $b:block )+) => {
        $(
            case! {
                std: { #[test] fn $n() $b }
                loom: { #[test] fn $n() { loom::model(|| $b); } }
            }
        )+
    };
}

select_test! {
    when_none_simple {
        let pool = Arc::new(MyThreadPool::with_threads(NonZeroUsize::new(4).unwrap()));
        let (sender, receiver) = channel();
        pool.when((), move |()| {
            sender.send(()).unwrap();
        });
        receiver.recv().unwrap();
        pool.shutdown();
    }

    when_none_loop {
        let pool = Arc::new(MyThreadPool::with_threads(NonZeroUsize::new(4).unwrap()));
        // let pool = Arc::new(OsThreads::new());
        let mut receivers = Vec::new();
        let mut values = Vec::new();
        for i in 0..100 {
            let value = 1_000 - i;
            values.push(value);
            let (sender, receiver) = channel();
            receivers.push(receiver);
            pool.when((), move |()| sender.send(value).unwrap())
        }
        for (receiver, value) in receivers.into_iter().zip(values) {
            assert_eq!(receiver.recv().unwrap(), value);
        }
        pool.shutdown();
    }

    sequential_1 {
        const COUNT: usize = 100;
        let pool = Arc::new(MyThreadPool::with_threads(NonZeroUsize::new(4).unwrap()));
        let cown = Arc::new(Cown::new(Vec::with_capacity(COUNT)));
        for i in 0..COUNT {
            pool.when(cown.clone(), move |mut cown| {
                cown.push(i);
            });
        }
        let (sender, receiver) = channel();
        pool.when(cown.clone(), move |mut guard| {
            for (i, v) in guard.drain(..).enumerate() {
                assert_eq!(i, v);
            }
            sender.send(()).unwrap();
        });
        receiver.recv().unwrap();
        pool.shutdown();
        assert_eq!(cown.last.load(AtomicOrd::Acquire), null_mut());
    }

    indirect_seq {
        let pool = Arc::new(MyThreadPool::with_threads(NonZero::new(4).unwrap()));
        let [a, b, c, d] = [0; 4].map(Cown::new).map(Arc::new);
        let counter = Arc::new(AtomicUsize::new(0));
        let (sender, receiver) = channel();
        pool.when((a.clone(), b.clone()), {
            let counter = counter.clone();
            move |(mut a, mut b)| {
                assert_eq!(*a, 0);
                *a += 1;
                assert_eq!(*b, 0);
                *b += 1;
                assert_eq!(counter.fetch_add(1, AtomicOrd::AcqRel), 0);
            }
        });
        pool.when((b.clone(), c.clone()), {
            let counter = counter.clone();
            move |(mut b, mut c)| {
                assert_eq!(*b, 1);
                *b += 1;
                assert_eq!(*c, 0);
                *c += 1;
                assert_eq!(counter.fetch_add(1, AtomicOrd::AcqRel), 1);
            }
        });
        pool.when((c.clone(), d.clone()), {
            let counter = counter.clone();
            move |(mut c, mut d)| {
                assert_eq!(*c, 1);
                *c += 1;
                assert_eq!(*d, 0);
                *d += 1;
                assert_eq!(counter.fetch_add(1, AtomicOrd::AcqRel), 2);
            }
        });
        pool.when((a, b, c, d), move |(a, b, c, d)| {
            assert_eq!(*a, 1);
            assert_eq!(*b, 2);
            assert_eq!(*c, 2);
            assert_eq!(*d, 1);
            assert_eq!(counter.load(AtomicOrd::Acquire), 3);
            sender.send(()).unwrap();
        });
        receiver.recv().unwrap();
        pool.shutdown();
    }

    triangle {
        let pool = Arc::new(MyThreadPool::with_threads(NonZero::new(4).unwrap()));
        let [top, left, right] = [0; 3].map(Cown::new).map(Arc::new);
        let counter = Arc::new(AtomicUsize::new(0));
        let (sender, receiver) = channel();
        pool.when(top.clone(), {
            let counter = counter.clone();
            move |mut top| {
                assert_eq!(counter.fetch_add(1, AtomicOrd::AcqRel), 0);
                assert_eq!(*top, 0);
                *top += 1;
            }
        });
        pool.when((top.clone(), left.clone()), {
            let counter = counter.clone();
            move |(mut top, mut left)| {
                assert!([1, 2].contains(&counter.fetch_add(1, AtomicOrd::AcqRel)));
                assert!([1, 2].contains(&*top));
                assert_eq!(*left, 0);
                *top += 1;
                *left += 1;
            }
        });
        pool.when((top.clone(), right.clone()), {
            let counter = counter.clone();
            move |(mut top, mut right)| {
                assert!([1, 2].contains(&counter.fetch_add(1, AtomicOrd::AcqRel)));
                assert!([1, 2].contains(&*top));
                assert_eq!(*right, 0);
                *top += 1;
                *right += 1;
            }
        });
        pool.when((left.clone(), right.clone()), {
            let counter = counter.clone();
            move |(mut left, mut right)| {
                assert_eq!(counter.fetch_add(1, AtomicOrd::AcqRel), 3);
                assert_eq!(*left, 1);
                assert_eq!(*right, 1);
                *left += 1;
                *right += 1;
            }
        });
        pool.when((top, right, left), {
            let counter = counter.clone();
            move |(top, left, right)| {
                assert_eq!(counter.load(AtomicOrd::Acquire), 4);
                assert_eq!(*top, 3);
                assert_eq!(*left, 2);
                assert_eq!(*right, 2);
                sender.send(()).unwrap();
            }
        });
        receiver.recv().unwrap();
        pool.shutdown();
    }
}
