use crate::ext4::Ext4BlockGroupConstructionInfo;
use crate::lohi::{LoHi, LoHiMut};

#[repr(C)]
#[derive(Debug, Copy, Clone, Default)]
pub struct Ext4GroupDescriptor {
    pub bg_block_bitmap_lo: u32,
    pub bg_inode_bitmap_lo: u32,
    pub bg_inode_table_lo: u32,
    pub bg_free_blocks_count_lo: u16,
    pub bg_free_inodes_count_lo: u16,
    pub bg_used_dirs_count_lo: u16,
    pub bg_flags: u16,
    pub bg_exclude_bitmap_lo: u32,
    pub bg_block_bitmap_csum_lo: u16,
    pub bg_inode_bitmap_csum_lo: u16,
    pub bg_itable_unused_lo: u16,
    pub bg_checksum: u16,
    pub bg_block_bitmap_hi: u32,
    pub bg_inode_bitmap_hi: u32,
    pub bg_inode_table_hi: u32,
    pub bg_free_blocks_count_hi: u16,
    pub bg_free_inodes_count_hi: u16,
    pub bg_used_dirs_count_hi: u16,
    pub bg_itable_unused_hi: u16,
    pub bg_exclude_bitmap_hi: u32,
    pub bg_block_bitmap_csum_hi: u16,
    pub bg_inode_bitmap_csum_hi: u16,
    pub bg_reserved: u32,
}

impl Ext4GroupDescriptor {
    pub fn new(info: Ext4BlockGroupConstructionInfo) -> Self {
        let block_bitmap_block = info.start_block + info.relative_block_bitmap_block;
        let inode_bitmap_block = info.start_block + info.relative_inode_bitmap_block;
        let inode_table_start_block = info.start_block + info.relative_inode_table_start_block;
        let free_inodes_count = info.inodes_count - info.used_inode_count;
        let free_blocks_count = info.blocks_count as u64 - info.overhead;

        let mut instance = Self::default();
        LoHiMut::new(&mut instance.bg_block_bitmap_lo, &mut instance.bg_block_bitmap_hi).set(block_bitmap_block);
        LoHiMut::new(&mut instance.bg_inode_bitmap_lo, &mut instance.bg_inode_bitmap_hi).set(inode_bitmap_block);
        LoHiMut::new(&mut instance.bg_inode_table_lo, &mut instance.bg_inode_table_hi).set(inode_table_start_block);
        LoHiMut::new(&mut instance.bg_free_inodes_count_lo, &mut instance.bg_free_inodes_count_hi)
            .set(free_inodes_count as u32);
        LoHiMut::new(&mut instance.bg_free_blocks_count_lo, &mut instance.bg_free_blocks_count_hi)
            .set(free_blocks_count as u32);
        instance
    }

    pub fn free_inodes_count(&self) -> u32 {
        LoHi::new(&self.bg_free_inodes_count_lo, &self.bg_free_inodes_count_hi).get()
    }

    pub fn free_blocks_count(&self) -> u32 {
        LoHi::new(&self.bg_free_blocks_count_lo, &self.bg_free_blocks_count_hi).get()
    }

    pub fn decrement_free_blocks_count(&mut self, count: u32) {
        let mut free_blocks = LoHiMut::new(&mut self.bg_free_blocks_count_lo, &mut self.bg_free_blocks_count_hi);
        free_blocks -= count;
    }
}
