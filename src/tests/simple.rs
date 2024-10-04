use std::num::NonZeroUsize;
use std::sync::Arc;
use crate::SimpleThreadPool;

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

}
