use std::convert::TryFrom;

use num::CheckedAdd;

/// Extension trait for a convenience method which transmutes a slice to a slice of another type
/// while ensuring correct alignment and size.
pub trait ExactAlign {
    /// SAFETY: See the documentation for `slice::align_to`
    unsafe fn exact_align_to<Target>(&self) -> &[Target];
}

impl<T> ExactAlign for [T] {
    unsafe fn exact_align_to<Target>(&self) -> &[Target] {
        let (before, target, after) = self.align_to::<Target>();
        assert!(before.is_empty());
        assert!(after.is_empty());
        target
    }
}

pub fn checked_add<T, U, V>(lhs: T, rhs: U) -> Option<V>
where V: TryFrom<T> + TryFrom<U> + CheckedAdd {
    let lhs = V::try_from(lhs).ok()?;
    let rhs = V::try_from(rhs).ok()?;
    lhs.checked_add(&rhs)
}

// TODO fail compilation if usize > u64
/// Converts a `usize` into a `u64`. Since `usize` is at most 64 bits wide, this conversion will never fail.
pub fn u64_from(n: usize) -> u64 {
    debug_assert!(u64::try_from(n).is_ok());
    n as u64
}

/// Converts a `u32` into a `usize`. Since `usize` is at least 32 bits wide, this conversion will never fail.
pub fn usize_from(n: u32) -> usize {
    debug_assert!(usize::try_from(n).is_ok());
    n as usize
}
