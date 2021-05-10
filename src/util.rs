/// Extension trait for a convenience method which transmutes a slice to a slice of another type 
/// while ensuring correct alignment and size.
pub trait ExactAlign {
	/// SAFETY: See the documentation for `slice::align_to`
	unsafe fn exact_align_to<'a, Target>(&'a self) -> &'a [Target];
}

impl<T> ExactAlign for [T] {
	unsafe fn exact_align_to<'a, Target>(&'a self) -> &'a [Target] {
		let (before, target, after) = self.align_to::<Target>();
		assert!(before.is_empty());
		assert!(after.is_empty());
		target
	}
}
