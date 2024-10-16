use crate::request::Request;
use crate::{CollectionSliceImm, CollectionSliceMut, CownSliceImm, CownSliceMut, VecDequeLockImm, VecDequeLockMut};
use std::cell::UnsafeCell;
use std::cmp::Ordering;
use std::collections::{vec_deque, VecDeque};
use std::ptr::null_mut;
use std::sync::atomic::AtomicPtr;
use std::sync::{Arc, LockResult};
use std::{iter, slice};

/// A mutex where multiple mutexes can be locked in parallel.
pub struct Cown<T> {
    pub(crate) meta: CownBase,
    pub(crate) data: UnsafeCell<T>,
}

pub(crate) struct CownBase {
    pub(crate) last: AtomicPtr<Request>,
    pub(crate) last_mut: AtomicPtr<Request>,
}

/// Needed to work around an issue with Rust's trait system.  Effectively part of [`CownCollection`].
pub trait CownCollectionSuper: Send + Sync + 'static {
    /// The mutable reference type.
    type Mut<'a>;
    /// The immutable reference type.
    type Imm<'a>;
}

/// Indicates that a type is a collection of [`Cown`]s and can be locked together.
pub trait CownCollection: CownCollectionSuper {
    /// Get mutable references into the collection of [`Cown`]s.
    unsafe fn get_mut(&self) -> Self::Mut<'_>;
    /// Get immutable references into the collection of [`Cown`]s.
    unsafe fn get_imm(&self) -> Self::Imm<'_>;
    /// Get type erased information about the [`Cown`]s.
    fn infos(&self) -> impl Iterator<Item = CownInfo>;
}

unsafe impl<T: Send> Send for Cown<T> {}

unsafe impl<T: Sync> Sync for Cown<T> {}

impl<T> Cown<T> {
    /// Create a new `cown`.
    pub const fn new(value: T) -> Self {
        Cown {
            meta: CownBase {
                last: AtomicPtr::new(null_mut()),
                last_mut: AtomicPtr::new(null_mut()),
            },
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

impl<T: Send + Sync + 'static> CownCollectionSuper for &'static [Cown<T>] {
    type Mut<'a> = CownSliceMut<'a, T>;
    type Imm<'a> = CownSliceImm<'a, T>;
}
impl<T: Send + Sync + 'static> CownCollection for &'static [Cown<T>] {
    unsafe fn get_mut(&self) -> Self::Mut<'_> {
        CownSliceMut { slice: self }
    }

    unsafe fn get_imm(&self) -> Self::Imm<'_> {
        CownSliceImm { slice: self }
    }

    fn infos(&self) -> impl Iterator<Item = CownInfo> {
        self.iter().map(|cown| CownInfo {
            last: &cown.meta as *const _ as *mut _,
        })
    }
}

impl<T: Send + Sync + 'static> CownCollectionSuper for Arc<[Cown<T>]> {
    type Mut<'a> = CownSliceMut<'a, T>;
    type Imm<'a> = CownSliceImm<'a, T>;
}
impl<T: Send + Sync + 'static> CownCollection for Arc<[Cown<T>]> {
    unsafe fn get_mut(&self) -> Self::Mut<'_> {
        CownSliceMut { slice: self }
    }

    unsafe fn get_imm(&self) -> Self::Imm<'_> {
        CownSliceImm { slice: self }
    }

    fn infos(&self) -> impl Iterator<Item = CownInfo> {
        self.iter().map(|cown| CownInfo {
            last: &cown.meta as *const _ as *mut _,
        })
    }
}

impl<T: CownCollectionSuper, const N: usize> CownCollectionSuper for [T; N] {
    type Mut<'a> = [T::Mut<'a>; N];
    type Imm<'a> = [T::Imm<'a>; N];
}

impl<T: CownCollection, const N: usize> CownCollection for [T; N] {
    unsafe fn get_mut(&self) -> Self::Mut<'_> {
        self.each_ref().map(|cc| unsafe { T::get_mut(cc) })
    }

    unsafe fn get_imm(&self) -> Self::Imm<'_> {
        self.each_ref().map(|cc| unsafe { T::get_imm(cc) })
    }

    fn infos(&self) -> impl Iterator<Item = CownInfo> {
        self.iter().flat_map(T::infos)
    }
}

