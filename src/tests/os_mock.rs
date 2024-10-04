use std::sync::Arc;
use crate::OsThreads;

use crate::tests::util;

#[test]
fn when_none_simple() {
    let pool = Arc::new(OsThreads::new());
    util::when_none_simple(pool.clone());
    pool.join();
}

#[test]
fn when_none_loop() {
    let pool = Arc::new(OsThreads::new());
    util::when_none_loop(pool.clone());
    pool.join();
}

#[test]
fn sequential_1() {
    let pool = Arc::new(OsThreads::new());
    util::sequential_1(pool.clone());
    pool.join();
}

#[test]
fn indirect_seq() {
    let pool = Arc::new(OsThreads::new());
    util::indirect_seq(pool.clone());
    pool.join();
}

#[test]
fn triangle() {
    let pool = Arc::new(OsThreads::new());
    util::triangle(pool.clone());
    pool.join();
}
