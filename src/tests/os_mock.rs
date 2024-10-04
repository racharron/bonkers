use crate::OsThreads;
use std::sync::Arc;

use crate::tests::util;

#[test]
fn when_none_simple() {
    let pool = Arc::new(OsThreads::new());
    util::when_none_simple(pool.clone());
    pool.finish();
}

#[test]
fn when_none_loop() {
    let pool = Arc::new(OsThreads::new());
    util::when_none_loop(pool.clone());
    pool.finish();
}

#[test]
fn sequential_1() {
    let pool = Arc::new(OsThreads::new());
    util::sequential_1(pool.clone());
    pool.finish();
}

#[test]
fn indirect_seq() {
    let pool = Arc::new(OsThreads::new());
    util::indirect_seq(pool.clone());
    pool.finish();
}

#[test]
fn triangle() {
    let pool = Arc::new(OsThreads::new());
    util::triangle(pool.clone());
    pool.finish();
}

#[test]
fn recursive_sequential() {
    let pool = Arc::new(OsThreads::new());
    util::recursive_sequential(pool.clone(), 10);
    pool.finish();
}

#[test]
fn recursive_shuffle() {
    let pool = Arc::new(OsThreads::new());
    util::recursive_shuffle(pool.clone(), 3);
    pool.finish();
}
