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

/// Stopgap function to convert an ASCII string from a FAT short name into a null-terminated UTF-16 string like
/// from a FAT long name
pub fn short_name_to_long_name(short_name: &str) -> Vec<u16> {
    let mut name: Vec<_> = short_name.chars().map(|c| c as u16).collect();
    name.push(0);
    name
}
