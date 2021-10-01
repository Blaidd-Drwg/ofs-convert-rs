use std::ops::Range;
use std::slice;

use crate::bitmap::Bitmap;
use crate::ext4::{
    BlockCount, BlockCount_from, BlockGroupIdx, BlockIdx, BlockSize, Ext4GroupDescriptor, HasSuperBlock, InodeCount,
    InodeInner, SuperBlock, FIRST_BLOCK_PADDING, FIRST_EXISTING_INODE, FIRST_NON_RESERVED_INODE,
};
use crate::util::usize_from;

pub struct BlockGroup<'a> {
    pub superblock: Option<&'a mut SuperBlock>,
    pub gdt: Option<&'a mut [Ext4GroupDescriptor]>,
    pub data_block_bitmap: &'a mut [u8],
    pub inode_bitmap: &'a mut [u8],
    pub inode_table_ptr: *mut u8,
    pub inode_table_len: usize,
}

impl<'a> BlockGroup<'a> {
    pub unsafe fn new(fs_ptr: *mut u8, info: Ext4BlockGroupConstructionInfo) -> Self {
        let start_byte = info.start_block * usize_from(info.block_size);
        let block_group_ptr = fs_ptr.add(start_byte);
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
        match info.superblock_construction_info {
            SuperBlockConstructionInfo::YesOriginal { .. } => {
                let superblock_ptr = if usize_from(info.block_size) == FIRST_BLOCK_PADDING {
                    block_group_ptr as *mut SuperBlock
                } else {
                    block_group_ptr.add(FIRST_BLOCK_PADDING) as *mut SuperBlock
                };
                Some(&mut *superblock_ptr)
            }
            SuperBlockConstructionInfo::YesBackup { .. } => Some(&mut *(block_group_ptr as *mut SuperBlock)),
            SuperBlockConstructionInfo::No => None,
        }
    }

    unsafe fn init_gdt<'b>(
        block_group_ptr: *mut u8,
        info: Ext4BlockGroupConstructionInfo,
    ) -> Option<&'b mut [Ext4GroupDescriptor]> {
        match info.superblock_construction_info {
            SuperBlockConstructionInfo::YesOriginal {
                relative_group_descriptor_start_block,
                group_descriptor_len,
            }
            | SuperBlockConstructionInfo::YesBackup {
                relative_group_descriptor_start_block,
                group_descriptor_len,
            } => {
                let start_byte = relative_group_descriptor_start_block * usize_from(info.block_size);
                let ptr = block_group_ptr.add(start_byte) as *mut Ext4GroupDescriptor;
                Some(slice::from_raw_parts_mut(ptr, group_descriptor_len))
            }
            SuperBlockConstructionInfo::No => None,
        }
    }

    unsafe fn init_data_block_bitmap<'b>(
        block_group_ptr: *mut u8,
        info: Ext4BlockGroupConstructionInfo,
    ) -> &'b mut [u8] {
        let start_byte = info.relative_block_bitmap_block * usize_from(info.block_size);
        let ptr = block_group_ptr.add(start_byte);
        let data_block_bitmap = slice::from_raw_parts_mut(ptr, usize_from(info.block_size));
        data_block_bitmap.fill(0);

        let mut bitmap = Bitmap { data: data_block_bitmap };
        for overhead_block_idx in 0..info.overhead {
            bitmap.set(overhead_block_idx);
        }
        for nonexistent_block_idx in info.blocks_count..bitmap.len() {
            bitmap.set(nonexistent_block_idx);
        }
        data_block_bitmap
    }

    unsafe fn init_inode_bitmap<'b>(block_group_ptr: *mut u8, info: Ext4BlockGroupConstructionInfo) -> &'b mut [u8] {
        let inode_bitmap = Self::blocks_slice(block_group_ptr, info.relative_inode_bitmap_block, info.block_size, 1);
        inode_bitmap.fill(0);

        let mut bitmap = Bitmap { data: inode_bitmap };
        for used_inode_idx in 0..usize_from(info.reserved_inode_count) {
            bitmap.set(used_inode_idx);
        }
        for nonexistent_inode_idx in usize_from(info.inodes_count)..bitmap.len() {
            bitmap.set(nonexistent_inode_idx);
        }
        inode_bitmap
    }

    unsafe fn init_inode_table(block_group_ptr: *mut u8, info: Ext4BlockGroupConstructionInfo) -> (*mut u8, usize) {
        let table = Self::blocks_slice(
            block_group_ptr,
            info.relative_inode_table_start_block,
            info.block_size,
            info.inode_table_block_count,
        );
        table.fill(0);
        (table.as_mut_ptr(), table.len())
    }

    unsafe fn blocks_slice<'b>(
        block_group_ptr: *mut u8,
        relative_block_idx: usize,
        block_size: BlockSize,
        block_count: BlockCount,
    ) -> &'b mut [u8] {
        let start_byte = relative_block_idx * usize_from(block_size);
        let ptr = block_group_ptr.add(start_byte);
        let len = block_count * usize_from(block_size);
        slice::from_raw_parts_mut(ptr, len)
    }

    pub fn mark_relative_range_as_used(&mut self, relative_range: Range<BlockIdx>) {
        let mut bitmap = Bitmap { data: self.data_block_bitmap };
        for block_idx in relative_range {
            bitmap.set(block_idx);
        }
    }

    pub fn allocate_relative_inode(&mut self, relative_inode_no: InodeCount, inode_size: u16) -> &'a mut InodeInner {
        let mut bitmap = Bitmap { data: self.inode_bitmap };
        assert!(
            !bitmap.get(usize_from(relative_inode_no)),
            "Tried to allocate used inode with relative index {}",
            relative_inode_no
        );

        bitmap.set(usize_from(relative_inode_no));
        unsafe { self.get_relative_inode(relative_inode_no, inode_size) }
    }

    /// SAFETY: Undefined behavior if the function is called twice with the same `relative_inode_no`.
    pub unsafe fn get_relative_inode(&mut self, relative_inode_no: InodeCount, inode_size: u16) -> &'a mut InodeInner {
        let offset = usize_from(relative_inode_no) * usize::from(inode_size);
        assert!(offset + usize::from(inode_size) <= self.inode_table_len);
        let ptr = self.inode_table_ptr.add(offset) as *mut InodeInner;
        &mut *ptr
    }
}

