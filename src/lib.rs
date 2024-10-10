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
//! use bonkers::ThreadPool;
//! use some::path::SomeThreadPool;
//! use bonkers::{Cown, Runner};
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
//! pool.when((a.clone(), b.clone()), {
//!     let counter = counter.clone();
//!     move |(mut a, mut b)| {
//!         assert_eq!(counter.fetch_add(1, Ordering::AcqRel), 0);
//!         assert_eq!(*a, 100);
//!         assert_eq!(*b, 200);
//!         *a += 1;
//!         *b += 1;
//!     }
//! });
//! pool.when((b.clone(), c.clone()), {
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
//! pool.when((a, b, c), {
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
use std::cell::UnsafeCell;
use std::cmp::Ordering;
use std::collections::{LinkedList, VecDeque};
use std::convert::identity;
use std::iter::{empty, once};
use std::ptr::{addr_of_mut, null_mut, slice_from_raw_parts_mut, NonNull};
use std::sync::atomic::{AtomicBool, AtomicPtr, AtomicUsize, Ordering as AtomicOrd};
use std::sync::{Arc, LockResult};
use std::thread::yield_now;

#[cfg(test)]
mod tests;

mod thread_pool;
pub use thread_pool::*;

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
    fn when<CC, T>(&self, cowns: CC, thunk: T)
    where
        CC: CownCollection,
        T: for<'a> FnOnce(CC::Guard<'a>) + Send + Sync + 'static;
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
pub fn when<R: Runner, CC: CownCollection, T: for<'a> FnOnce(CC::Guard<'a>) + Send + Sync + 'static>(runner: &R, cowns: CC, thunk: T) {
    let mut infos = Vec::from_iter(cowns.infos());
    infos.sort_unstable();
    infos.windows(2).for_each(|adjacent| assert_ne!(adjacent[0], adjacent[1]));
    let size = infos.len();
    let behavior = Box::into_raw(Behavior::new(
        BehaviorHeader {
            thunk: Box::new((cowns, Some(Box::new(thunk)))),
            count: AtomicUsize::new(size + 1),
        },
        infos.into_iter().map(|info| Request::new(info.last)),
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

    fn when<CC: CownCollection, T: for<'a> FnOnce(CC::Guard<'a>) + Send + Sync + 'static>(&self, cowns: CC, thunk: T) {
        when(self, cowns, thunk)
    }

    fn threadpool(&self) -> &Self::ThreadPool {
        self
    }
}

impl<TP: ThreadPool + Sync + Send + 'static> Runner for Arc<TP> {
    type ThreadPool = TP;

    fn when<CC: CownCollection, T: for<'a> FnOnce(CC::Guard<'a>) + Send + Sync + 'static>(&self, cowns: CC, thunk: T) {
        when(self, cowns, thunk)
    }

    fn threadpool(&self) -> &Self::ThreadPool {
        &*self
    }
}

/// A mutex where multiple mutexes can be locked in parallel.
pub struct Cown<T> {
    last: AtomicPtr<Request>,
    data: UnsafeCell<T>,
}

/// Information about the location at which a [`Cown`] is stored.
#[derive(Debug)]
pub struct CownInfo {
    last: *const AtomicPtr<Request>,
}

impl PartialEq for CownInfo {
    fn eq(&self, other: &Self) -> bool {
        self.last.eq(&other.last)
    }
}
impl Eq for CownInfo {}
impl PartialOrd for CownInfo {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        (self.last as usize).partial_cmp(&(other.last as usize))
    }
}
impl Ord for CownInfo {
    fn cmp(&self, other: &Self) -> Ordering {
        (self.last as usize).cmp(&(other.last as usize))
    }
}

/// Null pointer.
const NULL: *mut Request = 0 as *mut _;

/// In practice, this address will never be used by correct code (not in the least because it
/// (should) not be aligned.
///
/// TODO: check provenance (`offset` a no-provenance pointer is legal, but is `byte_offset`)?
const POISONED: *mut Request = 1 as *mut _;

trait CownBase {
    #[doc(hidden)]
    #[allow(private_interfaces)]
    fn info(&self) -> CownInfo;
}

/// Needed to work around an issue with Rust's trait system.  Effectively part of [`CownCollection`].
pub trait CownCollectionSuper: Send + Sync + 'static {
    /// The guard protecting the internal mutex(es).
    type Guard<'a>;
}

