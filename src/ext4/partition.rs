use crate::fat::{BootSector, ClusterIdx};
use crate::ranges::Ranges;
use crate::ext4::{HasSuperBlock, SuperBlock, GROUP_0_PADDING};

const FIRST_SUPERBLOCK_OFFSET: usize = 1024;

pub struct Ext4Partition<'a> {
    // padding: &'a mut [u8; GROUP_0_PADDING as usize],
    start: *const u8, // temporary
    block_groups: Vec<BlockGroup<'a>>,
}

impl<'a> Ext4Partition<'a> {
    // SAFETY: TODO
    pub fn from(partition_data: &'a mut [u8], boot_sector: &BootSector) -> Self {
        let superblock = SuperBlock::from(boot_sector).unwrap();
        // (0..superblock.block_group_count()).
        let mut block_groups = Vec::new();
        let ptr = partition_data.as_ptr();
        for (block_group_idx, block_group_data) in partition_data.chunks_mut(superblock.block_size() as usize).enumerate() {
            unsafe {
                block_groups.push(BlockGroup::new(block_group_data, superblock.block_group_has_superblock(block_group_idx)));
            }
        }
        Self { start: ptr, block_groups }
    }

    pub fn superblock(&self) -> &SuperBlock {
        self.block_groups[0].superblock().unwrap()
    }

    // temporary
    pub fn as_ptr(&self) -> *const u8 {
        self.start
    }
}

pub struct BlockGroup<'a> {
    superblock: Option<&'a mut SuperBlock>,
    // gdt: Option<&'a mut [u8]>,
    // data_block_bitmap: &'a mut [u8],
    // inode_bitmap: &'a mut [u8],
    // inode_table: &'a mut [u8],
    // data: &'a mut [u8],
}

impl<'a> BlockGroup<'a> {
    pub unsafe fn new(block_group_data: &'a mut [u8], has_superblock: HasSuperBlock) -> Self {
        let superblock = match has_superblock {
            HasSuperBlock::YesOriginal => Some(&mut *(block_group_data.as_mut_ptr().add(FIRST_SUPERBLOCK_OFFSET) as *mut SuperBlock)),
            HasSuperBlock::YesBackup => None,
            HasSuperBlock::No => None,
        };
        Self { superblock }
    }

    pub fn superblock(&'a self) -> Option<&'a SuperBlock> {
        self.superblock.as_deref()
    }
}
