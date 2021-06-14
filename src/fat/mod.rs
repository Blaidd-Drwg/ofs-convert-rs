// #[allow(dead_code)]
// mod fs_tree_serializer;
mod boot_sector;
mod dentry;
mod file;
mod partition;
mod partition_iter;
mod table_index;

pub use self::boot_sector::*;
pub use self::dentry::*;
pub use self::file::*;
pub use self::partition::*;
pub use self::partition_iter::*;
pub use self::table_index::*;

/// An index identifying a cluster in the partition.
pub type ClusterIdx = u32;
pub type Cluster = [u8];

/// The first FAT index belonging to the root directory. This corresponds to the first data cluster, i.e. the n-th FAT
/// entry corresponds to the (n-2)-th data cluster.
pub const ROOT_FAT_IDX: FatTableIndex = FatTableIndex::new(2);
