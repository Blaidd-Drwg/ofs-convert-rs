// #[allow(dead_code)]
// mod fs_tree_serializer;
mod boot_sector;
mod dentry;
mod file;
mod fs;
mod fs_iter;
mod table_index;

pub use self::boot_sector::*;
pub use self::dentry::*;
pub use self::file::*;
pub use self::fs::*;
pub use self::fs_iter::*;
pub use self::table_index::*;

/// An index identifying a cluster in the filesystem.
/// A FAT32 file system's cluster count fits into a `u32`. This means a valid cluster index will never overflow
/// `ClusterIdx`. Since the maximum possible cluster index + 1 fits into a `u32`, this also means that every range of
/// valid cluster indices can be represented as a `Range<ClusterIdx>`.
pub type ClusterIdx = u32;
pub type Cluster = [u8];
