//! A pure rust implementation of [Behavior Oriented Concurrency](https://doi.org/10.1145/3622852).
//!
//! Provides two simple threadpool implementations to be used, along with optional support for
//! [rayon](https://crates.io/crates/rayon) and
//! [threadpool](https://crates.io/crates/threadpool), which are activated by features of the same
//! name.
//!
//! ## Example
//! ```rust
//! # mod some { pub mod path { pub use bonkers::OsThreads as SomeThreadPool; } }
//! use some::path::SomeThreadPool;
//! use bonkers::{ThreadPool, Runner, Cown, Mut, Ref};
//!
//! use std::sync::Arc;
//! use std::sync::atomic::{AtomicUsize, Ordering};
//! use std::sync::mpsc::channel;
//!
//!
//! let pool = Arc::new(SomeThreadPool::new());
//! let a = Arc::new(Cown::new(100));
//! let b = Arc::new(Cown::new(200));
//! let c = Arc::new(Cown::new(300));
//! let counter = Arc::new(AtomicUsize::new(0));
//!
//! pool.when(Mut((a.clone(), b.clone())), {
//!     let counter = counter.clone();
//!     move |(mut a, mut b)| {
//!         assert_eq!(counter.fetch_add(1, Ordering::AcqRel), 0);
//!         assert_eq!(*a, 100);
//!         assert_eq!(*b, 200);
//!         *a += 1;
//!         *b += 1;
//!     }
//! });
//! pool.when(Mut((b.clone(), c.clone())), {
//!     let counter = counter.clone();
//!     move |(mut b, mut c)| {
//!         assert_eq!(counter.fetch_add(1, Ordering::AcqRel), 1);
//!         assert_eq!(*b, 201);
//!         assert_eq!(*c, 300);
//!         *b += 1;
//!         *c += 1;
//!     }
//! });
//! let (sender, receiver) = channel();
//! pool.when(Ref((a, b, c)), {
//!     let counter = counter.clone();
//!     move |(a, b, c)| {
//!         assert_eq!(counter.fetch_add(1, Ordering::AcqRel), 2);
//!         assert_eq!(*a, 101);
//!         assert_eq!(*b, 202);
//!         assert_eq!(*c, 301);
//!         sender.send(()).unwrap();
//!     }
//! });
//! receiver.recv().unwrap();
//! assert_eq!(counter.load(Ordering::Acquire), 3);
//! ```

#![deny(missing_docs)]

use std::convert::identity;
use std::ptr::null_mut;
use std::sync::atomic::{AtomicBool, AtomicPtr, AtomicUsize, Ordering as AtomicOrd};
use std::sync::Arc;
use std::thread::yield_now;

#[cfg(test)]
mod tests;

mod thread_pool;
pub use thread_pool::*;
mod cown;
pub use cown::*;

/// Basically taken from `crossbeam`.
struct Backoff {
    limit: u32,
    counter: u32,
}

impl Backoff {
    const DEFAULT_LIMIT: u32 = 7;
    fn new() -> Self {
        Self::with_limit(Self::DEFAULT_LIMIT)
    }
    fn with_limit(limit: u32) -> Self {
        Self { limit, counter: 1 }
    }
    /// True indicates that this function did a spin loop, false indicates that the thread should
    /// yield.
    fn snooze(&mut self) -> bool {
        if self.counter <= self.limit {
            for _ in 0..self.counter {
                std::hint::spin_loop();
            }
            self.counter += 1;
            true
        } else {
            false
        }
    }
}

/// A pointer to a threadpool that can be cloned without lifetime concerns and sent around to other
/// threads.  The classic examples are and `Arc<ThreadPool>` or `&'static ThreadPool`.
pub trait Runner: Clone + Send + Sync + 'static {
    /// Needed to keep the type system happy.
    type ThreadPool: ThreadPool;
    /// Queue a task to be run when the `cown`s are available.  See [`when`] for more details.
    fn when<RC, T>(&self, cowns: RC, thunk: T)
    where
        RC: RequestCollection,
        T: for<'a> FnOnce(RC::Locked<'a>) + Send + Sync + 'static;
    /// Returns a reference to the threadpool.
    fn threadpool(&self) -> &Self::ThreadPool;
}

/// Queue a task to be run on a runner when the `cown`s are available.  Note that the resolution algorithm
/// for scheduling thunks is deliberately simple, so
///
/// ```ignore
/// let pool = SomeThreadPool::new();
/// let [a, b, c, d]: [Arc<Cown<Something>>; 4] = ...;
/// pool.when((a, b), |_| ...); //  Task 1
/// pool.when((b, c), |_| ...); //  Task 2
/// pool.when((c, d), |_| ...); //  Task 3
/// ```
/// will have all three tasks run in sequence, even though the required `cown`s of tasks 1 and 3 do not overlap.
pub fn when<R: Runner, RC: RequestCollection, T: for<'a> FnOnce(RC::Locked<'a>) + Send + Sync + 'static>(runner: &R, cowns: RC, thunk: T) {
    let mut cown_vec = Vec::from_iter(cowns.cown_bases());
    cown_vec.sort_unstable_by_key(|&cbr| cbr as *const _ as *const () as usize);
    cown_vec
        .windows(2)
        .for_each(|cbs| assert_ne!(cbs[0] as *const _ as *const () as usize, cbs[1] as *const _ as *const () as usize));
    let requests = Vec::from_iter(cown_vec.iter().cloned().map(Request::new));
    let behavior = Box::into_raw(Behavior::new(requests, (cowns, Some(thunk))));
    unsafe {
        for i in 0..(*behavior).requests.len() {
            Request::start_appending((*behavior).requests.as_ptr().add(i), behavior, runner);
        }
        for request in &(*behavior).requests {
            request.finish_appending();
        }
    }
    Behavior::resolve_one(behavior, runner);
}

