use std::sync::Arc;
use rayon_core::ThreadPoolBuilder;
use crate::tests::util;

#[test]
fn when_none_simple() {
    let pool = Arc::new(ThreadPoolBuilder::new().num_threads(4).build().unwrap());
    util::when_none_simple(pool);
}

#[test]
fn when_none_loop() {
    let pool = Arc::new(ThreadPoolBuilder::new().num_threads(4).build().unwrap());
    util::when_none_loop(pool.clone());
}

#[test]
fn sequential_1() {
    let pool = Arc::new(ThreadPoolBuilder::new().num_threads(4).build().unwrap());
    util::sequential_1(pool.clone());
    drop(pool);
}

#[test]
fn indirect_seq() {
    let pool = Arc::new(ThreadPoolBuilder::new().num_threads(4).build().unwrap());
    util::indirect_seq(pool.clone());
}

#[test]
fn triangle() {
    let pool = Arc::new(ThreadPoolBuilder::new().num_threads(4).build().unwrap());
    util::triangle(pool.clone());
}
