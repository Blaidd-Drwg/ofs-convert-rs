use std::convert::TryFrom;

use anyhow::Result;
use chrono::prelude::*;
use nix::unistd::{getegid, geteuid};

use crate::allocator::Allocator;
use crate::ext4::{
    BlockCount, BlockIdx, BlockSize, Extent, ExtentHeader, ExtentTree, ExtentTreeElement, ExtentTreeLevel, InodeNo,
};
use crate::lohi::LoHiMut;
use crate::serialization::DentryRepresentation;
use crate::util::FromUsize;

pub const EXTENT_ENTRIES_IN_INODE: u16 = 5;
pub const EXT2_LINK_MAX: u16 = 65_000;
pub const NON_REPRESENTABLE_LINK_COUNT: u16 = 1;

// i_flags
const INODE_USES_EXTENTS: u32 = 0x00080000;

// i_mode
const DIR_FLAG: u16 = 0o040_000;
const REG_FLAG: u16 = 0o100_000;
const READ_USER: u16 = 0o000_400;
const READ_GROUP: u16 = 0o000_040;
const READ_OTHERS: u16 = 0o000_004;
const WRITE_USER: u16 = 0o000_200;
const WRITE_GROUP: u16 = 0o000_020;
const WRITE_OTHERS: u16 = 0o000_002;
const EXECUTE_USER: u16 = 0o000_100;
const EXECUTE_GROUP: u16 = 0o000_010;
const EXECUTE_OTHERS: u16 = 0o000_001;
const NO_WRITE_PERMS: u16 = READ_USER | READ_GROUP | READ_OTHERS | EXECUTE_USER | EXECUTE_GROUP | EXECUTE_OTHERS;
const DEFAULT_PERMS: u16 = NO_WRITE_PERMS | WRITE_USER;

pub struct Inode<'a> {
    pub inode_no: InodeNo,
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
    pub extents: [ExtentTreeElement; EXTENT_ENTRIES_IN_INODE as usize],
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
    pub fn init_from_dentry(&mut self, dentry: DentryRepresentation) {
        self.inner.init_from_dentry(dentry);
    }

    pub fn init_lost_found(&mut self) {
        self.inner.init_lost_found();
    }
    pub fn init_root(&mut self) {
        self.inner.init_root();
    }

    pub fn increment_size(&mut self, size: u64) {
        let mut current_size = LoHiMut::new(&mut self.inner.i_size_lo, &mut self.inner.i_size_high);
        current_size += size;
    }

    pub fn set_size(&mut self, size: u64) {
        LoHiMut::new(&mut self.inner.i_size_lo, &mut self.inner.i_size_high).set(size);
    }

    pub fn set_link_count_from_subdirs(&mut self, mut link_count: u64) {
        link_count += u64::from(self.inner.i_links_count);
        let representable_link_count = u16::try_from(link_count).ok().and_then(|link_count| {
            if link_count <= EXT2_LINK_MAX {
                Some(link_count)
            } else {
                None
            }
        });

        if let Some(link_count) = representable_link_count {
            self.inner.i_links_count = link_count;
        } else {
            debug_assert!(self.inner.is_dir());
            self.inner.i_links_count = NON_REPRESENTABLE_LINK_COUNT;
        }
    }

    pub fn add_extent(&mut self, extent: Extent, allocator: &Allocator<'_>) -> Result<Vec<BlockIdx>> {
        self.extent_tree(allocator).add_extent(extent)
    }

    fn extent_tree<'b>(&'b mut self, allocator: &'b Allocator<'b>) -> ExtentTree<'b> {
        // SAFETY: Safe because the extent tree in is consistent when `self.inner` is initialized, and is kept
        // consistent when adding extents via `ExtentTree::add_extent`.
        let root_level = unsafe { ExtentTreeLevel::new(&mut self.inner.extents) };
        ExtentTree::new(root_level, allocator)
    }

    pub fn increment_used_blocks(&mut self, block_count: BlockCount, block_size: BlockSize) {
        // number of 512-byte blocks allocated
        let mini_block_count = u64::fromx(block_count) * (u64::from(block_size) / 512);
        let mut current_mini_block_count = LoHiMut::new(&mut self.inner.i_blocks_lo, &mut self.inner.l_i_blocks_high);
        current_mini_block_count += mini_block_count;
    }
}

impl InodeInner {
    fn init_from_dentry(&mut self, dentry: DentryRepresentation) {
        let user_id = u32::from(geteuid());
        let group_id = u32::from(getegid());
        LoHiMut::new(&mut self.i_uid, &mut self.l_i_uid_high).set(user_id);
        LoHiMut::new(&mut self.i_gid, &mut self.l_i_gid_high).set(group_id);
        self.i_mode = Self::mode_from_dentry(&dentry);
        self.i_crtime = dentry.create_time;
        self.i_atime = dentry.access_time;
        self.i_mtime = dentry.mod_time;
        self.i_ctime = self.i_mtime + 1; // mimic behavior of the Linux FAT driver
        self.i_links_count = 1;
        self.i_flags = INODE_USES_EXTENTS;
        self.init_extent_header();
    }

    fn init_lost_found(&mut self) {
        const ROOT_USER_ID: u32 = 0;
        const ROOT_GROUP_ID: u32 = 0;

        let now = u32::try_from(Utc::now().timestamp()).unwrap();
        LoHiMut::new(&mut self.i_uid, &mut self.l_i_uid_high).set(ROOT_USER_ID);
        LoHiMut::new(&mut self.i_gid, &mut self.l_i_gid_high).set(ROOT_GROUP_ID);
        self.i_mode = DEFAULT_PERMS | DIR_FLAG;
        self.i_crtime = 0;
        self.i_atime = now;
        self.i_mtime = now;
        self.i_ctime = now;
        self.i_links_count = 1;
        self.i_flags = INODE_USES_EXTENTS;
        self.init_extent_header();
    }

    fn init_root(&mut self) {
        let now = u32::try_from(Utc::now().timestamp()).unwrap();
        let user_id = u32::from(geteuid());
        let group_id = u32::from(getegid());
        LoHiMut::new(&mut self.i_uid, &mut self.l_i_uid_high).set(user_id);
        LoHiMut::new(&mut self.i_gid, &mut self.l_i_gid_high).set(group_id);
        self.i_mode = DEFAULT_PERMS | DIR_FLAG;
        self.i_crtime = 0;
        self.i_atime = now;
        self.i_mtime = now;
        self.i_ctime = now;
        self.i_links_count = 0;
        self.i_flags = INODE_USES_EXTENTS;
        self.init_extent_header();
    }

    fn init_extent_header(&mut self) {
        self.extents[0].header = ExtentHeader::new(EXTENT_ENTRIES_IN_INODE);
    }

    fn is_dir(&self) -> bool {
        self.i_mode & DIR_FLAG != 0
    }

    fn mode_from_dentry(dentry: &DentryRepresentation) -> u16 {
        let rwx = if dentry.is_read_only { NO_WRITE_PERMS } else { DEFAULT_PERMS };
        let dir = if dentry.is_dir { DIR_FLAG } else { REG_FLAG };
        rwx | dir
    }
}
