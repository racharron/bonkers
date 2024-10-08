use crate::SimpleThreadPool;
use std::num::NonZeroUsize;
use std::sync::Arc;

use crate::tests::util;

#[test]
fn when_none_simple() {
    let pool = Arc::new(SimpleThreadPool::with_threads(NonZeroUsize::new(4).unwrap()));
    util::when_none_simple(pool.clone());
    pool.shutdown();
}

#[test]
fn when_none_loop() {
    let pool = Arc::new(SimpleThreadPool::with_threads(NonZeroUsize::new(4).unwrap()));
    util::when_none_loop(pool.clone());
    pool.shutdown();
}

#[test]
fn sequential_1() {
    let pool = Arc::new(SimpleThreadPool::with_threads(NonZeroUsize::new(4).unwrap()));
    util::sequential_1(pool.clone());
    pool.shutdown();
}

#[test]
fn indirect_seq() {
    let pool = Arc::new(SimpleThreadPool::with_threads(NonZeroUsize::new(4).unwrap()));
    util::indirect_seq(pool.clone());
    pool.shutdown();
}

#[test]
fn triangle() {
    let pool = Arc::new(SimpleThreadPool::with_threads(NonZeroUsize::new(4).unwrap()));
    util::triangle(pool.clone());
    pool.shutdown();
}

#[test]
fn recursive_sequential() {
    let pool = Arc::new(SimpleThreadPool::with_threads(NonZeroUsize::new(4).unwrap()));
    util::recursive_sequential(pool.clone(), 10);
    pool.shutdown();
}

#[test]
fn recursive_shuffle() {
    let pool = Arc::new(SimpleThreadPool::with_threads(NonZeroUsize::new(4).unwrap()));
    util::recursive_shuffle(pool.clone(), 3);
    pool.shutdown();
}
