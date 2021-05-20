// #[allow(dead_code)]
// mod fs_tree_serializer;
mod dentry;
mod boot_sector;
mod partition;
mod partition_iter;
mod file;

pub use self::dentry::*;
pub use self::boot_sector::*;
pub use self::partition::*;
pub use self::partition_iter::*;
pub use self::file::*;

pub type ClusterIdx = u32;
pub type Cluster = [u8];

const FIRST_ROOT_DIR_CLUSTER_IDX: ClusterIdx = 2; // the first cluster containing data has the index 2
