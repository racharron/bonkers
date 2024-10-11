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
//! use bonkers::{Cown, ThreadPool, Runner, Mut, Imm};
//! use some::path::SomeThreadPool;
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
//! pool.when((Mut(b.clone()), Mut(c.clone())), {
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
//! pool.when(Imm((a, b, c)), {
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

use erasable::{Erasable, ErasedPtr};
use slice_dst::SliceWithHeader;
use std::collections::VecDeque;
use std::convert::identity;
use std::ptr::{addr_of_mut, null_mut, NonNull};
use std::sync::atomic::{AtomicBool, AtomicPtr, AtomicUsize, Ordering as AtomicOrd};
use std::sync::Arc;
use std::thread::yield_now;

#[cfg(test)]
mod tests;

mod thread_pool;
pub use thread_pool::*;
mod cown;
pub use cown::*;

mod lock;
pub use lock::*;
mod macro_impls;

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
    fn when<L, T>(&self, cowns: L, thunk: T)
    where
        L: LockCollection,
        T: for<'a> FnOnce(L::Ref<'a>) + Send + Sync + 'static;
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
pub fn when<R: Runner, L: LockCollection, T: for<'a> FnOnce(L::Ref<'a>) + Send + Sync + 'static>(runner: &R, cowns: L, thunk: T) {
    let mut infos = Vec::from_iter(cowns.infos());
    infos.sort_unstable_by(|a, b| a.cown.cmp(&b.cown));
    infos.windows(2).for_each(|adjacent| assert_ne!(adjacent[0].cown, adjacent[1].cown));
    let size = infos.len();
    let behavior = Box::into_raw(Behavior::new(
        BehaviorHeader {
            thunk: Box::new((cowns, Some(Box::new(thunk)))),
            count: AtomicUsize::new(size + 1),
        },
        infos.into_iter().map(|info| Request::new(info.cown.last, info.read_only)),
    ));
    unsafe {
        let requests_base = addr_of_mut!((*behavior).slice) as *mut Request;
        let behavior = NonNull::new_unchecked(behavior);
        for i in 0..size {
            Request::start_appending(requests_base.add(i), behavior, runner);
        }
        for i in 0..size {
            Request::finish_appending(requests_base.add(i));
        }
        resolve_one(behavior, runner);
    }
}

impl<TP: ThreadPool + Sync> Runner for &'static TP {
    type ThreadPool = TP;

    fn when<L: LockCollection, T: for<'a> FnOnce(L::Ref<'a>) + Send + Sync + 'static>(&self, cowns: L, thunk: T) {
        when(self, cowns, thunk)
    }

    fn threadpool(&self) -> &Self::ThreadPool {
        self
    }
}

impl<TP: ThreadPool + Sync + Send + 'static> Runner for Arc<TP> {
    type ThreadPool = TP;

    fn when<L: LockCollection, T: for<'a> FnOnce(L::Ref<'a>) + Send + Sync + 'static>(&self, cowns: L, thunk: T) {
        when(self, cowns, thunk)
    }

    fn threadpool(&self) -> &Self::ThreadPool {
        &*self
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
    next_behavior: AtomicPtr<()>,
    next_request: AtomicPtr<Request>,
    scheduled: AtomicBool,
    read_only: bool,
    released: bool,
    target: *const AtomicPtr<Request>,
}

unsafe impl Send for Request {}
unsafe impl Sync for Request {}

