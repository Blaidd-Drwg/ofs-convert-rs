use std::ops::Range;

use crate::allocator::Allocator;
use crate::c_wrapper::{c_add_extent, c_build_inode, c_build_lost_found_inode, c_build_root_inode};
use crate::ext4::{Extent, HasSuperBlock, Inode, SuperBlock};
use crate::fat::{BootSector, ClusterIdx, FatDentry};

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
        for (block_group_idx, block_group_data) in
            partition_data.chunks_mut(superblock.block_size() as usize).enumerate()
        {
            unsafe {
                block_groups.push(BlockGroup::new(
                    block_group_data,
                    superblock.block_group_has_superblock(block_group_idx),
                ));
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

    pub fn build_inode(&mut self, dentry: &FatDentry) -> Inode<'a> {
        c_build_inode(dentry)
    }

    pub fn build_root_inode(&mut self) -> Inode<'a> {
        c_build_root_inode()
    }

    pub fn build_lost_found_inode(&mut self) -> Inode<'a> {
        c_build_lost_found_inode()
    }

    pub fn set_extents(&mut self, inode: &mut Inode, extents: Vec<Range<ClusterIdx>>, allocator: &Allocator<'_>) {
        let mut logical_start = 0;
        for extent in extents {
            let ext_extent = Extent::new(extent, logical_start);
            self.register_extent(inode, ext_extent, allocator);
            logical_start += ext_extent.len as u32;
        }
    }

    pub fn register_extent(&mut self, inode: &mut Inode, extent: Extent, allocator: &Allocator) {
        c_add_extent(inode.inode_no, extent.physical_start_lo, extent.len);

        let additional_blocks = inode.add_extent(extent, allocator);
        for block in additional_blocks {
            c_add_extent(inode.inode_no, block, 1);
        }
    }
}

pub struct BlockGroup<'a> {
    superblock: Option<&'a mut SuperBlock>,
    /* gdt: Option<&'a mut [u8]>,
     * data_block_bitmap: &'a mut [u8],
     * inode_bitmap: &'a mut [u8],
     * inode_table: &'a mut [InodeInner],
     * data: &'a mut [u8], */
}

impl<'a> BlockGroup<'a> {
    pub unsafe fn new(block_group_data: &'a mut [u8], has_superblock: HasSuperBlock) -> Self {
        let superblock = match has_superblock {
            HasSuperBlock::YesOriginal => {
                Some(&mut *(block_group_data.as_mut_ptr().add(FIRST_SUPERBLOCK_OFFSET) as *mut SuperBlock))
            }
            HasSuperBlock::YesBackup => None,
            HasSuperBlock::No => None,
        };
        Self { superblock }
    }

    pub fn superblock(&'a self) -> Option<&'a SuperBlock> {
        self.superblock.as_deref()
    }
}
