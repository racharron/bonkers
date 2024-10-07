use std::sync::{Arc, LockResult};
use std::sync::atomic::AtomicPtr;
use std::iter::{once, empty};
use std::cell::UnsafeCell;
use std::collections::{LinkedList, VecDeque};
use std::marker::PhantomData;
use std::ops::{Index, IndexMut};
use std::ptr::null_mut;
use crate::Request;

/// A mutex where multiple mutexes can be locked in parallel.  By default, locks provide a read-only
/// view of the contained data.
pub struct Cown<T> {
    last: AtomicPtr<Request>,
    data: UnsafeCell<T>,
}

/// Indicates a read-write lock of a [`Cown`].
pub struct Mut<T>(pub T);

/// Indicates a read-only lock of a [`Cown`].
pub struct Ref<T>(pub T);

/// A type that can be converted into reference(s) to a [`Cown`] of some sort.  This includes [`Arc`]s.
pub trait ToCownRef: Clone {
    /// Abstracts references to a [`Cown`] for ease of use.
    type CownRef: CownRef;
    /// Clones the base [`Cown`] reference to a static object that can be copied and passed around
    /// threads.  This is used to ensure that [`Cown`]s live long enough (e.g., [`Arc<Cown<T>>`]s.
    fn to_cown_ref(self) -> Self::CownRef;
}

/// A direct reference to a (or multiple) [`Cown`].
pub trait CownRef: ToCownRef + Send + Sync + 'static {
    /// An immutable reference to the inner data.
    type Ref<'a>;
    /// A mutable reference to the inner data.
    type Mut<'a>;
    /// Get an immutable reference to the [`Cown`]'s inner data.
    unsafe fn get_ref(&self) -> Self::Ref<'_>;
    /// Get a mutable reference to the [`Cown`]'s inner data.
    unsafe fn get_mut(&self) -> Self::Mut<'_>;
    #[doc(hidden)]
    #[allow(private_interfaces)]
    /// Get an iterator of pointers to the pointer to the most recent request of this [`Cown`].
    fn last(&self) -> impl Iterator<Item=*const AtomicPtr<Request>>;
}

/// A possibly owned collection of [`Cown`] references.
pub trait RequestRefCollection {
    /// The owned lock collection.
    type Owned: RequestCollection;
    /// Clone whatever needs to be cloned to get an owned collection of [`Cown`]s.
    fn to_owned(self) -> Self::Owned;
}

/// This supertrait is needed as a workaround for a limitation in Rust's trait system.
pub trait RequestCollectionSuper {
    /// Allows access to the locked data.
    type Locked<'a>;
}

/// Indicates that a type is a collection requests to access a [`Cown`].
pub trait RequestCollection: RequestCollectionSuper + Send + Sync + 'static {
    /// Get the locks of the cowns in the collection.  Structurally mirrors the collection.
    ///
    /// Does not check if the [`Cown`]s are in use.
    unsafe fn locked(&self) -> Self::Locked<'_>;
    #[doc(hidden)]
    #[allow(private_interfaces)]
    fn cown_bases(&self) -> impl Iterator<Item = *const AtomicPtr<Request>>;
}

/// Allows for indexing into the contained data of datastructures of [`Cown`]s without needing to allocate
/// arrays of references.  Only allows for immutable access.
pub struct IndexCownRef<'a, T: ?Sized> {
    inner: &'a T
}

/// Allows for indexing into the contained data of datastructures of [`Cown`]s without needing to allocate
/// arrays of references.  Allows for mutable access.
pub struct IndexCownMut<'a, T: ?Sized> {
    inner: &'a T,
}

/// An iterator over references to the inner values of a sequence of [`Cown`]s.
pub struct CownIter<'a, I> {
    inner: I,
    _phantom: PhantomData<&'a I>
}
/// An iterator over references to the inner values of a sequence of [`Cown`]s.
pub struct CownIterMut<'a, I> {
    inner: I,
    _phantom: PhantomData<&'a I>
}

impl<'a, C, CR: for<'b> CownRef<Ref<'b>=&'b C>, I, T: Index<I, Output=CR>> Index<I> for IndexCownRef<'a, T> {
    type Output = C;

    fn index(&self, index: I) -> &Self::Output {
        unsafe {
            self.inner[index].get_ref()
        }
    }
}
impl<'a, C, CR: for<'b> CownRef<Ref<'b>=&'b C, Mut<'b>=&'b mut C>, I, T: Index<I, Output=CR>> Index<I> for IndexCownMut<'a, T> {
    type Output = C;

    fn index(&self, index: I) -> &Self::Output {
        unsafe {
            self.inner[index].get_ref()
        }
    }
}
impl<'a, C, CR: for<'b> CownRef<Ref<'b>=&'b C, Mut<'b>=&'b mut C>, I, T: Index<I, Output=CR>> IndexMut<I> for IndexCownMut<'a, T> {

    fn index_mut(&mut self, index: I) -> &mut Self::Output {
        unsafe {
            self.inner[index].get_mut()
        }
    }
}

