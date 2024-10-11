use super::*;
use std::cell::UnsafeCell;
use std::iter;

macro_rules! ref_cown_collection {
    ( $v:ident => $( $t:ty ),+ )  =>  {
        $(
            impl<$v: Send + Sync + 'static> CownCollectionSuper for $t {
                type Mut<'a> = &'a mut T;
                type Imm<'a> = &'a T;
            }
            impl<$v: Send + Sync + 'static> CownCollection for $t {
                unsafe fn get_mut(&self) -> Self::Mut<'_> {
                    &mut *UnsafeCell::get(&self.data)
                }
                unsafe fn get_imm(&self) -> Self::Imm<'_> {
                    &*UnsafeCell::get(&self.data)
                }
                fn infos<'a>(&'a self) -> impl Iterator<Item=CownInfo> {
                    iter::once(self.info())
                }
            }
        )+
    };
}

ref_cown_collection! {
    T => &'static Cown<T>, Arc<Cown<T>>
}
/*
macro_rules! collection_cown_collection {
    ( $v:ident => $( $t:ty ),+ ) => {
        $(
            impl<$v: CownCollectionSuper> CownCollectionSuper for $t {
                type Mut<'a> = Box<[$v::Mut<'a>]>;
                type Imm<'a> = Box<[$v::Imm<'a>]>;
            }
            impl<$v: CownCollection> CownCollection for $t {
                unsafe fn get_mut(&self) -> Self::Mut<'_> {
                    self.into_iter().map(|cc| unsafe { cc.get_mut() }).collect()
                }
                unsafe fn get_imm(&self) -> Self::Imm<'_> {
                    self.into_iter().map(|cc| unsafe { cc.get_imm() }).collect()
                }

                fn infos(&self) -> impl IntoIterator<Item=CownInfo> {
                    self.into_iter().flat_map($v::infos)
                }
            }
        )+
    };
}

collection_cown_collection! {
    T => Vec<T>, VecDeque<T>, Arc<[T]>
}
*/
macro_rules! variadic_cown_collection {
    ($($v:ident)*)   =>  {
        impl<$($v: CownCollectionSuper,)*> CownCollectionSuper for ($($v,)*) {
            type Mut<'a> = ($($v::Mut<'a>,)*);
            type Imm<'a> = ($($v::Imm<'a>,)*);
        }
        impl<$($v: CownCollection,)*> CownCollection for ($($v,)*) {
            unsafe fn get_mut(&self) -> Self::Mut<'_> {
                #![allow(non_snake_case)]
                let ($(ref $v,)*) = self;
                #[allow(clippy::unused_unit)]
                ($($v.get_mut(),)*)
            }
            unsafe fn get_imm(&self) -> Self::Imm<'_> {
                #![allow(non_snake_case)]
                let ($(ref $v,)*) = self;
                #[allow(clippy::unused_unit)]
                ($($v.get_imm(),)*)
            }
            fn infos(&self) -> impl Iterator<Item=CownInfo> {
                #![allow(non_snake_case)]
                let ($(ref $v,)*) = self;
                iter::empty() $(.chain($v.infos()))*
            }
        }
        impl<$($v: LockCollectionSuper,)*> LockCollectionSuper for ($($v,)*) {
            type Ref<'a> = ($($v::Ref<'a>,)*);
        }
        impl<$($v: LockCollection,)*> LockCollection for ($($v,)*) {
            unsafe fn get_ref(&self) -> Self::Ref<'_> {
                #![allow(non_snake_case)]
                let ($(ref $v,)*) = self;
                #[allow(clippy::unused_unit)]
                ($($v.get_ref(),)*)
            }
            fn infos(&self) -> impl Iterator<Item=LockInfo> {
                #![allow(non_snake_case)]
                let ($(ref $v,)*) = self;
                iter::empty() $(.chain($v.infos()))*
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
