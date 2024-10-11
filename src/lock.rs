use super::*;
use std::ops::{Index, IndexMut};

/// Needed to work around an issue with Rust's trait system.  A super trait of [`LockCollection`].
pub trait LockCollectionSuper: Send + Sync + 'static {
    /// A reference to the inner data of a [`Cown`].
    type Ref<'a>;
}

/// A collection of locks ([`Mut`] or [`Imm`]) of [`Cown`]`s.
pub trait LockCollection: LockCollectionSuper {
    /// Get references to data inside the [`Cown`]s.
    unsafe fn get_ref(&self) -> Self::Ref<'_>;
    /// Get information about the locks.
    fn infos(&self) -> impl Iterator<Item = LockInfo>;
}

/// A lock on a [`Cown`] granting exclusive mutable access.
pub struct Mut<C: CownCollection>(pub C);

/// A lock on a [`Cown`] granting shared immutable access.
pub struct Imm<C: CownCollection>(pub C);

/// Information about a lock of a [`Cown`].
pub struct LockInfo {
    pub(crate) cown: CownInfo,
    pub(crate) read_only: bool,
}
impl LockInfo {
    /// Returns information about the locked [`Cown`]'s memory address.
    pub fn cown_info(&self) -> CownInfo {
        self.cown
    }
    /// Returns information about whether the lock is read-only.
    pub fn read_only(&self) -> bool {
        self.read_only
    }
}

impl<C: CownCollection> LockCollectionSuper for Mut<C> {
    type Ref<'a> = C::Mut<'a>;
}
impl<C: CownCollection> LockCollectionSuper for Imm<C> {
    type Ref<'a> = C::Imm<'a>;
}

impl<C: CownCollection> LockCollection for Mut<C> {
    unsafe fn get_ref(&self) -> Self::Ref<'_> {
        self.0.get_mut()
    }

    fn infos(&self) -> impl Iterator<Item = LockInfo> {
        self.0.infos().map(|info| LockInfo { cown: info, read_only: false })
    }
}

impl<C: CownCollection> LockCollection for Imm<C> {
    unsafe fn get_ref(&self) -> Self::Ref<'_> {
        self.0.get_imm()
    }

    fn infos(&self) -> impl Iterator<Item = LockInfo> {
        self.0.infos().map(|info| LockInfo { cown: info, read_only: true })
    }
}

impl<T: LockCollectionSuper, const N: usize> LockCollectionSuper for [T; N] {
    type Ref<'a> = [T::Ref<'a>; N];
}

impl<T: LockCollection, const N: usize> LockCollection for [T; N] {
    unsafe fn get_ref(&self) -> Self::Ref<'_> {
        self.each_ref().map(|cc| unsafe { T::get_ref(cc) })
    }

    fn infos(&self) -> impl Iterator<Item = LockInfo> {
        self.iter().flat_map(T::infos)
    }
}

/// Access the inner data of a slice of [`CownCollection`]s.  Immutable references only.
pub struct CollectionSliceImm<'a, C: CownCollection> {
    pub(crate) slice: &'a [C],
}
/// Access the inner data of a slice of [`CownCollection`]s.  Mutable and immutable references.
pub struct CollectionSliceMut<'a, C: CownCollection> {
    pub(crate) slice: &'a [C],
}

/// Accesses the inner data of a slice of [`Cown`]s.  Immutable references only.
pub struct CownSliceImm<'a, T> {
    pub(crate) slice: &'a [Cown<T>],
}
/// Accesses the inner data of a slice of [`Cown`]s.  Mutable and immutable references only.
pub struct CownSliceMut<'a, T> {
    pub(crate) slice: &'a [Cown<T>],
}

impl<'a, C: CownCollection> CollectionSliceImm<'a, C> {
    /// Unfortunately, the default [`Index`] trait does not allow returning custom, non-reference
    /// types, so it cannot be used.  Instead, this indexes the container of [`CownCollection`]s.
    pub fn get(&self, index: usize) -> C::Imm<'_> {
        unsafe { self.slice[index].get_imm() }
    }
}

impl<'a, C: CownCollection> CollectionSliceMut<'a, C> {
    /// Essentially [`Index::index`].
    pub fn get(&self, index: usize) -> C::Imm<'_> {
        unsafe { self.slice[index].get_imm() }
    }
    /// Essentially [`IndexMut::index`].
    pub fn get_mut(&self, index: usize) -> C::Mut<'_> {
        unsafe { self.slice[index].get_mut() }
    }
}

/// Gets the inner data of a [`VecDeque`] of [`Cown`]s.  Immutable references only.
pub struct VecDequeLockImm<'a, C: CownCollection> {
    pub(crate) vec_deque: &'a VecDeque<C>,
}
/// Gets the inner data of a [`VecDeque`] of [`Cown`]s.  Mutable and immutable references.
pub struct VecDequeLockMut<'a, C: CownCollection> {
    pub(crate) vec_deque: &'a VecDeque<C>,
}

impl<'a, C: CownCollection> VecDequeLockImm<'a, C> {
    /// Essentially [`Index::index`].
    pub fn get(&self, index: usize) -> C::Imm<'_> {
        unsafe { self.vec_deque[index].get_imm() }
    }
}

impl<'a, C: CownCollection> VecDequeLockMut<'a, C> {
    /// Essentially [`Index::index`].
    pub fn get(&self, index: usize) -> C::Imm<'_> {
        unsafe { self.vec_deque[index].get_imm() }
    }
    /// Essentially [`IndexMut::index_mut`].
    pub fn get_mut(&self, index: usize) -> C::Mut<'_> {
        unsafe { self.vec_deque[index].get_mut() }
    }
}

impl<'a, T> Index<usize> for CownSliceImm<'a, T> {
    type Output = T;

    fn index(&self, index: usize) -> &Self::Output {
        unsafe { &*self.slice[index].data.get() }
    }
}

impl<'a, T> Index<usize> for CownSliceMut<'a, T> {
    type Output = T;

    fn index(&self, index: usize) -> &Self::Output {
        unsafe { &*self.slice[index].data.get() }
    }
}
impl<'a, T> IndexMut<usize> for CownSliceMut<'a, T> {
    fn index_mut(&mut self, index: usize) -> &mut Self::Output {
        unsafe { &mut *self.slice[index].data.get() }
    }
}