/// Indicates that a type is a collection of [`Cown`]s and can be locked together.
pub trait CownCollection: CownCollectionSuper {
    /// Get the locks of the cowns in the collection.  Structurally mirrors the collection.
    ///
    /// Does not check if the [`Cown`]s are in use.
    unsafe fn get_mut(&self) -> Self::Guard<'_>;
    #[doc(hidden)]
    #[allow(private_interfaces)]
    fn infos(&self) -> impl IntoIterator<Item = CownInfo>;
}

struct Request {
    next: AtomicPtr<()>,
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
                    let mut backoff = Backoff::new();
                    while !(*prev).scheduled.load(AtomicOrd::Acquire) {
                        if !backoff.snooze() {
                            yield_now();
                        }
                    }
                    (*prev).next.store(behavior.as_ptr() as *mut _, AtomicOrd::Release);
                }
            }
        }
    }
    pub fn release<R: Runner>(this: *mut Self, runner: &R) {
        unsafe {
            let next = (*this).next.load(AtomicOrd::Acquire);
            let mut next = if next.is_null() {
                slice_from_raw_parts_mut(null_mut::<()>(), 0) as *mut _
            } else {
                Behavior::unerase(NonNull::new_unchecked(next as *mut _)).as_ptr()
            };
            if next.is_null() {
                let old = (*(*this).target)
                    .compare_exchange(this, NULL, AtomicOrd::SeqCst, AtomicOrd::Acquire)
                    .unwrap_or_else(identity);
                if std::ptr::addr_eq(old, this) {
                    return;
                }
                let mut backoff = Backoff::new();
                loop {
                    let new = (*this).next.load(AtomicOrd::Acquire);
                    next = if new.is_null() {
                        slice_from_raw_parts_mut(null_mut::<()>(), 0) as *mut _
                    } else {
                        Behavior::unerase(NonNull::new_unchecked(new as *mut _)).as_ptr()
                    };
                    if next.is_null() {
                        if !backoff.snooze() {
                            yield_now();
                        }
                    } else {
                        break;
                    }
                }
            }
            resolve_one(NonNull::new_unchecked(next), runner)
        }
    }
}

trait Thunk: Send + Sync + Send + 'static {
    unsafe fn consume_boxed_and_release(&mut self);
}

impl<C, F> Thunk for (C, Option<F>)
where
    C: CownCollection,
    F: for<'a> FnOnce(C::Guard<'a>) + Send + Sync + 'static,
{
    unsafe fn consume_boxed_and_release(&mut self) {
        self.1.take().unwrap()(self.0.get_mut());
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

unsafe impl<T: Send> Send for Cown<T> {}
unsafe impl<T: Sync> Sync for Cown<T> {}
// impl<T> UnwindSafe for Cown<T> {}

impl<T> Cown<T> {
    /// Create a new `cown`.
    pub const fn new(value: T) -> Self {
        Cown {
            last: AtomicPtr::new(null_mut()),
            data: UnsafeCell::new(value),
        }
    }
    /// Extracts the inner data from the `cown`.
    pub fn into_inner(self) -> LockResult<T> {
        Ok(self.data.into_inner())
    }
    /// Returns a mutable reference to the underlying data.  In practice, forwarded to the internal
    /// mutex.
    pub fn get_mut(&mut self) -> LockResult<&mut T> {
        Ok(self.data.get_mut())
    }
}

impl<T: Send + Sync + 'static> CownBase for Cown<T> {
    #[doc(hidden)]
    #[allow(private_interfaces)]
    fn info(&self) -> CownInfo {
        CownInfo { last: &self.last }
    }
}

macro_rules! ref_cown_collection {
    ( $v:ident => $( $t:ty ),+ )  =>  {
        $(
            impl<$v: Send + Sync + 'static> CownCollectionSuper for $t {
                type Guard<'a> = &'a mut T;
            }
            impl<$v: Send + Sync + 'static> CownCollection for $t {
                unsafe fn get_mut(&self) -> Self::Guard<'_> {
                    &mut *UnsafeCell::raw_get(&self.data)
                }

                #[doc(hidden)]
                #[allow(private_interfaces)]
                fn infos<'a>(&'a self) -> impl IntoIterator<Item=CownInfo> {
                    once(self.info())
                }
            }
        )+
    };
}

