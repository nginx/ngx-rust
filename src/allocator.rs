//! The allocator module.
//!
//! The module provides custom memory allocator support traits and utilities based on the unstable
//! [feature(allocator_api)].
//!
//! Currently implemented as a reexport of parts of the [allocator_api2].
//!
//! [feature(allocator_api)]: https://github.com/rust-lang/rust/issues/32838

use ::core::alloc::Layout;
use ::core::mem;
use ::core::ptr::{self, NonNull};

pub use allocator_api2::alloc::{AllocError, Allocator};

#[cfg(feature = "alloc")]
pub use allocator_api2::{alloc::Global, boxed::Box};

/// Explicitly duplicate an object using the specified Allocator.
pub trait TryCloneIn: Sized {
    /// Target type, generic over an allocator.
    type Target<A: Allocator + Clone>;

    /// Attempts to copy the value using `alloc` as an underlying Allocator.
    fn try_clone_in<A: Allocator + Clone>(&self, alloc: A) -> Result<Self::Target<A>, AllocError>;
}

/// Moves `value` to the memory backed by `alloc` and returns a pointer.
///
/// This should be similar to `Box::into_raw(Box::try_new_in(value, alloc)?)`, except without
/// `alloc` requirement and intermediate steps.
///
/// # Note
///
/// The resulting pointer has no owner. The caller is responsible for destroying `T` and releasing
/// the memory.
pub fn allocate<T, A>(value: T, alloc: &A) -> Result<NonNull<T>, AllocError>
where
    A: Allocator,
{
    let layout = Layout::for_value(&value);
    let ptr: NonNull<T> = alloc.allocate(layout)?.cast();

    // SAFETY: the allocator succeeded and gave us a correctly aligned pointer to an uninitialized
    // data
    unsafe { ptr.cast::<mem::MaybeUninit<T>>().as_mut().write(value) };

    Ok(ptr)
}
///
/// Creates a [NonNull] that is dangling, but well-aligned for this [Layout].
///
/// See also [::core::alloc::Layout::dangling()]
#[inline(always)]
pub(crate) const fn dangling_for_layout(layout: &Layout) -> NonNull<u8> {
    unsafe {
        let ptr = ptr::null_mut::<u8>().byte_add(layout.align());
        NonNull::new_unchecked(ptr)
    }
}

#[cfg(feature = "alloc")]
mod impls {
    use allocator_api2::boxed::Box;

    use super::*;

    impl<T, OA> TryCloneIn for Box<T, OA>
    where
        T: TryCloneIn,
        OA: Allocator,
    {
        type Target<A: Allocator + Clone> = Box<<T as TryCloneIn>::Target<A>, A>;

        fn try_clone_in<A: Allocator + Clone>(
            &self,
            alloc: A,
        ) -> Result<Self::Target<A>, AllocError> {
            let x = self.as_ref().try_clone_in(alloc.clone())?;
            Box::try_new_in(x, alloc)
        }
    }
}

/// Allows turning a [`Box<T: Sized, A>`][Box] into a [`Box<U: ?Sized, A>`][Box] where `T` can be
/// unsizing-coerced into a `U`.
///
/// See [allocator_api2::unsize_box] for an explanation why this macro is necessary.
#[cfg(feature = "alloc")]
#[doc(inline)]
pub use crate::__unsize_box as unsize_box;

// We have to reimplement this macro because the original implementation is not reexportable.
// Macro definitions float to the top of the crate, thus we also mark it as hidden and reexport
// again from the right namespace.
#[cfg(feature = "alloc")]
#[doc(hidden)]
#[macro_export]
macro_rules! __unsize_box {
    ( $boxed:expr $(,)? ) => {{
        let (ptr, alloc) = $crate::allocator::Box::into_raw_with_allocator($boxed);
        unsafe { $crate::allocator::Box::from_raw_in(ptr as *mut _, alloc) }
    }};
}