impl<'a, 'b, T: ?Sized, CR: CownRef> IntoIterator for &'b IndexCownRef<'a, T> where &'a T: IntoIterator<Item=&'a CR> {
    type Item = CR::Ref<'a>;
    type IntoIter = CownIter<'a, <&'a T as IntoIterator>::IntoIter>;

    fn into_iter(self) -> Self::IntoIter {
        CownIter {
            inner: self.inner.into_iter(),
            _phantom: PhantomData,
        }
    }
}

impl<'a, 'b, T: ?Sized, CR: CownRef> IntoIterator for &'b IndexCownMut<'a, T> where &'a T: IntoIterator<Item=&'a CR> {
    type Item = CR::Ref<'a>;
    type IntoIter = CownIter<'a, <&'a T as IntoIterator>::IntoIter>;

    fn into_iter(self) -> Self::IntoIter {
        CownIter {
            inner: self.inner.into_iter(),
            _phantom: PhantomData,
        }
    }
}
impl<'a, 'b, T: ?Sized, CR: CownRef> IntoIterator for &'b mut IndexCownMut<'a, T> where &'a T: IntoIterator<Item=&'a CR> {
    type Item = CR::Mut<'a>;
    type IntoIter = CownIterMut<'a, <&'a T as IntoIterator>::IntoIter>;

    fn into_iter(self) -> Self::IntoIter {
        CownIterMut {
            inner: self.inner.into_iter(),
            _phantom: PhantomData,
        }
    }
}

impl<'a, CR: CownRef, I: Iterator<Item=&'a CR>> Iterator for CownIter<'a, I> {
    type Item = CR::Ref<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        self.inner.next().map(|c| unsafe { c.get_ref() })
    }
}
impl<'a, CR: CownRef, I: Iterator<Item=&'a CR>> Iterator for CownIterMut<'a, I> {
    type Item = CR::Mut<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        self.inner.next().map(|c| unsafe { c.get_mut() })
    }
}

unsafe impl<T: Send> Send for Cown<T> {}

unsafe impl<T: Sync> Sync for Cown<T> {}

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

impl<T: Send + Sync + 'static> ToCownRef for &'static Cown<T> {
    type CownRef = Self;

    fn to_cown_ref(self) -> Self::CownRef {
        self
    }
}

impl<T: Send + Sync + 'static> ToCownRef for Arc<Cown<T>> {
    type CownRef = Self;

    fn to_cown_ref(self) -> Self::CownRef {
        self
    }
}

impl<'a, TCR: ToCownRef> ToCownRef for &'a TCR {
    type CownRef = TCR::CownRef;

    fn to_cown_ref(self) -> Self::CownRef {
        TCR::clone(self).to_cown_ref()
    }
}

impl<TCR: ToCownRef, const N: usize> ToCownRef for [TCR; N] {
    type CownRef = [TCR::CownRef; N];

    fn to_cown_ref(self) -> Self::CownRef {
        self.map(TCR::to_cown_ref)
    }
}

impl<'a, TCR: ToCownRef> ToCownRef for &'a [TCR] {
    type CownRef = Box<[TCR::CownRef]>;

    fn to_cown_ref(self) -> Self::CownRef {
        self.into_iter().map(|tcr| TCR::clone(tcr).to_cown_ref()).collect()
    }
}

impl<TCR: ToCownRef> ToCownRef for Box<[TCR]> {
    type CownRef = Box<[TCR::CownRef]>;

    fn to_cown_ref(self) -> Self::CownRef {
        Box::into_iter(self).map(TCR::to_cown_ref).collect()
    }
}
impl<CR: CownRef> CownRef for Box<[CR]> {
    type Ref<'a> = IndexCownRef<'a, [CR]>;
    type Mut<'a> = IndexCownMut<'a, [CR]>;

    unsafe fn get_ref(&self) -> Self::Ref<'_> {
        IndexCownRef {
            inner: &**self,
        }
    }

    unsafe fn get_mut(&self) -> Self::Mut<'_> {
        IndexCownMut {
            inner: &**self
        }
    }

    #[doc(hidden)]
    #[allow(private_interfaces)]
    fn last(&self) -> impl Iterator<Item=*const AtomicPtr<Request>> {
        self.iter().flat_map(CR::last)
    }
}
impl<CR: CownRef> ToCownRef for Vec<CR> {
    type CownRef = Self;