ref_cown_collection! {
    T => &'static Cown<T>, Arc<Cown<T>>
}

macro_rules! collection_cown_collection {
    ( $v:ident => $( $t:ident ),+ ) => {
        $(
            impl<$v: CownCollectionSuper> CownCollectionSuper for $t<$v> {
                type Guard<'a> = $t<$v::Guard<'a>>;
            }
            impl<$v: CownCollection> CownCollection for $t<$v> {
                unsafe fn get_mut(&self) -> Self::Guard<'_> {
                    self.into_iter().map(|cc| unsafe { cc.get_mut() }).collect()
                }

                #[doc(hidden)]
                #[allow(private_interfaces)]
                fn infos(&self) -> impl IntoIterator<Item=CownInfo> {
                    self.into_iter().flat_map($v::infos)
                }
            }
        )+
    };
}

collection_cown_collection! {
    T => Vec, VecDeque, LinkedList
}

impl<T: CownCollectionSuper, const N: usize> CownCollectionSuper for [T; N] {
    type Guard<'a> = [T::Guard<'a>; N];
}
impl<T: CownCollection, const N: usize> CownCollection for [T; N] {
    unsafe fn get_mut(&self) -> Self::Guard<'_> {
        self.each_ref().map(|cc| unsafe { cc.get_mut() })
    }
    #[doc(hidden)]
    #[allow(private_interfaces)]
    fn infos(&self) -> impl IntoIterator<Item = CownInfo> {
        self.iter().flat_map(T::infos)
    }
}

macro_rules! variadic_cown_collection {
    ($($v:ident)*)   =>  {
        impl<$($v: CownCollectionSuper,)*> CownCollectionSuper for ($($v,)*) {
            type Guard<'a> = ($($v::Guard<'a>,)*);
        }
        impl<$($v: CownCollection,)*> CownCollection for ($($v,)*) {
            unsafe fn get_mut(&self) -> Self::Guard<'_> {
                #![allow(non_snake_case)]
                let ($(ref $v,)*) = self;
                #[allow(clippy::unused_unit)]
                ($($v.get_mut(),)*)
            }
            #[doc(hidden)]
            #[allow(private_interfaces)]
            fn infos(&self) -> impl IntoIterator<Item=CownInfo> {
                #![allow(non_snake_case)]
                let ($(ref $v,)*) = self;
                empty() $(.chain($v.infos()))*
            }
        }
    }
}

variadic_cown_collection! {}
variadic_cown_collection! { A }
variadic_cown_collection! { A B }
variadic_cown_collection! { A B C }
variadic_cown_collection! { A B C D }
variadic_cown_collection! { A B C D E }
variadic_cown_collection! { A B C D E F }
variadic_cown_collection! { A B C D E F G }
variadic_cown_collection! { A B C D E F G H }
variadic_cown_collection! { A B C D E F G H I }
variadic_cown_collection! { A B C D E F G H I J }
variadic_cown_collection! { A B C D E F G H I J K }
variadic_cown_collection! { A B C D E F G H I J K L }
variadic_cown_collection! { A B C D E F G H I J K L M }
variadic_cown_collection! { A B C D E F G H I J K L M N }
variadic_cown_collection! { A B C D E F G H I J K L M N O }
variadic_cown_collection! { A B C D E F G H I J K L M N O P }
variadic_cown_collection! { A B C D E F G H I J K L M N O P Q }
variadic_cown_collection! { A B C D E F G H I J K L M N O P Q R }
variadic_cown_collection! { A B C D E F G H I J K L M N O P Q R S }
variadic_cown_collection! { A B C D E F G H I J K L M N O P Q R S T }
variadic_cown_collection! { A B C D E F G H I J K L M N O P Q R S T U }
variadic_cown_collection! { A B C D E F G H I J K L M N O P Q R S T U V }
variadic_cown_collection! { A B C D E F G H I J K L M N O P Q R S T U V W }
variadic_cown_collection! { A B C D E F G H I J K L M N O P Q R S T U V W X }
variadic_cown_collection! { A B C D E F G H I J K L M N O P Q R S T U V W X Y }
variadic_cown_collection! { A B C D E F G H I J K L M N O P Q R S T U V W X Y Z }
