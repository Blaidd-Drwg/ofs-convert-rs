use std::ops::Range;

use anyhow::Result;
use num::Integer;

use crate::allocator::Allocator;
use crate::ext4::{
    BlockGroup, Ext4BlockGroupConstructionInfo, Ext4GroupDescriptor, Extent, Inode, SuperBlock, FIRST_EXISTING_INODE,
    FIRST_NON_RESERVED_INODE, LOST_FOUND_INODE_NO, ROOT_INODE_NO,
};
use crate::fat::{BootSector, ClusterIdx};

pub struct Ext4Fs<'a> {
    block_groups: Vec<BlockGroup<'a>>,
    /// Used for allocating inodes
    next_free_inode_no: u32,
}

impl<'a> Ext4Fs<'a> {
    /// SAFETY: Safe if `partition_ptr` is valid for reads for `boot_sector.partition_len()` many bytes, and no memory
    /// belonging to a blocks in `superblock.block_group_overhead_ranges()` is dereferenced for the duration of the
    /// lifetime `'a`.
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

    /// Assumes that `inode` currently has no extents.
    pub fn set_extents(
        &mut self,
        inode: &mut Inode,
        ranges: Vec<Range<ClusterIdx>>,
        allocator: &Allocator<'_>,
    ) -> Result<()> {
        for extent in Self::ranges_to_extents(&ranges) {
            self.register_extent(inode, extent, allocator)?;
        }
        Ok(())
    }

    pub fn ranges_to_extents(ranges: &[Range<ClusterIdx>]) -> Vec<Extent> {
        let mut logical_start = 0;
        let mut extents = Vec::new();
        for mut range in ranges.iter().cloned() {
            while !range.is_empty() {
                let range_len = range.len().min(Extent::max_len());
                let range_first_part = range.start..range.start + range_len as ClusterIdx;
                extents.push(Extent::new(range_first_part, logical_start));
                logical_start += range_len as u32;
                range.start += range_len as u32;
            }
        }
        extents
    }

    pub fn register_extent(&mut self, inode: &mut Inode, extent: Extent, allocator: &Allocator) -> Result<()> {
        self.mark_range_as_used(inode, extent.as_range());

        let additional_blocks = inode.add_extent(extent, allocator)?;
        for block in additional_blocks {
            self.mark_range_as_used(inode, block..block + 1);
        }
        Ok(())
    }

    pub fn block_group_idx_of_block(&self, block_idx: ClusterIdx) -> usize {
        // any block before `s_first_data_block` doesn't belong to any block group
        let data_block_idx = block_idx - self.superblock().s_first_data_block;
        (data_block_idx / self.superblock().s_blocks_per_group) as usize
    }

    pub fn mark_range_as_used(&mut self, inode: &mut Inode, range: Range<ClusterIdx>) {
        inode.increment_used_blocks(range.len(), self.superblock().block_size() as usize);

        let block_group_idx = self.block_group_idx_of_block(range.start);
        assert_eq!(block_group_idx, self.block_group_idx_of_block(range.end - 1));

        self.group_descriptor_table_mut()[block_group_idx].decrement_free_blocks_count(range.len() as u32);

        let group_start_block = self.superblock_mut().block_group_start_cluster(block_group_idx);
        let relative_range = range.start - group_start_block..range.end - group_start_block;
        self.block_groups[block_group_idx].mark_relative_range_as_used(relative_range);
    }

    pub unsafe fn build_root_inode(&mut self) -> Inode<'a> {
        let inode_no = ROOT_INODE_NO;
        let existing_inode_no = inode_no - FIRST_EXISTING_INODE;
        let inode_size = self.superblock().s_inode_size as usize;
        let inner = self.block_groups[0].get_relative_inode(existing_inode_no as usize, inode_size);
        let mut inode = Inode { inode_no, inner };
        inode.init_root();

        let descriptor = &mut self.group_descriptor_table_mut()[0];
        // root inode is reserved and already marked as not free, no need to decrement count
        descriptor.increment_used_directory_count();
        inode
    }

    pub fn build_lost_found_inode(&mut self) -> Inode<'a> {
        let mut inode = self.allocate_inode(true);
        assert_eq!(inode.inode_no, LOST_FOUND_INODE_NO);
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

        let descriptor = &mut self.group_descriptor_table_mut()[block_group_idx as usize];
        descriptor.decrement_free_inode_count();
        if is_dir {
            descriptor.increment_used_directory_count();
        }

        Inode { inode_no, inner }
    }
}

impl Drop for Ext4Fs<'_> {
    fn drop(&mut self) {
        // Fill in sum fields in superblock with data from group descriptors
        self.superblock_mut().s_free_inodes_count = self
            .group_descriptor_table_mut()
            .iter_mut()
            .map(|block_group| block_group.free_inodes_count())
            .sum();
        let free_blocks_count = self
            .group_descriptor_table_mut()
            .iter_mut()
            .map(|block_group| block_group.free_blocks_count() as u64)
            .sum();
        self.superblock_mut().set_free_blocks_count(free_blocks_count);

        // Make superblock and group descriptor table backup copies
        let superblock = *self.superblock_mut();
        let gdt = self.group_descriptor_table_mut().to_vec();
        for backup_group_idx in superblock.s_backup_bgs {
            let block_group = &mut self.block_groups[backup_group_idx as usize];
            (*block_group.superblock.as_deref_mut().unwrap()) = superblock;
            block_group.gdt.as_deref_mut().unwrap().copy_from_slice(&gdt);
        }
    }
}