impl Request {
    pub fn new(target: *const AtomicPtr<Request>, read_only: bool) -> Self {
        Request {
            next_behavior: AtomicPtr::new(null_mut()),
            next_request: AtomicPtr::new(null_mut()),
            scheduled: AtomicBool::new(false),
            read_only,
            released: false,
            target,
        }
    }
    pub fn finish_appending(this: *mut Self) {
        unsafe {
            (*this).scheduled.store(true, AtomicOrd::Release);
        }
    }
    pub fn start_appending<R: Runner>(this: *mut Self, behavior: NonNull<Behavior>, runner: &R) {
        unsafe {
            let prev = (*(*this).target).swap(this, AtomicOrd::AcqRel);
            match prev {
                NULL => resolve_one(behavior, runner),
                POISONED => todo!("poisoned!"),
                prev => {
                    (*prev).next_request.store(this, AtomicOrd::Release);
                    let mut backoff = Backoff::new();
                    while !(*prev).scheduled.load(AtomicOrd::Acquire) {
                        if !backoff.snooze() {
                            yield_now();
                        }
                    }
                    (*prev).next_behavior.store(behavior.as_ptr() as *mut _, AtomicOrd::Release);
                }
            }
        }
    }
    pub fn release<R: Runner>(this: *mut Self, runner: &R) {
        unsafe {
            if !(*this).read_only && !(*this).released {
                let mut current = this;
                loop {
                    if let Some(next_behavior) = NonNull::new((*current).next_behavior.load(AtomicOrd::Acquire) as *mut _) {
                        let next_behavior = Behavior::unerase(next_behavior);
                        (*current).released = true;
                        resolve_one(next_behavior, runner);
                        if (*current).read_only {
                            current = (*current).next_request.load(AtomicOrd::Acquire);
                        } else {
                            return;
                        }
                    } else {
                        let old = (*(*this).target)
                            .compare_exchange(current, NULL, AtomicOrd::SeqCst, AtomicOrd::Acquire)
                            .unwrap_or_else(identity);
                        if std::ptr::addr_eq(old, current) {
                            return;
                        }
                        let mut backoff = Backoff::new();
                        loop {
                            if let Some(new) = NonNull::new((*current).next_behavior.load(AtomicOrd::Acquire) as *mut _) {
                                let next_behavior = Behavior::unerase(new);
                                resolve_one(next_behavior, runner);
                                return;
                            } else if !backoff.snooze() {
                                yield_now();
                            }
                        }
                    }
                }
            }
        }
    }
}

trait Thunk: Send + Sync + Send + 'static {
    unsafe fn consume_boxed_and_release(&mut self);
}

impl<L, F> Thunk for (L, Option<F>)
where
    L: LockCollection,
    F: for<'a> FnOnce(L::Ref<'a>) + Send + Sync + 'static,
{
    unsafe fn consume_boxed_and_release(&mut self) {
        self.1.take().unwrap()(self.0.get_ref());
    }
}

/// Contains information about queued behavior.  Since it is stored in a directed acyclic graph,
/// weak reference counts are not needed.
type Behavior = SliceWithHeader<BehaviorHeader, Request>;

struct BehaviorHeader {
    thunk: Box<dyn Thunk>,
    count: AtomicUsize,
}

fn resolve_one<R: Runner>(this: NonNull<Behavior>, runner: &R) {
    unsafe {
        if (*this.as_ptr()).header.count.fetch_sub(1, AtomicOrd::AcqRel) == 1 {
            struct BehaviorOwner(ErasedPtr);
            unsafe impl Send for BehaviorOwner {}
            unsafe impl Sync for BehaviorOwner {}
            impl Drop for BehaviorOwner {
                fn drop(&mut self) {
                    unsafe {
                        let _ = Box::from_raw(Behavior::unerase(self.0).as_ptr());
                    }
                }
            }
            let behavior = BehaviorOwner(Behavior::erase(this));
            runner.threadpool().run({
                let runner = runner.clone();
                move || {
                    let _ = &behavior;
                    let behavior = Behavior::unerase(behavior.0);
                    (*behavior.as_ptr()).header.thunk.consume_boxed_and_release();
                    let requests_base = addr_of_mut!((*behavior.as_ptr()).slice) as *mut Request;
                    let size = (*behavior.as_ptr()).slice.len();
                    for i in 0..size {
                        Request::release(requests_base.add(i), &runner);
                    }
                }
            });
        }
    }
}
