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

/// Test to check that immutable locks can proceed in parallel.
#[test]
fn imm_parallel() {
    use crate::*;
    use std::sync::*;
    let pool = Arc::new(OsThreads::new());
    let cown = Arc::new(Cown::new(()));
    let cvar = Arc::new(Condvar::new());
    let mutex = Arc::new(Mutex::new(false));

    pool.when(Imm(cown.clone()), {
        let cvar = cvar.clone();
        let mutex = mutex.clone();
        move |_| {
            drop(cvar.wait_while(mutex.lock().unwrap(), |done| !*done));
        }
    });
    pool.when(Imm(cown), move |_| {
        *mutex.lock().unwrap() = true;
        cvar.notify_one();
    });
    pool.finish();
}