impl<T: CownCollection> CownCollectionSuper for Vec<T> {
    type Mut<'a> = CollectionSliceMut<'a, T>;
    type Imm<'a> = CollectionSliceImm<'a, T>;
}
impl<T: CownCollection> CownCollection for Vec<T> {
    unsafe fn get_mut(&self) -> Self::Mut<'_> {
        CollectionSliceMut { slice: self.as_slice() }
    }

    unsafe fn get_imm(&self) -> Self::Imm<'_> {
        CollectionSliceImm { slice: self.as_slice() }
    }

    fn infos(&self) -> impl Iterator<Item = CownInfo> {
        self.iter().flat_map(T::infos)
    }
}

impl<T: CownCollection> CownCollectionSuper for Arc<[T]> {
    type Mut<'a> = CollectionSliceMut<'a, T>;
    type Imm<'a> = CollectionSliceImm<'a, T>;
}
impl<T: CownCollection> CownCollection for Arc<[T]> {
    unsafe fn get_mut(&self) -> Self::Mut<'_> {
        CollectionSliceMut { slice: self }
    }

    unsafe fn get_imm(&self) -> Self::Imm<'_> {
        CollectionSliceImm { slice: self }
    }

    fn infos(&self) -> impl Iterator<Item = CownInfo> {
        self.iter().flat_map(T::infos)
    }
}

impl<T: CownCollection> CownCollectionSuper for Box<[T]> {
    type Mut<'a> = CollectionSliceMut<'a, T>;
    type Imm<'a> = CollectionSliceImm<'a, T>;
}
impl<T: CownCollection> CownCollection for Box<[T]> {
    unsafe fn get_mut(&self) -> Self::Mut<'_> {
        CollectionSliceMut { slice: self }
    }

    unsafe fn get_imm(&self) -> Self::Imm<'_> {
        CollectionSliceImm { slice: self }
    }

    fn infos(&self) -> impl Iterator<Item = CownInfo> {
        self.iter().flat_map(T::infos)
    }
}

impl<T: CownCollection> CownCollectionSuper for VecDeque<T> {
    type Mut<'a> = VecDequeLockMut<'a, T>;
    type Imm<'a> = VecDequeLockImm<'a, T>;
}
impl<T: CownCollection> CownCollection for VecDeque<T> {
    unsafe fn get_mut(&self) -> Self::Mut<'_> {
        VecDequeLockMut { vec_deque: self }
    }

    unsafe fn get_imm(&self) -> Self::Imm<'_> {
        VecDequeLockImm { vec_deque: self }
    }

    fn infos(&self) -> impl Iterator<Item = CownInfo> {
        self.iter().flat_map(T::infos)
    }
}

