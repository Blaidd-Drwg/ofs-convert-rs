use std::convert::TryFrom;
use std::ops::Range;

use anyhow::Result;
use num::Integer;

use crate::allocator::Allocator;
use crate::ext4::{
    BlockGroup, BlockIdx, BlockIdx_from, Ext4BlockGroupConstructionInfo, Ext4GroupDescriptor, Extent, Inode,
    SuperBlock, FIRST_EXISTING_INODE, FIRST_NON_RESERVED_INODE, LOST_FOUND_INODE_NO, ROOT_INODE_NO,
};
use crate::fat::BootSector;
use crate::util::usize_from;

pub struct Ext4Fs<'a> {
    block_groups: Vec<BlockGroup<'a>>,
    /// Used for allocating inodes
    next_free_inode_no: u32,
}

impl<'a> Ext4Fs<'a> {
    /// SAFETY: Safe if `partition_ptr` is valid for reads for `boot_sector.partition_len()` many bytes, and no memory
    /// belonging to a blocks in `superblock.block_group_overhead_ranges()` is dereferenced for the duration of the
    /// lifetime `'a`.
    pub unsafe fn from(partition_ptr: *mut u8, boot_sector: &BootSector) -> Result<Self> {
        let superblock = SuperBlock::from(boot_sector)?;
        let mut block_groups = Vec::new();
        let mut block_group_descriptors = Vec::new();

        for block_group_idx in 0..superblock.block_group_count() {
            let info = Ext4BlockGroupConstructionInfo::new(&superblock, block_group_idx);
            block_group_descriptors.push(Ext4GroupDescriptor::new(info));
            // SAFETY: TODO
            block_groups.push(BlockGroup::new(partition_ptr, info));
        }

        *block_groups[0]
            .superblock
            .as_deref_mut()
            .expect("First ext4 block group has no superblock") = superblock;
        block_groups[0]
            .gdt
            .as_deref_mut()
            .expect("First ext4 block group has no GDT")
            .copy_from_slice(&block_group_descriptors);
        Ok(Self {
            block_groups,
            next_free_inode_no: FIRST_NON_RESERVED_INODE,
        })
    }

    pub fn superblock(&self) -> &SuperBlock {
        self.block_groups[0]
            .superblock
            .as_deref()
            .expect("First ext4 block group has no superblock")
    }

    pub fn superblock_mut(&mut self) -> &mut SuperBlock {
        self.block_groups[0]
            .superblock
            .as_deref_mut()
            .expect("First ext4 block group has no superblock")
    }

    pub fn group_descriptor_table_mut(&mut self) -> &mut [Ext4GroupDescriptor] {
        self.block_groups[0]
            .gdt
            .as_deref_mut()
            .expect("First ext4 block group has no GDT")
    }

    /// Assumes that `inode` currently has no extents.
    pub fn set_extents<I>(&mut self, inode: &mut Inode, ranges: I, allocator: &Allocator<'_>) -> Result<()>
    where I: IntoIterator<Item = Range<BlockIdx>> {
        for extent in Extent::from_ranges(ranges)? {
            self.register_extent(inode, extent, allocator)?;
        }
        Ok(())
    }

    pub fn register_extent(&mut self, inode: &mut Inode, extent: Extent, allocator: &Allocator) -> Result<()> {
        self.mark_range_as_used(inode, extent.as_range());

        let additional_blocks = inode.add_extent(extent, allocator)?;
        for block in additional_blocks {
            self.mark_range_as_used(inode, block..block + 1);
        }
        Ok(())
    }

    pub fn block_group_idx_of_block(&self, block_idx: BlockIdx) -> u32 {
        // any block before `s_first_data_block` doesn't belong to any block group
        let data_block_idx = block_idx - BlockIdx_from(self.superblock().s_first_data_block);
        let bg_idx = data_block_idx / usize_from(self.superblock().s_blocks_per_group);
        u32::try_from(bg_idx).expect("Attempted to compute a block group index that does not fit in a u32")
    }

    /// PANICS: Panics if `range` contains blocks belonging to more than one block group
    pub fn mark_range_as_used(&mut self, inode: &mut Inode, range: Range<BlockIdx>) {
        inode.increment_used_blocks(range.len(), self.superblock().block_size());

        let block_group_idx = self.block_group_idx_of_block(range.start);
        assert_eq!(block_group_idx, self.block_group_idx_of_block(range.end - 1));

        let range_len = u32::try_from(range.len())
            .expect("All blocks belong to the same block group, so their count can't overflow u32");
        self.group_descriptor_table_mut()[usize_from(block_group_idx)].decrement_free_blocks_count(range_len);

        let group_start_block = self.superblock_mut().block_group_start_block(block_group_idx);
        let relative_range = range.start - group_start_block..range.end - group_start_block;
        self.block_groups[usize_from(block_group_idx)].mark_relative_range_as_used(relative_range);
    }

    pub unsafe fn build_root_inode(&mut self) -> Inode<'a> {
        let inode_no = ROOT_INODE_NO;
        let existing_inode_no = inode_no - FIRST_EXISTING_INODE;
        let inode_size = self.superblock().s_inode_size;
        let inner = self.block_groups[0].get_relative_inode(existing_inode_no, inode_size);
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
        let inode_size = self.superblock().s_inode_size;
        self.next_free_inode_no += 1;

        let existing_inode_no = inode_no - FIRST_EXISTING_INODE;
        let (block_group_idx, relative_inode_no) = existing_inode_no.div_rem(&self.superblock().s_inodes_per_group);
        let block_group = &mut self.block_groups[usize_from(block_group_idx)];
        let inner = block_group.allocate_relative_inode(relative_inode_no, inode_size);

        let descriptor = &mut self.group_descriptor_table_mut()[usize_from(block_group_idx)];
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
            .map(|block_group| u64::from(block_group.free_blocks_count()))
            .sum();
        self.superblock_mut().set_free_blocks_count(free_blocks_count);

        // Make superblock and group descriptor table backup copies
        let superblock = *self.superblock_mut();
        let gdt = self.group_descriptor_table_mut().to_vec();
        for backup_group_idx in superblock.s_backup_bgs {
            let block_group = &mut self.block_groups[usize_from(backup_group_idx)];
            (*block_group
                .superblock
                .as_deref_mut()
                .expect("ext4 backup block group has no superblock")) = superblock;
            block_group
                .gdt
                .as_deref_mut()
                .expect("ext4 backup block group has no GDT")
                .copy_from_slice(&gdt);
        }
    }
}
