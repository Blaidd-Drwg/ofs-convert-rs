use crate::allocator::Allocator;
use crate::ext4::{Extent, ExtentTree, ExtentTreeElement, ExtentTreeLevel};
use crate::fat::ClusterIdx;
use crate::lohi::LoHiMut;

pub const EXTENT_ENTRIES_IN_INODE: usize = 5;

pub struct Inode<'a> {
    pub inode_no: u32,
    pub inner: &'a mut InodeInner,
}

#[repr(C)]
#[derive(Copy, Clone)]
pub struct InodeInner {
    pub i_mode: u16,
    pub i_uid: u16,
    pub i_size_lo: u32,
    pub i_atime: u32,
    pub i_ctime: u32,
    pub i_mtime: u32,
    pub i_dtime: u32,
    pub i_gid: u16,
    pub i_links_count: u16,
    pub i_blocks_lo: u32,
    pub i_flags: u32,
    pub l_i_version: u32,
    pub extents: [ExtentTreeElement; EXTENT_ENTRIES_IN_INODE],
    pub i_generation: u32,
    pub i_file_acl_lo: u32,
    pub i_size_high: u32,
    pub i_obso_faddr: u32,
    pub l_i_blocks_high: u16,
    pub l_i_file_acl_high: u16,
    pub l_i_uid_high: u16,
    pub l_i_gid_high: u16,
    pub l_i_checksum_lo: u16,
    pub l_i_reserved: u16,
    pub i_extra_isize: u16,
    pub i_checksum_hi: u16,
    pub i_ctime_extra: u32,
    pub i_mtime_extra: u32,
    pub i_atime_extra: u32,
    pub i_crtime: u32,
    pub i_crtime_extra: u32,
    pub i_version_hi: u32,
    pub i_projid: u32,
}

impl<'a> Inode<'a> {
    pub fn increment_size(&mut self, size: u64) {
        let mut current_size = LoHiMut::new(&mut self.inner.i_size_lo, &mut self.inner.i_size_high);
        current_size += size;
    }

    pub fn set_size(&mut self, size: u64) {
        LoHiMut::new(&mut self.inner.i_size_lo, &mut self.inner.i_size_high).set(size);
    }

    pub fn increment_link_count(&mut self) {
        self.inner.i_links_count += 1;
    }

    pub fn add_extent(&mut self, extent: Extent, allocator: &Allocator<'_>) -> Vec<ClusterIdx> {
        self.extent_tree(allocator).add_extent(extent)
    }

    fn extent_tree<'b>(&'b mut self, allocator: &'b Allocator<'b>) -> ExtentTree<'b> {
        // SAFETY: TODO
        unsafe {
            let root_level = ExtentTreeLevel::new(&mut self.inner.extents);
            ExtentTree::new(root_level, allocator)
        }
    }

    pub fn increment_used_blocks(&mut self, block_count: usize, block_size: usize) {
        // number of 512-byte blocks allocated
        let mini_block_count = block_count * block_size / 512;
        let mut current_mini_block_count = LoHiMut::new(&mut self.inner.i_blocks_lo, &mut self.inner.l_i_blocks_high);
        current_mini_block_count += mini_block_count as u64;
    }
}
