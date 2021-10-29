// See https://github.com/rust-lang/rust-clippy/issues/7846
#![allow(clippy::needless_option_as_deref)]

use std::convert::TryFrom;
use std::mem::MaybeUninit;
use std::ops::Range;

use anyhow::{bail, Result};
use num::Integer;

use crate::allocator::Allocator;
use crate::ext4::{
    BlockGroup, BlockGroupIdx, BlockIdx, Ext4BlockGroupConstructionInfo, Ext4GroupDescriptor, Extent, Inode, InodeNo,
    SuperBlock, FIRST_EXISTING_INODE, FIRST_NON_RESERVED_INODE, LOST_FOUND_INODE_NO, ROOT_INODE_NO,
};
use crate::fat::BootSector;
use crate::util::{AddUsize, FromU32};

pub struct Ext4Fs<'a> {
    block_groups: Vec<BlockGroup<'a>>,
    /// Used for allocating inodes
    last_allocated_inode_no: InodeNo,
}

impl<'a> Ext4Fs<'a> {
    /// SAFETY: Safe if `partition_ptr` is valid for reads for `boot_sector.partition_len()` many bytes, and no memory
    /// belonging to a block in `SuperBlock::from(boot_sector).block_group_overhead_ranges()` is dereferenced for the
    /// duration of the lifetime `'a` by someone other than `self`.
    pub unsafe fn from(partition_ptr: *mut u8, boot_sector: &BootSector) -> Result<Self> {
        let superblock = SuperBlock::from(boot_sector)?;
        let mut block_groups = Vec::new();
        let mut block_group_descriptors = Vec::new();

        for block_group_idx in 0..superblock.block_group_count() {
            let info = Ext4BlockGroupConstructionInfo::new(&superblock, block_group_idx);
            block_group_descriptors.push(Ext4GroupDescriptor::new(info));
            // SAFETY: safe because the block group is within the partition.
            let block_group_ptr = unsafe { partition_ptr.add_usize(info.start_block * usize::fromx(info.block_size)) };
            let metadata_len = usize::fromx(superblock.block_size())
                * superblock.block_group_overhead(superblock.block_group_has_superblock(block_group_idx));
            // SAFETY: safe because the memory is valid and we have exclusive access for the duration of `'a`
            let metadata = unsafe { std::slice::from_raw_parts_mut(block_group_ptr, metadata_len) };
            block_groups.push(BlockGroup::new(metadata, info));
        }

        block_groups[0]
            .superblock
            .as_deref_mut()
            .expect("First ext4 block group has no superblock")
            .write(superblock);
        MaybeUninit::write_slice(
            block_groups[0].gdt.as_deref_mut().expect("First ext4 block group has no GDT"),
            &block_group_descriptors,
        );
        Ok(Self {
            block_groups,
            last_allocated_inode_no: FIRST_NON_RESERVED_INODE - 1,
        })
    }

    fn superblock(&self) -> &SuperBlock {
        // SAFETY: safe because we initialized the superblock in `from`
        unsafe {
            self.block_groups[0]
                .superblock
                .as_deref()
                .expect("First ext4 block group has no superblock")
                .assume_init_ref()
        }
    }

    fn superblock_mut(&mut self) -> &mut SuperBlock {
        // SAFETY: safe because we initialized the superblock in `from`
        unsafe {
            self.block_groups[0]
                .superblock
                .as_deref_mut()
                .expect("First ext4 block group has no superblock")
                .assume_init_mut()
        }
    }

    fn group_descriptor_table_mut(&mut self) -> &mut [Ext4GroupDescriptor] {
        let table = self.block_groups[0]
            .gdt
            .as_deref_mut()
            .expect("First ext4 block group has no GDT");
        // SAFETY: safe because we initialized the gdt in `from`
        unsafe { MaybeUninit::slice_assume_init_mut(table) }
    }

