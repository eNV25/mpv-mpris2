pub(crate) use std::ptr::*;

/// Convert a mutable reference to a raw pointer.
///
/// This is equivalent to `r as *mut T`, but is a bit safer since it will never silently change
/// type or mutability, in particular if the code is refactored.
#[inline(always)]
#[must_use]
pub(crate) fn from_mut<T: ?Sized>(r: &mut T) -> *mut T {
    r
}