#[derive(Clone, Copy, Debug)]
pub enum SuperBlockConstructionInfo {
    YesOriginal {
        relative_group_descriptor_start_block: usize,
        group_descriptor_len: usize,
    },
    YesBackup {
        relative_group_descriptor_start_block: usize,
        group_descriptor_len: usize,
    },
    No,
}

#[derive(Clone, Copy, Debug)]
pub struct Ext4BlockGroupConstructionInfo {
    pub start_block: BlockIdx,
    pub relative_block_bitmap_block: BlockCount,
    pub relative_inode_bitmap_block: BlockCount,
    pub relative_inode_table_start_block: BlockCount,
    pub blocks_count: BlockCount,
    pub inodes_count: InodeCount,
    pub inode_table_block_count: BlockCount,
    pub superblock_construction_info: SuperBlockConstructionInfo,
    pub block_size: BlockSize,
    pub reserved_inode_count: InodeCount,
    pub overhead: BlockCount,
}

impl Ext4BlockGroupConstructionInfo {
    pub fn new(superblock: &SuperBlock, block_group_idx: BlockGroupIdx) -> Self {
        let has_superblock = superblock.block_group_has_superblock(block_group_idx);

        let relative_block_bitmap_block = superblock.superblock_copy_overhead(has_superblock);
        let relative_inode_bitmap_block = relative_block_bitmap_block + 1;
        let relative_inode_table_start_block = relative_inode_bitmap_block + 1;

        let max_block_count = superblock.block_count_without_padding()
            - usize_from(block_group_idx) * usize_from(superblock.s_blocks_per_group);
        let blocks_count = max_block_count.min(BlockCount_from(superblock.s_blocks_per_group));

        let superblock_construction_info = match has_superblock {
            HasSuperBlock::No => SuperBlockConstructionInfo::No,
            HasSuperBlock::YesOriginal => SuperBlockConstructionInfo::YesOriginal {
                relative_group_descriptor_start_block: 1,
                group_descriptor_len: BlockCount_from(superblock.block_group_count()),
            },
            HasSuperBlock::YesBackup => SuperBlockConstructionInfo::YesBackup {
                relative_group_descriptor_start_block: 1,
                group_descriptor_len: BlockCount_from(superblock.block_group_count()),
            },
        };

        Self {
            start_block: superblock.block_group_start_block(block_group_idx),
            relative_block_bitmap_block,
            relative_inode_bitmap_block,
            relative_inode_table_start_block,
            blocks_count,
            inodes_count: superblock.s_inodes_per_group,
            inode_table_block_count: superblock.inode_table_block_count(),
            superblock_construction_info,
            block_size: superblock.block_size(),
            overhead: superblock.block_group_overhead(has_superblock),
            reserved_inode_count: if block_group_idx == 0 {
                FIRST_NON_RESERVED_INODE - FIRST_EXISTING_INODE
            } else {
                0
            },
        }
    }
}
