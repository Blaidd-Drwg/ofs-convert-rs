use std::convert::TryFrom;

use anyhow::{bail, Result};
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

/// Converts a `usize` into a `u64`. Since `usize` is at most 64 bits wide, this conversion will never fail.
pub trait FromUsize {
    fn fromx(n: usize) -> Self;
}
impl FromUsize for u64 {
    fn fromx(n: usize) -> Self {
        debug_assert!(Self::try_from(n).is_ok());
        n as Self
    }
}

/// Converts a `u32` into a `usize`. Since `usize` is at least 32 bits wide, this conversion will never fail.
pub trait FromU32 {
    fn fromx(n: u32) -> Self;
}
impl FromU32 for usize {
    fn fromx(n: u32) -> Self {
        debug_assert!(Self::try_from(n).is_ok());
        n as Self
    }
}

pub fn exact_log2(n: u32) -> Result<u8> {
    let log = (f64::from(n)).log2().round() as u8;
    if 2_u32.pow(u32::from(log)) != n {
        bail!("n is not a power of 2");
    }
    Ok(log)
}
