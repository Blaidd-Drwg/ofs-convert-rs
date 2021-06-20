use std::ops::Range;

use crate::allocator::Allocator;
use crate::c_wrapper::{c_add_extent, c_build_inode, c_build_lost_found_inode, c_build_root_inode};
use crate::ext4::{BlockGroup, Ext4BlockGroupConstructionInfo, Ext4GroupDescriptor, Extent, Inode, SuperBlock};
use crate::fat::{BootSector, ClusterIdx, FatDentry};

pub struct Ext4Partition<'a> {
    // padding: &'a mut [u8; GROUP_0_PADDING as usize],
    start: *const u8, // temporary
    block_groups: Vec<BlockGroup<'a>>,
}

impl<'a> Ext4Partition<'a> {
    pub unsafe fn from(partition_ptr: *mut u8, boot_sector: &BootSector) -> Self {
        let superblock = SuperBlock::from(boot_sector).unwrap();
        let mut block_groups = Vec::new();
        let mut block_group_descriptors = Vec::new();

        for block_group_idx in 0..superblock.block_group_count() as usize {
            let info = Ext4BlockGroupConstructionInfo::new(&superblock, block_group_idx);
            block_group_descriptors.push(Ext4GroupDescriptor::new(info));
            block_groups.push(BlockGroup::new(partition_ptr, info));
        }

        *block_groups[0].superblock.as_deref_mut().unwrap() = superblock;
        block_groups[0]
            .gdt
            .as_deref_mut()
            .unwrap()
            .copy_from_slice(&block_group_descriptors);
        Self { start: partition_ptr, block_groups }
    }

    pub fn superblock(&self) -> &SuperBlock {
        self.block_groups[0].superblock.as_deref().unwrap()
    }

    pub fn superblock_mut(&mut self) -> &mut SuperBlock {
        self.block_groups[0].superblock.as_deref_mut().unwrap()
    }

    pub fn group_descriptor_table_mut(&mut self) -> &mut [Ext4GroupDescriptor] {
        self.block_groups[0].gdt.as_deref_mut().unwrap()
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
        self.mark_range_as_used(inode, extent.as_range());

        let additional_blocks = inode.add_extent(extent, allocator);
        for block in additional_blocks {
            self.mark_range_as_used(inode, block..block + 1);
        }
    }

    pub fn block_group_idx_of_block(&self, block_idx: ClusterIdx) -> usize {
        // any block before `s_first_data_block` doesn't belong to any block group
        let data_block_idx = block_idx - self.superblock().s_first_data_block;
        (data_block_idx / self.superblock().s_blocks_per_group) as usize
    }

    pub fn mark_range_as_used(&mut self, inode: &mut Inode, range: Range<ClusterIdx>) {
        c_add_extent(inode.inode_no, range.start, range.len() as u16);

        inode.increment_used_blocks(range.len(), self.superblock().block_size() as usize);

        let block_group_idx = self.block_group_idx_of_block(range.start);
        assert_eq!(block_group_idx, self.block_group_idx_of_block(range.end - 1));

        self.group_descriptor_table_mut()[block_group_idx].decrement_free_blocks_count(range.len() as u32);

        let group_start_block = self.superblock_mut().block_group_start_cluster(block_group_idx);
        let relative_range = range.start - group_start_block..range.end - group_start_block;
        self.block_groups[block_group_idx].mark_relative_range_as_used(relative_range);
    }
}

        // // Make superblock and group descriptor table backup copies
        // let superblock = *self.superblock_mut();
        // let gdt = self.group_descriptor_table_mut().to_vec();
        // for backup_group_idx in superblock.s_backup_bgs {
        // let block_group = &mut self.block_groups[backup_group_idx as usize];
        // (*block_group.superblock.as_deref_mut().unwrap()) = superblock;
        // block_group.gdt.as_deref_mut().unwrap().copy_from_slice(&gdt);
        // }
