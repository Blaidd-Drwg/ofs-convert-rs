use std::mem::{size_of, MaybeUninit};
use std::ops::Range;

use crate::bitmap::Bitmap;
use crate::ext4::{
    BlockCount, BlockGroupIdx, BlockIdx, BlockSize, Ext4GroupDescriptor, HasSuperBlock, InodeCount, InodeInner,
    SuperBlock, FIRST_EXISTING_INODE, SPECIAL_INODES,
};
use crate::util::{AddUsize, FromU32};

pub struct BlockGroup<'a> {
    pub superblock: Option<&'a mut MaybeUninit<SuperBlock>>,
    pub gdt: Option<&'a mut [MaybeUninit<Ext4GroupDescriptor>]>,
    pub data_block_bitmap: Bitmap<'a>,
    pub inode_bitmap: Bitmap<'a>,
    pub inode_table_ptr: *mut u8,
    pub inode_table_len: usize,
}

impl<'a> BlockGroup<'a> {
    /// PANICS: Panics if `block_group_metadata.len() != info.overhead * info.block_size`.
    pub fn new(mut block_group_metadata: &'a mut [u8], info: Ext4BlockGroupConstructionInfo) -> Self {
        let remaining_blocks = &mut block_group_metadata;
        let superblock = Self::init_superblock(remaining_blocks, info);
        let gdt = Self::init_gdt(remaining_blocks, info);
        let data_block_bitmap = Self::init_data_block_bitmap(remaining_blocks, info);
        let inode_bitmap = Self::init_inode_bitmap(remaining_blocks, info);
        let (inode_table_ptr, inode_table_len) = Self::init_inode_table(remaining_blocks, info);
        assert!(remaining_blocks.is_empty());

        Self {
            superblock,
            gdt,
            data_block_bitmap,
            inode_bitmap,
            inode_table_ptr,
            inode_table_len,
        }
    }

    fn init_superblock<'b>(
        block_group_metadata: &'b mut &'a mut [u8],
        info: Ext4BlockGroupConstructionInfo,
    ) -> Option<&'a mut MaybeUninit<SuperBlock>> {
        match info.superblock_construction_info {
            SuperBlockConstructionInfo::Yes { superblock_start_byte, .. } => {
                let metadata_blocks = std::mem::take(block_group_metadata);
                let (block_containing_superblock, remaining_blocks) =
                    Self::split_at_block_mut(metadata_blocks, 1, info);
                *block_group_metadata = remaining_blocks;
                // SAFETY: Safe because we warn the compiler that it's uninitialized.
                let (before, superblock, _) = unsafe {
                    block_containing_superblock[superblock_start_byte..].align_to_mut::<MaybeUninit<SuperBlock>>()
                };
                assert!(before.is_empty());
                Some(&mut superblock[0])
            }
            SuperBlockConstructionInfo::No => None,
        }
    }

    fn init_gdt<'b>(
        block_group_metadata: &'b mut &'a mut [u8],
        info: Ext4BlockGroupConstructionInfo,
    ) -> Option<&'a mut [MaybeUninit<Ext4GroupDescriptor>]> {
        match info.superblock_construction_info {
            SuperBlockConstructionInfo::Yes { group_descriptor_count, .. } => {
                let gdt_size = size_of::<Ext4GroupDescriptor>() * group_descriptor_count;
                let gdt_blocks_count = gdt_size.div_ceil(usize::fromx(info.block_size));
                let metadata_blocks = std::mem::take(block_group_metadata);
                let (gdt_blocks, remaining_blocks) = Self::split_at_block_mut(metadata_blocks, gdt_blocks_count, info);
                *block_group_metadata = remaining_blocks;
                // SAFETY: Safe because we warn the compiler that it's uninitialized.
                let (before, mut gdt, _) = unsafe { gdt_blocks.align_to_mut::<MaybeUninit<Ext4GroupDescriptor>>() };
                gdt = &mut gdt[..group_descriptor_count];
                assert!(before.is_empty());
                Some(gdt)
            }
            SuperBlockConstructionInfo::No => None,
        }
    }

    fn init_data_block_bitmap<'b>(
        block_group_metadata: &'b mut &'a mut [u8],
        info: Ext4BlockGroupConstructionInfo,
    ) -> Bitmap<'a> {
        let metadata_blocks = std::mem::take(block_group_metadata);
        let (bitmap_bytes, remaining_blocks) = Self::split_at_block_mut(metadata_blocks, 1, info);
        *block_group_metadata = remaining_blocks;

        let mut bitmap = Bitmap { data: bitmap_bytes };
        bitmap.clear_all();
        for overhead_block_idx in 0..info.overhead {
            bitmap.set(overhead_block_idx);
        }
        for nonexistent_block_idx in info.blocks_count..bitmap.len() {
            bitmap.set(nonexistent_block_idx);
        }
        bitmap
    }

    fn init_inode_bitmap<'b>(
        block_group_metadata: &'b mut &'a mut [u8],
        info: Ext4BlockGroupConstructionInfo,
    ) -> Bitmap<'a> {
        let metadata_blocks = std::mem::take(block_group_metadata);
        let (bitmap_bytes, remaining_blocks) = Self::split_at_block_mut(metadata_blocks, 1, info);
        *block_group_metadata = remaining_blocks;

        let mut bitmap = Bitmap { data: bitmap_bytes };
        bitmap.clear_all();
        if info.is_first_block_group {
            Self::mark_special_inodes_as_used(&mut bitmap);
        }
        for nonexistent_inode_idx in usize::fromx(info.inodes_count)..bitmap.len() {
            bitmap.set(nonexistent_inode_idx);
        }
        bitmap
    }

    fn mark_special_inodes_as_used(inode_bitmap: &mut Bitmap) {
        for used_inode_idx in SPECIAL_INODES {
            inode_bitmap.set(usize::fromx(used_inode_idx - FIRST_EXISTING_INODE));
        }
    }

    fn init_inode_table<'b>(
        block_group_metadata: &'b mut &'a mut [u8],
        info: Ext4BlockGroupConstructionInfo,
    ) -> (*mut u8, usize) {
        let metadata_blocks = std::mem::take(block_group_metadata);
        let (table, remaining_blocks) = Self::split_at_block_mut(metadata_blocks, info.inode_table_block_count, info);
        *block_group_metadata = remaining_blocks;
        table.fill(0);
        (table.as_mut_ptr(), table.len())
    }

    fn split_at_block_mut(
        slice: &mut [u8],
        mid: BlockCount,
        info: Ext4BlockGroupConstructionInfo,
    ) -> (&mut [u8], &mut [u8]) {
        let mid_byte = mid * usize::fromx(info.block_size);
        slice.split_at_mut(mid_byte)
    }

    pub fn mark_relative_range_as_used(&mut self, relative_range: Range<BlockIdx>) {
        for block_idx in relative_range {
            self.data_block_bitmap.set(block_idx);
        }
    }

    /// PANICS: Panics if `relative_inode_no` is already allocated or is out of bounds.
    pub fn allocate_relative_inode(&mut self, relative_inode_no: InodeCount, inode_size: u16) -> &'a mut InodeInner {
        assert!(
            !self.inode_bitmap.get(usize::fromx(relative_inode_no)),
            "Tried to allocate already used inode with relative index {}",
            relative_inode_no
        );

        self.inode_bitmap.set(usize::fromx(relative_inode_no));
        // SAFETY: Safe since the bitmap ensures we don't use the same `relative_inode_no` twice.
        unsafe { self.get_relative_inode(relative_inode_no, inode_size) }
    }

    /// SAFETY: Undefined behavior if the function is called twice with the same `relative_inode_no`.
    unsafe fn get_relative_inode(&mut self, relative_inode_no: InodeCount, inode_size: u16) -> &'a mut InodeInner {
        let offset = usize::fromx(relative_inode_no) * usize::from(inode_size);
        assert!(offset + usize::from(inode_size) <= self.inode_table_len);
        // SAFETY: safe because the inode is within the partition.
        let ptr = unsafe { self.inode_table_ptr.add_usize(offset) as *mut InodeInner };
        // SAFETY: safe because we have exclusive access to that inode and because its memory was initialized with
        // zeroes.
        unsafe { &mut *ptr }
    }
}