impl<TP: ThreadPool + Sync> Runner for &'static TP {
    type ThreadPool = TP;

    fn when<RC: RequestCollection, T: for<'a> FnOnce(RC::Locked<'a>) + Send + Sync + 'static>(&self, cowns: RC, thunk: T) {
        when(self, cowns, thunk)
    }

    fn threadpool(&self) -> &Self::ThreadPool {
        self
    }
}

impl<TP: ThreadPool + Sync + Send + 'static> Runner for Arc<TP> {
    type ThreadPool = TP;

    fn when<RC: RequestCollection, T: for<'a> FnOnce(RC::Locked<'a>) + Send + Sync + 'static>(&self, cowns: RC, thunk: T) {
        when(self, cowns, thunk)
    }

    fn threadpool(&self) -> &Self::ThreadPool {
        self
    }
}

/// Null pointer.
const NULL: *mut Request = 0 as *mut _;

/// In practice, this address will never be used by correct code (not in the least because it
/// (should) not be aligned.
///
/// TODO: check provenance (`offset` a no-provenance pointer is legal, but is `byte_offset`)?
const POISONED: *mut Request = 1 as *mut _;

struct Request {
    next: AtomicPtr<Behavior>,
    scheduled: AtomicBool,
    target: *const AtomicPtr<Request>,
}

unsafe impl Send for Request {}
unsafe impl Sync for Request {}

impl Request {
    pub fn new(target: *const AtomicPtr<Request>) -> Self {
        Request {
            next: AtomicPtr::new(null_mut()),
            scheduled: AtomicBool::new(false),
            target,
        }
    }
    pub fn finish_appending(&self) {
        self.scheduled.store(true, AtomicOrd::Release);
    }
    pub fn start_appending<R: Runner>(this: *const Self, behavior: *mut Behavior, runner: &R) {
        unsafe {
            let prev = (*(*this).target).swap(this as *mut _, AtomicOrd::AcqRel);
            match prev {
                NULL => Behavior::resolve_one(behavior, runner),
                POISONED => todo!("poisoned!"),
                prev => {
                    let mut backoff = Backoff::new();
                    while !(*prev).scheduled.load(AtomicOrd::Acquire) {
                        if !backoff.snooze() {
                            yield_now();
                        }
                    }
                    (*prev).next.store(behavior, AtomicOrd::Release);
                }
            }
        }
    }
    pub fn release<R: Runner>(&self, runner: &R) {
        unsafe {
            let mut next = self.next.load(AtomicOrd::Acquire);
            if next.is_null() {
                let latest = &*self.target;
                let old = latest
                    .compare_exchange(self as *const _ as *mut _, NULL, AtomicOrd::SeqCst, AtomicOrd::Acquire)
                    .unwrap_or_else(identity);
                if std::ptr::eq(old as *const _, self as *const _) {
                    return;
                }
                let mut backoff = Backoff::new();
                loop {
                    next = self.next.load(AtomicOrd::Acquire);
                    if next.is_null() {
                        if !backoff.snooze() {
                            yield_now();
                        }
                    } else {
                        break;
                    }
                }
            }
            Behavior::resolve_one(next, runner)
        }
    }
}

trait Thunk: Send + Sync + Send + 'static {
    unsafe fn consume_boxed_and_release(&mut self);
}

impl<C, F> Thunk for (C, Option<F>)
where
    C: RequestCollection,
    F: for<'a> FnOnce(C::Locked<'a>) + Send + Sync + 'static,
{
    unsafe fn consume_boxed_and_release(&mut self) {
        self.1.take().unwrap()(self.0.locked());
    }
}

/// Contains information about queued behavior.  Since it is stored in a directed acyclic graph,
/// weak reference counts are not needed.
struct Behavior {
    thunk: Box<dyn Thunk>,
    count: AtomicUsize,
    requests: Vec<Request>,
}

impl Behavior {
    pub fn new<T: Thunk>(requests: Vec<Request>, thunk: T) -> Box<Self> {
        Box::new(Behavior {
            thunk: Box::new(thunk),
            count: AtomicUsize::new(requests.len() + 1),
            requests,
        })
    }
    pub fn resolve_one<R: Runner>(this: *mut Self, runner: &R) {
        unsafe {
            let behavior = &*this;
            if behavior.count.fetch_sub(1, AtomicOrd::AcqRel) == 1 {
                let mut behavior = Box::from_raw(this);
                runner.threadpool().run({
                    let runner = runner.clone();
                    move || {
                        behavior.thunk.consume_boxed_and_release();
                        for request in &behavior.requests {
                            request.release(&runner);
                        }
                    }
                });
            }
        }
    }
}
