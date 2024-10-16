use crate::{Behavior, CownBase, ErasedBehavior};
use erasable::Erasable;
use std::mem::ManuallyDrop;
use std::ptr::{null_mut, NonNull};
use std::sync::atomic::{AtomicBool, AtomicPtr, AtomicU8, AtomicUsize, Ordering};
use tagptr::{AtomicTagPtr, TagPtr};

pub(crate) struct Request {
    pub next_behavior: AtomicPtr<ErasedBehavior>,
    pub state: AtomicU8,
    /// In mutable requests, points to the next request
    pub prev_request: AtomicPtr<Request>,
    pub next_request: AtomicPtr<Request>,
    pub read_only: bool,
    pub base: *mut CownBase,
}

impl Request {
    pub fn new(target: *mut CownBase, read_only: bool) -> Self {
        Request {
            next_behavior: AtomicPtr::new(null_mut()),
            state: AtomicU8::new(Self::UNSCHEDULED),
            prev_request: AtomicPtr::new(null_mut()),
            next_request: AtomicPtr::new(null_mut()),
            read_only,
            base: target,
        }
    }
    /// Indicates that `scheduled` from the paper is false, otherwise it is true and the additional
    /// states are to deal with the read-write lock aspects.
    pub const UNSCHEDULED: u8 = 0;
    /// This lock is scheduled. If immutable, then it is not released and does not have a mutable
    /// lock that depends on it.
    pub const SCHEDULED: u8 = 1;
    /// This lock is in the middle of being released (or has been released, and should not be added
    /// to the count of behavior dependencies.
    pub const RELEASED: u8 = 2;
    /// This (immutable) lock has a mutable lock that depends on it.
    pub const HAS_NEXT_MUT: u8 = 3;
}

/*
/// Contains [`scheduled`] and [`next`] from the original paper.
///
/// For mutable requests, points to the direct next [`Behavior`].  For immutable requests, points
/// to the next *mutable* [`Behavior`].
///
/// A thin pointer (so it can be atomic).
pub(crate) union State {
    tagged: ManuallyDrop<AtomicTagPtr<ErasedBehavior, 1>>,
    raw: ManuallyDrop<AtomicPtr<ErasedBehavior>>,
    uint: ManuallyDrop<AtomicUsize>,
}
impl State {
    pub fn new() -> Self {
        State { uint: ManuallyDrop::new(AtomicUsize::new(1)) }
    }
    pub fn scheduled(&self) -> bool {
        unsafe {
            self.tagged.load(Ordering::Acquire).decompose_tag() == 0
        }
    }
    /// Erase the not scheduled tag bit.
    pub fn set_scheduled(&self) {
        unsafe {
            self.tagged.fetch_and(0, Ordering::AcqRel);
        }
    }
    /// Should only be called after [`set_next_behavior`] is called.
    pub fn next_behavior(&self) -> Option<NonNull<Behavior>> {
        unsafe {
            NonNull::new(self.raw.load(Ordering::Acquire) as *mut _).map(|erased| Behavior::unerase(erased))
        }
    }
    /// Keeps the scheduled tag
    pub fn set_next_behavior(&self, next_behavior: *mut ErasedBehavior) {
        unsafe {
            self.uint.fetch_and((next_behavior as usize) | 1, Ordering::AcqRel);
        }
    }
}

pub(crate) union Data {
    /// A count (+1) of outstanding immutable locks that need to be freed before this mutable lock can be granted.
    /// Over counts by 1 so zero can be used as an uninitialized value.
    pub imm_count: ManuallyDrop<AtomicUsize>,
    /// The previous immutable lock request.  Null indicates that the previous request of this [`Cown`]
    /// was a mutable lock request.
    pub prev: ManuallyDrop<AtomicPtr<Request>>,
    /// The next mutable lock request.
    pub next_mut: ManuallyDrop<AtomicPtr<Request>>,
    /// A neutral way of checking if [`prev_imm`] and [`next_mut`] are non-null.
    pub other_request: ManuallyDrop<AtomicPtr<Request>>,
}

pub(crate) struct Lock(TagPtr<CownBase, 1>);
impl Lock {
    pub fn new(cown: *const CownBase, read_only: bool) -> Self {
        Lock(TagPtr::compose(cown as *mut _, read_only as _))
    }
    pub fn read_only(&self) -> bool {
        self.0.decompose_tag() == 1
    }
    pub fn cown(&self) -> *const CownBase {
        self.0.decompose_ptr()
    }
}*/

unsafe impl Send for Request {}

unsafe impl Sync for Request {}

/// Null pointer.
pub(crate) const NULL: *mut Request = 0 as *mut _;
/// In practice, this address will never be used by correct code (not in the least because it
/// (should) not be aligned.
///
/// TODO: check provenance (`offset` a no-provenance pointer is legal, but is `byte_offset`)?
pub(crate) const POISONED: *mut Request = 1 as *mut _;
