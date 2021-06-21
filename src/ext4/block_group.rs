use std::ops::Range;
use std::slice;

use crate::bitmap::Bitmap;
use crate::ext4::{Ext4GroupDescriptor, HasSuperBlock, InodeInner, SuperBlock, FIRST_EXISTING_INODE};
use crate::fat::ClusterIdx;

const FIRST_SUPERBLOCK_OFFSET: usize = 1024;
pub const FIRST_NON_RESERVED_INODE: u32 = 11;


pub struct BlockGroup<'a> {
    pub superblock: Option<&'a mut SuperBlock>,
    pub gdt: Option<&'a mut [Ext4GroupDescriptor]>,
    pub data_block_bitmap: &'a mut [u8],
    pub inode_bitmap: &'a mut [u8],
    pub inode_table_ptr: *mut u8,
    pub inode_table_len: usize,
}

impl<'a> BlockGroup<'a> {
    pub unsafe fn new(partition_ptr: *mut u8, info: Ext4BlockGroupConstructionInfo) -> Self {
        let start_byte = info.start_block as usize * info.block_size as usize;
        let block_group_ptr = partition_ptr.add(start_byte);
        let (inode_table_ptr, inode_table_len) = Self::init_inode_table(block_group_ptr, info);

        Self {
            superblock: Self::init_superblock(block_group_ptr, info),
            gdt: Self::init_gdt(block_group_ptr, info),
            data_block_bitmap: Self::init_data_block_bitmap(block_group_ptr, info),
            inode_bitmap: Self::init_inode_bitmap(block_group_ptr, info),
            inode_table_ptr,
            inode_table_len,
        }
    }

