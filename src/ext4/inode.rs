use crate::ext4::{Extent, ExtentHeader};
use crate::lohi::LoHiMut;

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct Inode {
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
    pub ext_header: ExtentHeader,
    pub extents: [Extent; 4],
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

impl Inode {
    pub fn increment_size(&mut self, size: u64) {
        let mut current_size = LoHiMut::new(&mut self.i_size_lo, &mut self.i_size_high);
        current_size += size;
    }

    pub fn set_size(&mut self, size: u64) {
        LoHiMut::new(&mut self.i_size_lo, &mut self.i_size_high).set(size);
    }

    pub fn increment_link_count(&mut self) {
        self.i_links_count += 1;
    }
}
