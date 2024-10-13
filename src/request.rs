use crate::{Behavior, CownBase, ErasedBehavior};
use erasable::Erasable;
use std::mem::ManuallyDrop;
use std::ptr::{null_mut, NonNull};
use std::sync::atomic::{AtomicPtr, AtomicUsize, Ordering};
use tagptr::{AtomicTagPtr, TagPtr};

pub(crate) struct Request {
    /// Holds whether this [`Request`] is scheduled or a pointer to the next behavior.
    pub state: State,
    pub next_request: AtomicPtr<Request>,
    pub data: DataUnion,
    pub lock: Lock,
}

impl Request {
    pub fn new(target: *mut CownBase, read_only: bool) -> Self {
        Request {
            state: State::new(),
            next_request: AtomicPtr::new(null_mut()),
            //  Dummy value
            data: DataUnion {
                imm_count: ManuallyDrop::new(AtomicUsize::new(0)),
            },
            lock: Lock::new(target, read_only),
        }
    }
}

/// Contains [`scheduled`] and [`next`] from the original paper.
///
/// For mutable requests, points to the direct next [`Behavior`].  For immutable requests, points
/// to the next *mutable* [`Behavior`].
///
/// A thin pointer (so it can be atomic).
pub(crate) struct State(AtomicTagPtr<ErasedBehavior, 1>);
impl State {
    pub fn new() -> Self {
        State(AtomicTagPtr::from(1 as *mut _))
    }
    pub fn scheduled(&self) -> bool {
        self.0.load(Ordering::Acquire).decompose_tag() == 0
    }
    /// Erase the not scheduled tag bit.
    pub fn set_scheduled(&self) {
        self.0.fetch_and(!1, Ordering::AcqRel);
    }
    /// Should only be called after [`set_next_behavior`] is called.
    pub fn next_behavior(&self) -> Option<NonNull<Behavior>> {
        NonNull::new(self.0.load(Ordering::Acquire).into_raw() as *mut _).map(|erased| unsafe { Behavior::unerase(erased) })
    }
    /// Keeps the scheduled tag
    pub fn set_next_behavior(&self, next_behavior: *mut ErasedBehavior) {
        self.0.fetch_or(next_behavior as usize, Ordering::Release);
    }
}

pub(crate) union DataUnion {
    /// A count (+1) of outstanding immutable locks that need to be freed before this mutable lock can be granted.
    /// Over counts by 1 so zero can be used as an uninitialized value.
    pub imm_count: ManuallyDrop<AtomicUsize>,
    /// The previous immutable lock request.  Null indicates that the previous request of this [`Cown`]
    /// was a mutable lock request.
    pub prev_imm: ManuallyDrop<AtomicPtr<Request>>,
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
}

unsafe impl Send for Request {}

unsafe impl Sync for Request {}

/// Null pointer.
pub(crate) const NULL: *mut Request = 0 as *mut _;
/// In practice, this address will never be used by correct code (not in the least because it
/// (should) not be aligned.
///
/// TODO: check provenance (`offset` a no-provenance pointer is legal, but is `byte_offset`)?
pub(crate) const POISONED: *mut Request = 1 as *mut _;