    /// Assumes that `inode` currently has no extents.
    pub fn set_extents<I>(&mut self, inode: &mut Inode, data_ranges: I, allocator: &Allocator<'_>) -> Result<()>
    where I: IntoIterator<Item = Range<BlockIdx>> {
        for extent in Extent::from_ranges(data_ranges)? {
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

    /// Returns None if the block belong to no block group. That is the case if `block_idx` is the padding block at the
    /// start of the filesystem, or if it is beyond the end of the last block group.
    pub fn block_group_idx_of_block(&self, block_idx: BlockIdx) -> Option<BlockGroupIdx> {
        // any block before `s_first_data_block` doesn't belong to any block group
        let data_block_idx = block_idx.checked_sub(self.superblock().first_usable_block())?;
        let bg_idx = data_block_idx / usize::fromx(self.superblock().s_blocks_per_group);
        BlockGroupIdx::try_from(bg_idx).ok()
    }

    /// PANICS: Panics if `range` contains blocks belonging to more than one block group
    pub fn mark_range_as_used(&mut self, inode: &mut Inode, range: Range<BlockIdx>) {
        let block_group_idx = self
            .block_group_idx_of_block(range.start)
            .expect("Attempted to mark an unusable block as used");
        let end_block_group_idx = self
            .block_group_idx_of_block(range.end - 1)
            .expect("Attempted to mark an unusable block as used");
        assert_eq!(
            block_group_idx, end_block_group_idx,
            "Attempted to mark a range of blocks from different block groups as used"
        );

        let range_len = u32::try_from(range.len())
            .expect("All blocks belong to the same block group, which has at most u32::MAX blocks");
        self.group_descriptor_table_mut()[usize::fromx(block_group_idx)].decrement_free_blocks_count(range_len);
        inode.increment_used_blocks(range.len(), self.superblock().block_size());

        let group_start_block = self.superblock().block_group_start_block(block_group_idx);
        let relative_range = range.start - group_start_block..range.end - group_start_block;
        self.block_groups[usize::fromx(block_group_idx)].mark_relative_range_as_used(relative_range);
    }

    /// PANICS: Panics if called multiple times
    pub fn build_root_inode(&mut self) -> Inode<'a> {
        let mut inode = self.allocate_inode_with_no(ROOT_INODE_NO, true);
        inode.init_root();
        inode
    }

    /// PANICS: Panics if called multiple times
    pub fn build_lost_found_inode(&mut self) -> Result<Inode<'a>> {
        let mut inode = self.allocate_inode(true)?;
        assert_eq!(inode.inode_no, LOST_FOUND_INODE_NO);
        inode.init_lost_found();
        Ok(inode)
    }

    /// Inode 11 is not officially reserved for the lost+found directory, but fsck complains if it's not there.
    /// Therefore, the inode returned by the first call to `allocate_inode` should be used for lost+found.
    pub fn allocate_inode(&mut self, is_dir: bool) -> Result<Inode<'a>> {
        let inode_no = self.last_allocated_inode_no.checked_add(1);
        match inode_no.filter(|&inode_no| inode_no <= self.superblock().max_inode_no()) {
            Some(inode_no) => {
                self.last_allocated_inode_no = inode_no;
                Ok(self.allocate_inode_with_no(inode_no, is_dir))
            },
            None => bail!("No free inodes left")
        }
    }

    /// PANICS: Panics if an inode with number `inode_no` was already allocated or does not exist.
    fn allocate_inode_with_no(&mut self, inode_no: InodeNo, is_dir: bool) -> Inode<'a> {
        let inode_size = self.superblock().s_inode_size;
        let existing_inode_no = inode_no - FIRST_EXISTING_INODE;
        let (block_group_idx, relative_inode_no) = existing_inode_no.div_rem(&self.superblock().s_inodes_per_group);

        let block_group = &mut self.block_groups[usize::fromx(block_group_idx)];
        let inner = block_group.allocate_relative_inode(relative_inode_no, inode_size);

        let descriptor = &mut self.group_descriptor_table_mut()[usize::fromx(block_group_idx)];
        descriptor.decrement_free_inode_count();
        if is_dir {
            descriptor.increment_used_directory_count();
        }

        Inode { inode_no, inner }
    }

    fn update_superblock(&mut self) {
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
    }

    fn backup_superblock_and_gdt(&mut self) {
        let superblock = *self.superblock();
        let gdt = self.group_descriptor_table_mut().to_vec();

        for backup_group_idx in superblock.backup_bgs() {
            let block_group = &mut self.block_groups[usize::fromx(backup_group_idx)];

            block_group
                .superblock
                .as_deref_mut()
                .expect("ext4 backup block group has no superblock")
                .write(superblock);

            let gdt_backup = block_group.gdt.as_deref_mut().expect("ext4 backup block group has no GDT");
            MaybeUninit::write_slice(gdt_backup, &gdt);
        }
    }
}

impl Drop for Ext4Fs<'_> {
    fn drop(&mut self) {
        self.update_superblock();
        self.backup_superblock_and_gdt();

        // Manually drop `MaybeUninit`s
        let mut block_groups_with_superblocks = vec![0];
        block_groups_with_superblocks.extend(self.superblock().backup_bgs());
        for block_group_idx in block_groups_with_superblocks {
            let block_group = &mut self.block_groups[usize::fromx(block_group_idx)];
            // SAFETY: safe because block group 0 was already initialized and we initialized the backups in
            // `backup_superblock_and_gdt`.
            unsafe {
                block_group
                    .superblock
                    .as_deref_mut()
                    .expect("Backup block group has no superblock")
                    .assume_init_drop();
                let gdt = block_group.gdt.as_deref_mut().expect("Backup block group has no gdt");
                for descriptor in gdt {
                    descriptor.assume_init_drop();
                }
            }
        }
    }
}
