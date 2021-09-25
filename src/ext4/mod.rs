mod block_group;
mod dentry;
mod extent;
mod fs;
mod group_descriptor;
mod inode;
mod superblock;

pub use self::block_group::*;
pub use self::dentry::*;
pub use self::extent::*;
pub use self::fs::*;
pub use self::group_descriptor::*;
pub use self::inode::*;
pub use self::superblock::*;
use crate::util::usize_from;

/// The first block in the partition is padded with 1024 bytes. If the block size is also 1024 bytes, the entire first
/// block is padding, and the first block group starts with the second block.
pub const FIRST_BLOCK_PADDING: usize = 1024;

/// There is no inode with inode_no 0.
pub const FIRST_EXISTING_INODE: u32 = 1;
pub const FIRST_NON_RESERVED_INODE: u32 = 11;

// TODO blockidx newtype?
pub type BlockIdx = usize;
#[allow(non_snake_case)]
pub fn BlockIdx_from(n: u32) -> BlockIdx {
    usize_from(n)
}
