use std::ops::Range;

use num::Integer;

use crate::allocator::Allocator;
use crate::c_wrapper::{c_add_extent, c_add_inode};
use crate::ext4::{
    BlockGroup, Ext4BlockGroupConstructionInfo, Ext4GroupDescriptor, Extent, Inode, SuperBlock, EXT4_LOST_FOUND_INODE,
    EXT4_ROOT_INODE, FIRST_NON_RESERVED_INODE,
};
use crate::fat::{BootSector, ClusterIdx};

pub const FIRST_EXISTING_INODE: u32 = 1;

pub struct Ext4Partition<'a> {
    // padding: &'a mut [u8; GROUP_0_PADDING as usize],
    start: *const u8, // temporary
    block_groups: Vec<BlockGroup<'a>>,
    next_free_inode_no: u32,
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
        Self {
            start: partition_ptr,
            block_groups,
            next_free_inode_no: FIRST_NON_RESERVED_INODE,
        }
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

    pub unsafe fn build_root_inode(&mut self) -> Inode<'a> {
        let inode_no = EXT4_ROOT_INODE;
        let existing_inode_no = inode_no - FIRST_EXISTING_INODE;
        let inode_size = self.superblock().s_inode_size as usize;
        let inner = self.block_groups[0].get_relative_inode(existing_inode_no as usize, inode_size);
        let mut inode = Inode { inode_no, inner };
        inode.init_root();
        c_add_inode(inode_no, true);
        inode
    }

    pub fn build_lost_found_inode(&mut self) -> Inode<'a> {
        let mut inode = self.allocate_inode(true);
        assert_eq!(inode.inode_no, EXT4_LOST_FOUND_INODE);
        inode.init_lost_found();
        inode
    }

    /// Inode 11 is not officially reserved for the lost+found directory, but fsck complains if it's not there.
    /// Therefore, the inode returned by the first call to `allocate_inode` should be used for lost+found.
    pub fn allocate_inode(&mut self, is_dir: bool) -> Inode<'a> {
        let inode_no = self.next_free_inode_no;
        let inode_size = self.superblock().s_inode_size as usize;
        self.next_free_inode_no += 1;

        let existing_inode_no = inode_no - FIRST_EXISTING_INODE;
        let (block_group_idx, relative_inode_no) = existing_inode_no.div_rem(&self.superblock().s_inodes_per_group);
        let block_group = &mut self.block_groups[block_group_idx as usize];
        let inner = block_group.allocate_relative_inode(relative_inode_no as usize, inode_size);

        c_add_inode(inode_no, is_dir);

        Inode { inode_no, inner }
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
