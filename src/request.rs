use std::ptr::{null_mut, NonNull};
use std::sync::atomic::{AtomicPtr, AtomicUsize, Ordering};
use erasable::Erasable;
use tagptr::{AtomicTagPtr, TagPtr};
use crate::Behavior;
use crate::cown::CownBase;



pub struct Request {
    pub(crate) scheduled_and_next_behavior: ScheduledAndNextBehavior,
    pub(crate) next_request: AtomicPtr<Request>,
    pub(crate) release_data: ReleaseDataUnion,
    pub(crate) lock: Lock,
}

impl Request {
    pub fn new(target: *mut CownBase, read_only: bool) -> Self {
        Request {
            scheduled_and_next_behavior: ScheduledAndNextBehavior(AtomicTagPtr::null()),
            next_request: AtomicPtr::new(null_mut()),
            //  Dummy value
            release_data: ReleaseDataUnion { imm_count: AtomicUsize::new(0) },
            lock: Lock(TagPtr::compose(target, read_only as _)),
        }
    }
    pub unsafe fn data(&self) -> ReleaseData {
        if self.lock.read_only() {
            ReleaseData::PrevOrMut(&self.release_data.prev_or_mut)
        } else {
            ReleaseData::ImmCount(&self.release_data.imm_count)
        }
    }
}

/// Contains [`scheduled`] and [`next`] from the original paper.
pub(crate) struct ScheduledAndNextBehavior(AtomicTagPtr<[Behavior; 0], 1>);
impl ScheduledAndNextBehavior {
    pub fn new() -> Self {
        ScheduledAndNextBehavior(AtomicTagPtr::new(TagPtr::null()))
    }
    pub fn scheduled(&self) -> bool {
        self.0.load(Ordering::Acquire).decompose_tag() == 1
    }
    pub fn set_scheduled(&self) {
        self.0.fetch_or(1, Ordering::AcqRel);
    }
    /// Should only be called after [`set_next_behavior`] is called.
    pub fn next_behavior(&self) -> NonNull<Behavior> {
        unsafe {
            Behavior::unerase(NonNull::new(self.0.load(Ordering::Acquire).decompose_ptr() as *mut _).unwrap())
        }
    }
    /// Also unsets `scheduled`, as `scheduled` is always set after `next_behavior`.
    pub fn set_next_behavior(&self, next_behavior: NonNull<Behavior>) {
        self.0.store(TagPtr::new(next_behavior.as_ptr() as *mut _), Ordering::Release)
    }
}

pub(crate) union ReleaseDataUnion {
    /// A count of outstanding immutable locks that need to be freed before this mutable lock can be granted.
    pub imm_count: AtomicUsize,
    /// Either the previous immutable lock request (or null if it is the first after a mutable lock)
    /// or the next mutable lock request after this set of immutable lock requests.
    pub prev_or_mut: AtomicPtr<Request>,
}
pub(crate) enum ReleaseData<'a> {
    ImmCount(&'a AtomicUsize),
    PrevOrMut(&'a AtomicPtr<Request>)
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
/// A sentinel value indicating that this read-only request has been released
pub(crate) const RELEASED: *mut Request = 2 as *mut _;