/// Type-erased information about the location at which a [`Cown`] is stored.
#[derive(Clone, Copy, Debug, Hash)]
pub struct CownInfo {
    pub(crate) last: *mut CownBase,
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

impl<'a, T> CownSliceMut<'a, T> {
    /// Get an iterator of immutable references to the locks' value.
    pub fn iter(&self) -> iter::Map<slice::Iter<'a, Cown<T>>, for<'c> fn(&'c Cown<T>) -> &'c T> {
        self.slice.iter().map(|cown| unsafe { &*cown.data.get() })
    }
    /// Get an iterator of mutable references to the locks' values
    pub fn iter_mut(&self) -> iter::Map<slice::Iter<'a, Cown<T>>, for<'c> fn(&'c Cown<T>) -> &'c mut T> {
        self.slice.iter().map(|cown| unsafe { &mut *cown.data.get() })
    }
}

impl<'a, 'b, T> IntoIterator for &'a CownSliceMut<'b, T> {
    type Item = &'b T;
    type IntoIter = iter::Map<slice::Iter<'b, Cown<T>>, for<'c> fn(&'c Cown<T>) -> &'c T>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}
impl<'a, 'b, T> IntoIterator for &'a mut CownSliceMut<'b, T> {
    type Item = &'b mut T;
    type IntoIter = iter::Map<slice::Iter<'b, Cown<T>>, for<'c> fn(&'c Cown<T>) -> &'c mut T>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter_mut()
    }
}

impl<'a, T> CownSliceImm<'a, T> {
    /// Get an iterator of immutable references to the locks' values
    pub fn iter(&self) -> iter::Map<slice::Iter<'a, Cown<T>>, for<'c> fn(&'c Cown<T>) -> &'c T> {
        self.slice.iter().map(|cown| unsafe { &*cown.data.get() })
    }
}

impl<'a, 'b, T> IntoIterator for &'a CownSliceImm<'b, T> {
    type Item = &'b T;
    type IntoIter = iter::Map<slice::Iter<'b, Cown<T>>, for<'c> fn(&'c Cown<T>) -> &'c T>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

impl<'a, CC: CownCollection> CollectionSliceMut<'a, CC> {
    /// Get an iterator of immutable references to the locks' values
    pub fn iter(&self) -> iter::Map<slice::Iter<'a, CC>, for<'b> fn(&'b CC) -> CC::Imm<'b>> {
        self.slice.iter().map(|cc| unsafe { cc.get_imm() })
    }
    /// Get an iterator of mutable references to the locks' values
    pub fn iter_mut(&mut self) -> iter::Map<slice::Iter<'a, CC>, for<'b> fn(&'b CC) -> CC::Mut<'b>> {
        self.slice.iter().map(|cc| unsafe { cc.get_mut() })
    }
}

impl<'a, 'b, CC: CownCollection> IntoIterator for &'a CollectionSliceMut<'b, CC> {
    type Item = CC::Imm<'b>;
    type IntoIter = iter::Map<slice::Iter<'b, CC>, for<'c> fn(&'c CC) -> CC::Imm<'c>>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}
impl<'a, 'b, CC: CownCollection> IntoIterator for &'a mut CollectionSliceMut<'b, CC> {
    type Item = CC::Mut<'b>;
    type IntoIter = iter::Map<slice::Iter<'b, CC>, for<'c> fn(&'c CC) -> CC::Mut<'c>>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter_mut()
    }
}

impl<'a, CC: CownCollection> CollectionSliceImm<'a, CC> {
    /// Get an iterator of immutable references to the locks' values
    pub fn iter(&self) -> iter::Map<slice::Iter<'a, CC>, for<'c> fn(&'c CC) -> CC::Imm<'c>> {
        self.slice.iter().map(|cc| unsafe { cc.get_imm() })
    }
}

impl<'a, 'b, CC: CownCollection> IntoIterator for &'a CollectionSliceImm<'b, CC> {
    type Item = CC::Imm<'b>;
    type IntoIter = iter::Map<slice::Iter<'b, CC>, for<'c> fn(&'c CC) -> CC::Imm<'c>>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

impl<'a, CC: CownCollection> VecDequeLockMut<'a, CC> {
    /// Get an iterator of immutable references to the locks' values
    pub fn iter(&self) -> iter::Map<vec_deque::Iter<'a, CC>, for<'c> fn(&'c CC) -> CC::Imm<'c>> {
        self.vec_deque.iter().map(|cc| unsafe { cc.get_imm() })
    }
    /// Get an iterator of mutable references to the locks' values
    pub fn iter_mut(&self) -> iter::Map<vec_deque::Iter<'a, CC>, for<'c> fn(&'c CC) -> CC::Mut<'c>> {
        self.vec_deque.iter().map(|cc| unsafe { cc.get_mut() })
    }
}

impl<'a, 'b, CC: CownCollection> IntoIterator for &'a VecDequeLockMut<'b, CC> {
    type Item = CC::Imm<'b>;
    type IntoIter = iter::Map<vec_deque::Iter<'b, CC>, for<'c> fn(&'c CC) -> CC::Imm<'c>>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}
impl<'a, 'b, CC: CownCollection> IntoIterator for &'a mut VecDequeLockMut<'b, CC> {
    type Item = CC::Mut<'b>;
    type IntoIter = iter::Map<vec_deque::Iter<'b, CC>, for<'c> fn(&'c CC) -> CC::Mut<'c>>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter_mut()
    }
}

impl<'a, CC: CownCollection> VecDequeLockImm<'a, CC> {
    /// Get an iterator of immutable references to the locks' values
    pub fn iter(&self) -> iter::Map<vec_deque::Iter<'a, CC>, for<'c> fn(&'c CC) -> CC::Imm<'c>> {
        self.vec_deque.iter().map(|cc| unsafe { cc.get_imm() })
    }
}

impl<'a, 'b, CC: CownCollection> IntoIterator for &'a VecDequeLockImm<'b, CC> {
    type Item = CC::Imm<'b>;
    type IntoIter = iter::Map<vec_deque::Iter<'b, CC>, for<'c> fn(&'c CC) -> CC::Imm<'c>>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}