    unsafe fn init_superblock<'b>(
        block_group_ptr: *mut u8,
        info: Ext4BlockGroupConstructionInfo,
    ) -> Option<&'b mut SuperBlock> {
        match info.has_superblock {
            HasSuperBlock::YesOriginal => {
                let superblock_ptr = if info.block_size as usize == FIRST_SUPERBLOCK_OFFSET {
                    block_group_ptr as *mut SuperBlock
                } else {
                    block_group_ptr.add(FIRST_SUPERBLOCK_OFFSET) as *mut SuperBlock
                };
                Some(&mut *superblock_ptr)
            }
            HasSuperBlock::YesBackup => Some(&mut *(block_group_ptr as *mut SuperBlock)),
            HasSuperBlock::No => None,
        }
    }

    unsafe fn init_gdt<'b>(
        block_group_ptr: *mut u8,
        info: Ext4BlockGroupConstructionInfo,
    ) -> Option<&'b mut [Ext4GroupDescriptor]> {
        match info.has_superblock {
            HasSuperBlock::YesOriginal | HasSuperBlock::YesBackup => {
                let start_byte = info.relative_group_descriptor_start_block * info.block_size;
                let ptr = block_group_ptr.add(start_byte as usize) as *mut Ext4GroupDescriptor;
                Some(slice::from_raw_parts_mut(ptr, info.group_descriptor_len))
            }
            HasSuperBlock::No => None,
        }
    }

    unsafe fn init_data_block_bitmap<'b>(
        block_group_ptr: *mut u8,
        info: Ext4BlockGroupConstructionInfo,
    ) -> &'b mut [u8] {
        let start_byte = info.relative_block_bitmap_block * info.block_size;
        let ptr = block_group_ptr.add(start_byte as usize);
        let data_block_bitmap = slice::from_raw_parts_mut(ptr, info.block_size as usize);
        data_block_bitmap.fill(0);

        let mut bitmap = Bitmap { data: data_block_bitmap };
        for overhead_block_idx in 0..info.overhead {
            bitmap.set(overhead_block_idx as usize);
        }
        for nonexistent_block_idx in info.blocks_count..bitmap.len() {
            bitmap.set(nonexistent_block_idx);
        }
        data_block_bitmap
    }

    unsafe fn init_inode_bitmap<'b>(block_group_ptr: *mut u8, info: Ext4BlockGroupConstructionInfo) -> &'b mut [u8] {
        let start_byte = info.relative_inode_bitmap_block * info.block_size;
        let ptr = block_group_ptr.add(start_byte as usize);
        let inode_bitmap = slice::from_raw_parts_mut(ptr, info.block_size as usize);
        inode_bitmap.fill(0);

        let mut bitmap = Bitmap { data: inode_bitmap };
        for used_inode_idx in 0..info.used_inode_count {
            bitmap.set(used_inode_idx);
        }
        for nonexistent_inode_idx in info.inodes_count..bitmap.len() {
            bitmap.set(nonexistent_inode_idx);
        }
        inode_bitmap
    }

    unsafe fn init_inode_table(block_group_ptr: *mut u8, info: Ext4BlockGroupConstructionInfo) -> (*mut u8, usize) {
        let start_byte = info.relative_inode_table_start_block * info.block_size;
        let ptr = block_group_ptr.add(start_byte as usize);
        let table = slice::from_raw_parts_mut(ptr, info.block_size as usize * info.inode_table_block_count);
        table.fill(0);
        (table.as_mut_ptr(), table.len())
    }

    pub fn mark_relative_range_as_used(&mut self, relative_range: Range<ClusterIdx>) {
        let mut bitmap = Bitmap { data: self.data_block_bitmap };
        for block_idx in relative_range {
            bitmap.set(block_idx as usize);
        }
    }

    pub fn allocate_relative_inode(&mut self, relative_inode_no: usize, inode_size: usize) -> &'a mut InodeInner {
        let mut bitmap = Bitmap { data: self.inode_bitmap };
        assert!(
            !bitmap.get(relative_inode_no),
            "Tried to allocate used inode with relative index {}",
            relative_inode_no
        );

        bitmap.set(relative_inode_no);
        unsafe { self.get_relative_inode(relative_inode_no, inode_size) }
    }

    /// SAFETY: TODO if somebody has borrow on same inode, undefined behavior
    pub unsafe fn get_relative_inode(&mut self, relative_inode_no: usize, inode_size: usize) -> &'a mut InodeInner {
        let offset = relative_inode_no * inode_size;
        assert!(offset + inode_size <= self.inode_table_len);
        let ptr = self.inode_table_ptr.add(offset) as *mut InodeInner;
        &mut *ptr
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Ext4BlockGroupConstructionInfo {
    pub start_block: u64,
    /// Value undefined if the block group does not have a superblock copy
    pub relative_group_descriptor_start_block: u64,
    pub relative_block_bitmap_block: u64,
    pub relative_inode_bitmap_block: u64,
    pub relative_inode_table_start_block: u64,
    /// Value undefined if the block group does not have a superblock copy
    pub group_descriptor_len: usize,
    pub blocks_count: usize,
    pub inodes_count: usize,
    pub inode_table_block_count: usize,
    pub has_superblock: HasSuperBlock,
    pub block_size: u64,
    pub used_inode_count: usize,
    pub overhead: u64,
}

impl Ext4BlockGroupConstructionInfo {
    pub fn new(superblock: &SuperBlock, block_group_idx: usize) -> Self {
        let has_superblock = superblock.block_group_has_superblock(block_group_idx);

        let relative_block_bitmap_block = superblock.superblock_copy_overhead(has_superblock);
        let relative_inode_bitmap_block = relative_block_bitmap_block + 1;
        let relative_inode_table_start_block = relative_inode_bitmap_block + 1;

        let max_block_count = superblock.block_count_without_padding() as usize
            - (block_group_idx * superblock.s_blocks_per_group as usize);
        let blocks_count = max_block_count.min(superblock.s_blocks_per_group as usize);

        Self {
            start_block: superblock.block_group_start_cluster(block_group_idx) as u64,
            relative_group_descriptor_start_block: 1,
            relative_block_bitmap_block,
            relative_inode_bitmap_block,
            relative_inode_table_start_block,
            group_descriptor_len: superblock.block_group_count() as usize,
            blocks_count,
            inodes_count: superblock.s_inodes_per_group as usize,
            inode_table_block_count: superblock.inode_table_block_count() as usize,
            has_superblock,
            block_size: superblock.block_size(),
            overhead: superblock.block_group_overhead(has_superblock),
            used_inode_count: if block_group_idx == 0 {
                FIRST_NON_RESERVED_INODE as usize - FIRST_EXISTING_INODE as usize
            } else {
                0
            },
        }
    }
}
