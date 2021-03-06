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

/// The first block in the partition is padded with 1024 bytes. If the block size is also 1024 bytes, the entire first
/// block is padding, and the first block group starts with the second block.
pub const FIRST_BLOCK_PADDING: usize = 1024;

/// These inode numbers don't correspond to an actual file.
pub const SPECIAL_INODES: [InodeNo; 9] = [1, 3, 4, 5, 6, 7, 8, 9, 10];
/// There is no inode with inode_no 0.
pub const FIRST_EXISTING_INODE: InodeNo = 1;
pub const FIRST_NON_RESERVED_INODE: InodeNo = 11;

pub type BlockSize = u32;
pub type BlockGroupCount = u32;
pub type BlockGroupIdx = BlockGroupCount;
pub type InodeCount = u32;
pub type InodeNo = InodeCount;
pub type BlockCount = usize;
pub type BlockIdx = BlockCount;
