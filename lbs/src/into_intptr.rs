use std::ptr::NonNull;

use libc::*;

pub trait BasicallyAnIntptr: Copy {
    unsafe fn into_intptr(self) -> intptr_t;
    unsafe fn from_intptr(intptr: intptr_t) -> Self;
}

macro_rules! impl_into_intptr_for_int {
    ($($T:ident),*) => {$(
        impl BasicallyAnIntptr for $T {
            #[inline(always)]
            unsafe fn into_intptr(self) -> intptr_t {
                self as _
            }

            #[inline(always)]
            unsafe fn from_intptr(intptr: intptr_t) -> Self {
                intptr as _
            }
        }
    )*};
}

impl_into_intptr_for_int!(i64, u64, usize, isize);

impl<T> BasicallyAnIntptr for *const T {
    #[inline(always)]
    unsafe fn into_intptr(self) -> intptr_t {
        self as _
    }

    #[inline(always)]
    unsafe fn from_intptr(intptr: intptr_t) -> Self {
        intptr as _
    }
}

impl<T> BasicallyAnIntptr for *mut T {
    #[inline(always)]
    unsafe fn into_intptr(self) -> intptr_t {
        self as _
    }

    #[inline(always)]
    unsafe fn from_intptr(intptr: intptr_t) -> Self {
        intptr as _
    }
}

impl<T> BasicallyAnIntptr for NonNull<T> {
    #[inline(always)]
    unsafe fn into_intptr(self) -> intptr_t {
        self.as_ptr() as _
    }

    #[inline(always)]
    unsafe fn from_intptr(intptr: intptr_t) -> Self {
        NonNull::new_unchecked(intptr as *mut T)
    }
}

pub trait ClosureArgsInner {
    unsafe fn read_closure_fn_args(pointer: *mut intptr_t) -> Self;
}

impl<T: BasicallyAnIntptr> ClosureArgsInner for (T,) {
    #[inline(always)]
    unsafe fn read_closure_fn_args(pointer: *mut intptr_t) -> Self {
        (T::from_intptr(*pointer.add(1)),)
    }
}