    fn to_cown_ref(self) -> Self::CownRef {
        self
    }
}
impl<CR: CownRef> CownRef for Vec<CR> {
    type Ref<'a> = IndexCownRef<'a, [CR]>;
    type Mut<'a> = IndexCownMut<'a, [CR]>;

    unsafe fn get_ref(&self) -> Self::Ref<'_> {
        IndexCownRef {
            inner: &**self,
        }
    }

    unsafe fn get_mut(&self) -> Self::Mut<'_> {
        IndexCownMut {
            inner: &**self,
        }
    }

    #[doc(hidden)]
    #[allow(private_interfaces)]
    fn last(&self) -> impl Iterator<Item=*const AtomicPtr<Request>> {
        self.into_iter().flat_map(CR::last)
    }
}

impl<TCR: ToCownRef> RequestRefCollection for Ref<TCR> {
    type Owned = Ref<TCR::CownRef>;
    fn to_owned(self) -> Self::Owned {
        Ref(self.0.to_cown_ref())
    }
}

impl<TCR: ToCownRef> RequestRefCollection for Mut<TCR> {
    type Owned = Mut<TCR::CownRef>;
    fn to_owned(self) -> Self::Owned {
        Mut(self.0.to_cown_ref())
    }
}

impl<'a, CC: RequestRefCollection + Clone> RequestRefCollection for &'a CC {
    type Owned = CC::Owned;

    fn to_owned(self) -> Self::Owned {
        CC::clone(self).to_owned()
    }
}

impl<TCR: ToCownRef> RequestRefCollection for Arc<[Ref<TCR>]> {
    type Owned = Ref<Box<[TCR::CownRef]>>;

    fn to_owned(self) -> Self::Owned {
        Ref(self.into_iter().map(|tcr| tcr.0.clone().to_cown_ref()).collect())
    }
}

impl<TCR: ToCownRef> RequestRefCollection for Arc<[Mut<TCR>]> {
    type Owned = Mut<Box<[TCR::CownRef]>>;

    fn to_owned(self) -> Self::Owned {
        Mut(self.into_iter().map(|tcr| tcr.0.clone().to_cown_ref()).collect())
    }
}

impl<CR: CownRef> RequestCollectionSuper for Ref<CR> {
    type Locked<'a> = CR::Ref<'a>;
}

impl<CR: CownRef> RequestCollectionSuper for Mut<CR> {
    type Locked<'a> = CR::Mut<'a>;
}

impl<CR: CownRef> RequestCollection for Ref<CR> {
    unsafe fn locked(&self) -> Self::Locked<'_> {
        self.0.get_ref()
    }

    #[doc(hidden)]
    #[allow(private_interfaces)]
    fn cown_bases(&self) -> impl Iterator<Item=*const AtomicPtr<Request>> {
        self.0.last()
    }
}

impl<CR: CownRef> RequestCollection for Mut<CR> {
    unsafe fn locked(&self) -> Self::Locked<'_> {
        self.0.get_mut()
    }

    #[doc(hidden)]
    #[allow(private_interfaces)]
    fn cown_bases(&self) -> impl Iterator<Item=*const AtomicPtr<Request>> {
        self.0.last()
    }
}

impl<T: Send + Sync + 'static> CownRef for &'static Cown<T> {
    type Ref<'a> = &'a T;
    type Mut<'a> = &'a mut T;

    unsafe fn get_ref(&self) -> Self::Ref<'_> {
        &*UnsafeCell::raw_get(&self.data)
    }

    unsafe fn get_mut(&self) -> Self::Mut<'_> {
        &mut *UnsafeCell::raw_get(&self.data)
    }

    #[doc(hidden)]
    #[allow(private_interfaces)]
    fn last(&self) -> impl Iterator<Item=*const AtomicPtr<Request>> {
        once(&self.last as *const _)
    }
}

impl<T: Send + Sync + 'static> CownRef for Arc<Cown<T>> {
    type Ref<'a> = &'a T;
    type Mut<'a> = &'a mut T;

    unsafe fn get_ref(&self) -> Self::Ref<'_> {
        &*UnsafeCell::raw_get(&self.data)
    }

    unsafe fn get_mut(&self) -> Self::Mut<'_> {
        &mut *UnsafeCell::raw_get(&self.data)
    }

    #[doc(hidden)]
    #[allow(private_interfaces)]
    fn last(&self) -> impl Iterator<Item=*const AtomicPtr<Request>> {
        once(&self.last as *const _)
    }
}

