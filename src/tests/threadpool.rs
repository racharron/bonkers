use std::sync::Arc;

use threadpool::ThreadPool;

use crate::tests::util;

#[test]
fn when_none_simple() {
    let pool = Arc::new(ThreadPool::new(4));
    util::when_none_simple(pool.clone());
    pool.join();
}

#[test]
fn when_none_loop() {
    let pool = Arc::new(ThreadPool::new(4));
    util::when_none_loop(pool.clone());
    pool.join();
}

#[test]
fn sequential_1() {
    let pool = Arc::new(ThreadPool::new(4));
    util::sequential_1(pool.clone());
    pool.join();
}

#[test]
fn indirect_seq() {
    let pool = Arc::new(ThreadPool::new(4));
    util::indirect_seq(pool.clone());
    pool.join();
}

#[test]
fn triangle() {
    let pool = Arc::new(ThreadPool::new(4));
    util::triangle(pool.clone());
    pool.join();
}
