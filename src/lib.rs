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
use std::ptr::{addr_of_mut, null_mut, NonNull};
use std::sync::atomic::{AtomicUsize, Ordering as AtomicOrd};
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
use request::Request;

mod macro_impls;
mod request;

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
pub fn when<R, L, T>(runner: &R, cowns: L, thunk: T)
where
    R: Runner,
    L: LockCollection,
    T: for<'a> FnOnce(L::Ref<'a>) + Send + Sync + 'static,
{
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
        let behavior = NonNull::new(behavior).unwrap();
        for i in 0..size {
            Request::start_appending(requests_base.add(i), behavior, runner);
        }
        for i in 0..size {
            Request::finish_appending(requests_base.add(i), behavior);
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

impl Request {
    pub fn finish_appending(this: *mut Self, behavior: NonNull<Behavior>) {
        unsafe {
            (*this).state.store(Request::SCHEDULED, AtomicOrd::Release);
        }
    }
    pub fn start_appending<R: Runner>(this: *mut Self, behavior: NonNull<Behavior>, runner: &R) {
        unsafe {
            let prev = (*(*this).base).last.swap(this, AtomicOrd::AcqRel);
            match prev {
                request::NULL => {
                    (*(*this).base).last_mut.store(this, AtomicOrd::Release);
                    resolve_one(behavior, runner);
                }
                request::POISONED => todo!("poisoned!"),
                prev => {
                    (*this).prev_request.store(prev, AtomicOrd::Release);
                    let mut backoff = Backoff::new();
                    while (*prev).state.load(AtomicOrd::Acquire) == Request::UNSCHEDULED {
                        if !backoff.snooze() {
                            yield_now();
                        }
                    }
                    (*prev).next_behavior.store(behavior.as_ptr() as *mut _, AtomicOrd::Acquire);
                    if !(*this).read_only {
                        let old_mut = (*(*this).base).last_mut.swap(this, AtomicOrd::AcqRel);
                        if old_mut.is_null() {
                            let mut current = prev;
                            let mut count = 0;
                            while !current.is_null() && (*current).read_only {
                                if (*current).state
                                    .compare_exchange(
                                        Request::SCHEDULED,
                                        Self::HAS_NEXT_MUT,
                                        AtomicOrd::AcqRel,
                                        AtomicOrd::Acquire
                                    ).is_ok()
                                {
                                    count += 1;
                                    current = (*current).prev_request.swap(null_mut(), AtomicOrd::Acquire);
                                } else {

                                }
                            }
                            (*behavior.as_ptr()).header.count.fetch_add(count, AtomicOrd::AcqRel);
                        }
                    }
                }
            }
        }
    }
    pub fn release<R: Runner>(this: *mut Self, runner: &R) {
        unsafe {
            if let Some(next_behavior)
                = NonNull::new((*this).next_behavior.load(AtomicOrd::Acquire) as *mut _).map(Behavior::unerase)
            {
                Self::release_request(this, next_behavior, runner);
            } else {
                if (*(*this).base)
                    .last
                    .compare_exchange(this, request::NULL, AtomicOrd::SeqCst, AtomicOrd::Acquire)
                    .is_ok()
                {
                    return;
                }
                let mut backoff = Backoff::new();
                loop {
                    if let Some(next_behavior)
                        = NonNull::new((*this).next_behavior.load(AtomicOrd::Acquire) as *mut _).map(Behavior::unerase)
                    {
                        Self::release_request(this, next_behavior, runner);
                        break;
                    } else if !backoff.snooze() {
                        yield_now();
                    }
                }
            }
        }
    }

    unsafe fn release_request<R: Runner>(this: *mut Request, mut next_behavior: NonNull<Behavior>, runner: &R) {
        if (*this).read_only {
            match (*this).state.compare_exchange(Self::SCHEDULED, Self::RELEASED, AtomicOrd::AcqRel, AtomicOrd::Acquire) {
                Ok(_)   =>  {
                    let prev = (*this).prev_request.load(AtomicOrd::Acquire);
                    let next = (*this).next_request.load(AtomicOrd::Acquire);
                    (*prev).next_behavior.store((*this).next_behavior.load(AtomicOrd::Acquire), AtomicOrd::Release);
                    (*prev).prev_request.store(next, AtomicOrd::Release);
                    (*next).prev_request.store(prev, AtomicOrd::Release);
                }
                Err(Self::HAS_NEXT_MUT) =>  {
                    let mut next_mut = (*this).next_request.load(AtomicOrd::Acquire);
                    let mut backoff = Backoff::new();
                    while (*next_mut).read_only {
                        if !backoff.snooze() { yield_now() }
                        next_mut = (*this).next_request.load(AtomicOrd::Acquire);
                    }
                    resolve_one(
                        Behavior::unerase(NonNull::new((*this).next_behavior.load(AtomicOrd::Acquire) as _).unwrap()),
                        runner
                    );
                }
                Err(_)  =>  unreachable!("invalid state"),
            }
            let other_request = (*this).data.other_request.load(AtomicOrd::Acquire);
            if other_request.is_null() {
                (*this).state.set_next_behavior(null_mut());
                let latest_other_request = (*this).data.other_request.load(AtomicOrd::Acquire);
                if !latest_other_request.is_null() {
                    Self::release_read_only(this, runner, latest_other_request);
                }
            } else {
                Self::release_read_only(this, runner, other_request);
            }
        } else {
            if let Some(mut next_request) = NonNull::new((*this).prev_request.load(AtomicOrd::Acquire)) {
                if (*next_request.as_ptr()).lock.read_only() {
                    loop {
                        resolve_one(next_behavior, runner);
                        let Some(next) = (*next_request.as_ptr()).state.next_behavior() else { return };
                        next_behavior = next;
                        next_request = NonNull::new((*next_request.as_ptr()).prev_request.load(AtomicOrd::Acquire)).unwrap();
                        if !(*next_request.as_ptr()).lock.read_only() {
                            return
                        }
                    }
                } else {
                    resolve_one(next_behavior, runner);
                }
            } else {
                unreachable!()
            }
        }
    }

    unsafe fn release_read_only<R: Runner>(this: *mut Request, runner: &R, other_request: *mut Request) {
        println!("release_read_only {:p}", this);
        let next = (*this).prev_request.load(AtomicOrd::Acquire);
        if next.is_null() {
            unreachable!() // Earlier, we checked (*this).state.next_behavior()
        } else {
            (*next).data.prev.store(other_request, AtomicOrd::Release);
            (*other_request).prev_request.store(next, AtomicOrd::Release);
            let next_mut = (*this).data.next_mut.load(AtomicOrd::Acquire);
            let mut backoff = Backoff::new();
            loop {
                let imm_count = (*next_mut).data.imm_count.load(AtomicOrd::Acquire);
                if imm_count != 0 {
                    if (*next_mut).data.imm_count.fetch_sub(1, AtomicOrd::AcqRel) == 2 {
                        resolve_one((*this).state.next_behavior().unwrap(), runner);
                    }
                    break;
                } else if !backoff.snooze() {
                    yield_now()
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

type ErasedBehavior = usize;

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
