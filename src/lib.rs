//! A pure rust implementation of [Behavior Oriented Concurrency](https://doi.org/10.1145/3622852).

use std::convert::identity;
use std::iter::{empty, once};
use std::ops::Deref;
use std::ptr::null_mut;

pub use std::sync::atomic::{AtomicBool, AtomicIsize, AtomicPtr, AtomicUsize, Ordering as AtomicOrd};
pub use std::sync::{Arc, LockResult, Mutex, MutexGuard};
pub use std::thread::{current, park, yield_now, Builder, JoinHandle, Thread};
pub use std::sync::mpsc::channel;

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
pub trait Runner: Deref<Target = Self::ThreadPool> + Clone + Send + Sync + 'static {
    type ThreadPool: ThreadPool;
    fn when<CC, T>(&self, cowns: CC, thunk: T)
    where CC: CownCollection, T: for<'a> FnOnce(CC::Guard<'a>) + Send + Sync + 'static, Self: 'static;
}

impl<TP: ThreadPool, P: Deref<Target = TP> + Clone + Send + Sync + 'static> Runner for P {
    type ThreadPool = TP;

    fn when<CC: CownCollection, T: for<'a> FnOnce(CC::Guard<'a>) + Send + Sync + 'static>(
        &self,
        cowns: CC,
        thunk: T,
    ) where
        Self: 'static,
    {
        let mut cown_vec = Vec::from_iter(cowns.cown_bases());
        cown_vec.sort_unstable_by_key(|&cbr| cbr as *const _ as *const () as usize);
        cown_vec.windows(2).for_each(|cbs| {
            assert_ne!(
                cbs[0] as *const _ as *const () as usize,
                cbs[1] as *const _ as *const () as usize
            )
        });
        let requests = Vec::from_iter(cown_vec.iter().cloned().map(Request::new));
        let behavior = Box::into_raw(Behavior::new(requests, (cowns, Some(thunk))));
        unsafe {
            for i in 0..(*behavior).requests.len() {
                Request::start_appending((*behavior).requests.as_ptr().add(i), behavior, self);
            }
            for request in &(*behavior).requests {
                request.finish_appending();
            }
        }
        Behavior::resolve_one(behavior, self);
    }
}

/// A mutex where multiple mutexes can be locked in parallel.
pub struct Cown<T> {
    last: AtomicPtr<Request>,
    data: Mutex<T>,
}

/// Null pointer.
const NULL: *mut Request = 0 as *mut _;

/// In practice, this address will never be used by correct code (not in the least because it
/// (should) not be aligned.
///
/// TODO: check provenance (`offset` a no-provenance pointer is legal, but is `byte_offset`)?
const POISONED: *mut Request = 1 as *mut _;

pub trait CownBase {
    #[doc(hidden)]
    #[allow(private_interfaces)]
    fn last(&self) -> *const AtomicPtr<Request>;
}

pub trait CownCollectionSuper: Send + Sync + 'static {
    type Guard<'a>;
}

pub trait CownCollection: CownCollectionSuper {
    fn lock(&self) -> Self::Guard<'_>;
    fn cown_bases(&self) -> impl IntoIterator<Item = &dyn CownBase>;
}

struct Request {
    next: AtomicPtr<Behavior>,
    scheduled: AtomicBool,
    target: *const AtomicPtr<Request>,
}

unsafe impl Send for Request {}
unsafe impl Sync for Request {}

impl Request {
    pub fn new(target: &dyn CownBase) -> Self {
        Request {
            next: AtomicPtr::new(null_mut()),
            scheduled: AtomicBool::new(false),
            target: target.last(),
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
                    .compare_exchange(
                        self as *const _ as *mut _,
                        NULL,
                        AtomicOrd::SeqCst,
                        AtomicOrd::Acquire,
                    )
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
    fn consume_boxed_and_release(&mut self);
}

impl<C, F> Thunk for (C, Option<F>)
where C: CownCollection, F: for<'a> FnOnce(C::Guard<'a>) + Send + Sync + 'static
{
    fn consume_boxed_and_release(&mut self) {
        self.1.take().unwrap()(self.0.lock());
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
                runner.run({
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

unsafe impl<T: Send> Send for Cown<T> {}
unsafe impl<T: Sync> Sync for Cown<T> {}
// impl<T> UnwindSafe for Cown<T> {}

impl<T> Cown<T> {
    pub const fn new(value: T) -> Self {
        Cown {
            last: AtomicPtr::new(null_mut()),
            data: Mutex::new(value),
        }
    }
    pub fn into_inner(self) -> LockResult<T> {
        self.data.into_inner()
    }
    pub fn get_mut(&mut self) -> LockResult<&mut T> {
        self.data.get_mut()
    }
    pub fn try_with<U, F: for<'a> FnOnce(&'a T) -> U>(&self, f: F) -> Option<U> {
        (!self.last.load(AtomicOrd::Acquire).is_null())
            .then(|| f(&mut *self.data.try_lock().unwrap()))
    }
    pub fn try_with_mut<U, F: for<'a> FnOnce(&'a mut T) -> U>(&self, f: F) -> Option<U> {
        (!self.last.load(AtomicOrd::Acquire).is_null())
            .then(|| f(&mut *self.data.try_lock().unwrap()))
    }
}

impl<T: Send + Sync + 'static> CownBase for Cown<T> {
    #[doc(hidden)]
    #[allow(private_interfaces)]
    fn last(&self) -> *const AtomicPtr<Request> {
        &self.last
    }
}

macro_rules! ref_cown_collection {
    ( $v:ident => $( $t:ty ),+ )  =>  {
        $(
            impl<$v: Send + Sync + 'static> CownCollectionSuper for $t {
                type Guard<'a> = MutexGuard<'a, T>;
            }
            impl<$v: Send + Sync + 'static> CownCollection for $t {
                fn lock(&self) -> Self::Guard<'_> {
                    self.data.lock().unwrap()
                }

                fn cown_bases<'a>(&'a self) -> impl IntoIterator<Item=&'a dyn CownBase> {
                    once(&**self as &dyn CownBase)
                }
            }
        )+
    };
}

ref_cown_collection! {
    T => &'static Cown<T>, Arc<Cown<T>>
}

impl<T: CownCollectionSuper, const N: usize> CownCollectionSuper for [T; N] {
    type Guard<'a> = [T::Guard<'a>; N];
}
impl<T: CownCollection, const N: usize> CownCollection for [T; N] {
    fn lock(&self) -> Self::Guard<'_> {
        self.each_ref().map(T::lock)
    }

    fn cown_bases(&self) -> impl IntoIterator<Item = &dyn CownBase> {
        self.iter().flat_map(T::cown_bases)
    }
}

macro_rules! variadic_cown_collection {
    ($($v:ident)*)   =>  {
        impl<$($v: CownCollectionSuper,)*> CownCollectionSuper for ($($v,)*) {
            type Guard<'a> = ($($v::Guard<'a>,)*);
        }
        impl<$($v: CownCollection,)*> CownCollection for ($($v,)*) {
            fn lock(&self) -> Self::Guard<'_> {
                #![allow(non_snake_case)]
                let ($(ref $v,)*) = self;
                #[allow(clippy::unused_unit)]
                ($($v.lock(),)*)
            }
            fn cown_bases(&self) -> impl IntoIterator<Item=&dyn CownBase> {
                #![allow(non_snake_case)]
                let ($(ref $v,)*) = self;
                empty() $(.chain($v.cown_bases()))*
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