#[derive(Clone, Copy, Debug)]
pub enum SuperBlockConstructionInfo {
    Yes {
        group_descriptor_count: usize,
        superblock_start_byte: usize,
    },
    No,
}

#[derive(Clone, Copy, Debug)]
pub struct Ext4BlockGroupConstructionInfo {
    pub start_block: BlockIdx,
    pub block_bitmap_block: BlockCount,
    pub inode_bitmap_block: BlockCount,
    pub inode_table_start_block: BlockCount,
    pub blocks_count: BlockCount,
    pub inodes_count: InodeCount,
    pub inode_table_block_count: BlockCount,
    pub superblock_construction_info: SuperBlockConstructionInfo,
    pub block_size: BlockSize,
    pub is_first_block_group: bool,
    pub overhead: BlockCount,
}

impl Ext4BlockGroupConstructionInfo {
    pub fn new(superblock: &SuperBlock, block_group_idx: BlockGroupIdx) -> Self {
        let has_superblock = superblock.block_group_has_superblock(block_group_idx);

        let start_block = superblock.block_group_start_block(block_group_idx);
        let block_bitmap_block = start_block + superblock.superblock_copy_overhead(has_superblock);
        let inode_bitmap_block = block_bitmap_block + 1;
        let inode_table_start_block = inode_bitmap_block + 1;

        let max_block_count = superblock.block_count_without_padding()
            - usize::fromx(block_group_idx) * usize::fromx(superblock.s_blocks_per_group);
        let blocks_count = max_block_count.min(BlockCount::fromx(superblock.s_blocks_per_group));

        let superblock_construction_info = match has_superblock {
            HasSuperBlock::No => SuperBlockConstructionInfo::No,
            HasSuperBlock::YesOriginal => SuperBlockConstructionInfo::Yes {
                superblock_start_byte: superblock.start_byte_within_block(),
                group_descriptor_count: BlockCount::fromx(superblock.block_group_count()),
            },
            HasSuperBlock::YesBackup => SuperBlockConstructionInfo::Yes {
                superblock_start_byte: 0,
                group_descriptor_count: BlockCount::fromx(superblock.block_group_count()),
            },
        };

        Self {
            start_block,
            block_bitmap_block,
            inode_bitmap_block,
            inode_table_start_block,
            blocks_count,
            inodes_count: superblock.s_inodes_per_group,
            inode_table_block_count: superblock.inode_table_block_count(),
            superblock_construction_info,
            block_size: superblock.block_size(),
            overhead: superblock.block_group_overhead(has_superblock),
            is_first_block_group: block_group_idx == 0,
        }
    }
}