impl<T: CownRef, const N: usize> CownRef for [T; N] {
    type Ref<'a> = [T::Ref<'a>; N];
    type Mut<'a> = [T::Mut<'a>; N];

    unsafe fn get_ref(&self) -> Self::Ref<'_> {
        self.each_ref().map(|cr| unsafe { cr.get_ref() })
    }

    unsafe fn get_mut(&self) -> Self::Mut<'_> {
        self.each_ref().map(|cr| unsafe { cr.get_mut() })
    }

    #[doc(hidden)]
    #[allow(private_interfaces)]
    fn last(&self) -> impl Iterator<Item=*const AtomicPtr<Request>> {
        self.iter().flat_map(T::last)
    }
}

impl<T: RequestRefCollection, const N: usize> RequestRefCollection for [T; N] {
    type Owned = [T::Owned; N];

    fn to_owned(self) -> Self::Owned {
        self.map(T::to_owned)
    }
}

impl<T: RequestCollection, const N: usize> RequestCollectionSuper for [T; N] {
    type Locked<'a> = [T::Locked<'a>; N];
}

impl<T: RequestCollection, const N: usize> RequestCollection for [T; N] {

    unsafe fn locked(&self) -> Self::Locked<'_> {
        self.each_ref().map(|cc| unsafe { cc.locked() })
    }
    #[doc(hidden)]
    #[allow(private_interfaces)]
    fn cown_bases(&self) -> impl Iterator<Item = *const AtomicPtr<Request>> {
        self.iter().flat_map(T::cown_bases)
    }
}


macro_rules! variadic_cown_impl {
    ($($v:ident)*) => {
        impl<$($v: ToCownRef),*> ToCownRef for ($($v,)*) {
            type CownRef = ($($v::CownRef,)*);
            fn to_cown_ref(self) -> Self::CownRef {
                #[allow(non_snake_case)]
                let ($($v,)*) = self;
                ($($v.to_cown_ref(),)*)
            }
        }
        impl<$($v: CownRef),*> CownRef for ($($v,)*) {
            type Ref<'a> = ($($v::Ref<'a>,)*);
            type Mut<'a> = ($($v::Mut<'a>,)*);
            unsafe fn get_ref(&self) -> Self::Ref<'_> {
                #[allow(non_snake_case)]
                let ($(ref $v,)*) = self;
                ($($v.get_ref(),)*)
            }
            unsafe fn get_mut(&self) -> Self::Mut<'_> {
                #[allow(non_snake_case)]
                let ($(ref $v,)*) = self;
                ($($v.get_mut(),)*)
            }
            #[doc(hidden)]
            #[allow(private_interfaces)]
            fn last(&self) -> impl Iterator<Item=*const AtomicPtr<Request>> {
                #[allow(non_snake_case)]
                let ($(ref $v,)*) = self;
                empty() $( .chain($v.last()) )*
            }
        }
    };
}

variadic_cown_impl! {}
variadic_cown_impl! { A }
variadic_cown_impl! { A B }
variadic_cown_impl! { A B C }
variadic_cown_impl! { A B C D }
variadic_cown_impl! { A B C D E }
variadic_cown_impl! { A B C D E F }
variadic_cown_impl! { A B C D E F G }
variadic_cown_impl! { A B C D E F G H }
variadic_cown_impl! { A B C D E F G H I }
variadic_cown_impl! { A B C D E F G H I J }
variadic_cown_impl! { A B C D E F G H I J K }
variadic_cown_impl! { A B C D E F G H I J K L }
variadic_cown_impl! { A B C D E F G H I J K L M }
variadic_cown_impl! { A B C D E F G H I J K L M N }
variadic_cown_impl! { A B C D E F G H I J K L M N O }
variadic_cown_impl! { A B C D E F G H I J K L M N O P }
variadic_cown_impl! { A B C D E F G H I J K L M N O P Q }
variadic_cown_impl! { A B C D E F G H I J K L M N O P Q R }
variadic_cown_impl! { A B C D E F G H I J K L M N O P Q R S }
variadic_cown_impl! { A B C D E F G H I J K L M N O P Q R S T }
variadic_cown_impl! { A B C D E F G H I J K L M N O P Q R S T U }
variadic_cown_impl! { A B C D E F G H I J K L M N O P Q R S T U V }
variadic_cown_impl! { A B C D E F G H I J K L M N O P Q R S T U V W }
variadic_cown_impl! { A B C D E F G H I J K L M N O P Q R S T U V W X }
variadic_cown_impl! { A B C D E F G H I J K L M N O P Q R S T U V W X Y }
variadic_cown_impl! { A B C D E F G H I J K L M N O P Q R S T U V W X Y Z }
