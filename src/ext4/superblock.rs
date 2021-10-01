use std::convert::TryFrom;

use anyhow::{bail, Context, Result};
use uuid::Uuid;

use crate::ext4::{BlockIdx, BlockIdx_from, FIRST_BLOCK_PADDING, FIRST_EXISTING_INODE, FIRST_NON_RESERVED_INODE};
use crate::fat::BootSector;
use crate::lohi::{LoHi, LoHiMut};
use crate::util::{exact_log2, u64_from, usize_from};
use crate::Ranges;

pub const ROOT_INODE_NO: u32 = 2;
pub const LOST_FOUND_INODE_NO: u32 = 11;

const SUPERBLOCK_MAGIC: u16 = 61267;
const STATE_CLEANLY_UNMOUNTED: u16 = 1;
const NEWEST_REVISION: u32 = 1;
const BLOCK_SIZE_MIN_LOG2: u32 = 10;
const DESC_SIZE_64BIT: u16 = 64;
const ERRORS_DEFAULT: u16 = 1;
const FEATURE_COMPAT_SPARSE_SUPER2: u32 = 0x200; // use only two superblock backups
const FEATURE_INCOMPAT_EXTENTS: u32 = 0x40; // use extents to represent a file's data blocks
const FEATURE_INCOMPAT_64BIT: u32 = 0x80; // allow filesystems bigger with more than 2^32 blocks
const FEATURE_INCOMPAT_LARGEDIR: u32 = 0x4000; // allow directories bigger than 2GB
const FEATURE_RO_COMPAT_LARGE_FILES: u32 = 0x2; // allow files bigger than 2GiB
const FEATURE_RO_COMPAT_DIR_NLINK: u32 = 0x20; // allow directories with more than 65000 subdirectories
const INODE_RATIO: u32 = 16384;
const INODE_SIZE: u16 = 256;
const VOLUME_NAME_LEN: usize = 16;
// Simplified because we don't use ext4 clusters
const MAX_BLOCKS_PER_GROUP: u32 = (1 << 16) - 8;
// Chosen for practicality, not actually enforced
const MIN_USABLE_BLOCKS_PER_GROUP: usize = 10;
const MIN_BLOCK_SIZE: u32 = 1024;
const MAX_BLOCK_SIZE: u32 = 65_536;

#[derive(PartialEq, Clone, Copy, Debug)]
pub enum HasSuperBlock {
    YesOriginal,
    YesBackup,
    No,
}

#[repr(C)]
#[derive(Copy, Clone)]
pub struct SuperBlock {
    pub s_inodes_count: u32,
    pub s_blocks_count_lo: u32,
    pub s_r_blocks_count_lo: u32,
    pub s_free_blocks_count_lo: u32,
    pub s_free_inodes_count: u32,
    pub s_first_data_block: u32,
    pub s_log_block_size: u32, // max value: 6
    pub s_log_cluster_size: u32,
    pub s_blocks_per_group: u32, // max value: 65528
    pub s_clusters_per_group: u32,
    pub s_inodes_per_group: u32,
    pub s_mtime: u32,
    pub s_wtime: u32,
    pub s_mnt_count: u16,
    pub s_max_mnt_count: u16,
    pub s_magic: u16,
    pub s_state: u16,
    pub s_errors: u16,
    pub s_minor_rev_level: u16,
    pub s_lastcheck: u32,
    pub s_checkinterval: u32,
    pub s_creator_os: u32,
    pub s_rev_level: u32,
    pub s_def_resuid: u16,
    pub s_def_resgid: u16,
    pub s_first_ino: u32,
    /// >= size_of::<InodeInner>()
    pub s_inode_size: u16,
    pub s_block_group_nr: u16,
    /// activated features that don't impact compatibility
    pub s_feature_compat: u32,
    /// activated features that impact compatibility
    pub s_feature_incompat: u32,
    /// activated features that only impact write compatibility
    pub s_feature_ro_compat: u32,
    pub s_uuid: [u8; 16],
    pub s_volume_name: [u8; VOLUME_NAME_LEN],
    pub s_last_mounted: [u8; 64],
    pub s_algorithm_usage_bitmap: u32,
    pub s_prealloc_blocks: u8,
    pub s_prealloc_dir_blocks: u8,
    pub s_reserved_gdt_blocks: u16,
    pub s_journal_uuid: [u8; 16],
    pub s_journal_inum: u32,
    pub s_journal_dev: u32,
    pub s_last_orphan: u32,
    pub s_hash_seed: [u32; 4],
    pub s_def_hash_version: u8,
    pub s_jnl_backup_type: u8,
    pub s_desc_size: u16,
    pub s_default_mount_opts: u32,
    pub s_first_meta_bg: u32,
    pub s_mkfs_time: u32,
    pub s_jnl_blocks: [u32; 17],
    pub s_blocks_count_hi: u32,
    pub s_r_blocks_count_hi: u32,
    pub s_free_blocks_count_hi: u32,
    pub s_min_extra_isize: u16,
    pub s_want_extra_isize: u16,
    pub s_flags: u32,
    pub s_raid_stride: u16,
    pub s_mmp_update_interval: u16,
    pub s_mmp_block: u64,
    pub s_raid_stripe_width: u32,
    pub s_log_groups_per_flex: u8,
    pub s_checksum_type: u8,
    pub s_encryption_level: u8,
    pub s_reserved_pad: u8,
    pub s_kbytes_written: u64,
    pub s_snapshot_inum: u32,
    pub s_snapshot_id: u32,
    pub s_snapshot_r_blocks_count: u64,
    pub s_snapshot_list: u32,
    pub s_error_count: u32,
    pub s_first_error_time: u32,
    pub s_first_error_ino: u32,
    pub s_first_error_block: u64,
    pub s_first_error_func: [u8; 32],
    pub s_first_error_line: u32,
    pub s_last_error_time: u32,
    pub s_last_error_ino: u32,
    pub s_last_error_line: u32,
    pub s_last_error_block: u64,
    pub s_last_error_func: [u8; 32],
    pub s_mount_opts: [u8; 64],
    pub s_usr_quota_inum: u32,
    pub s_grp_quota_inum: u32,
    pub s_overhead_clusters: u32,
    pub s_backup_bgs: [u32; 2],
    pub s_encrypt_algos: [u8; 4],
    pub s_encrypt_pw_salt: [u8; 16],
    pub s_lpf_ino: u32,
    pub s_prj_quota_inum: u32,
    pub s_checksum_seed: u32,
    pub s_reserved: [u32; 98],
    pub s_checksum: u32,
}

impl SuperBlock {
    pub fn from(boot_sector: &BootSector) -> Result<Self> {
        if boot_sector.get_data_range().start % usize_from(boot_sector.cluster_size()) != 0 {
            // We want to treat FAT clusters as ext4 blocks, but we can't if they're not aligned
            bail!(
                "The FAT filesystem's data section must be aligned to its cluster size (for more info, see the -a \
                 option in the mkfs.fat man page).",
            );
        }

        Self::new(boot_sector.fs_size(), boot_sector.cluster_size(), boot_sector.volume_label())
    }

    pub fn new(fs_len: usize, block_size: u32, volume_label: &[u8]) -> Result<Self> {
        assert!(volume_label.len() <= VOLUME_NAME_LEN);

        // SAFETY: This allows us to skip initializing a ton of fields to zero, but
        // CAUTION: some initialization steps rely on other fields already having been set,
        // so pay attention when refactoring/reordering steps.
        let mut sb: Self = unsafe { std::mem::zeroed() };

        if block_size < MIN_BLOCK_SIZE {
            bail!("The FAT filesystem's cluster size must be >= 1 KiB");
        } else if block_size > MAX_BLOCK_SIZE {
            bail!("The FAT filesystem's cluster size must be <= 64 KiB");
        }

        let log_block_size = exact_log2(block_size).context("Invalid FAT cluster size")?;
        sb.s_log_block_size = u32::from(log_block_size) - BLOCK_SIZE_MIN_LOG2;
        // `s_log_block_size` must have a value before this call
        sb.s_first_data_block = if sb.first_block_is_padding() { 1 } else { 0 };
        let block_bitmap_size = block_size * 8;
        sb.s_blocks_per_group = block_bitmap_size.min(MAX_BLOCKS_PER_GROUP);

        // TODO big dirs, prevent too deep extent trees
        sb.s_magic = SUPERBLOCK_MAGIC;
        sb.s_state = STATE_CLEANLY_UNMOUNTED;
        sb.s_feature_compat = FEATURE_COMPAT_SPARSE_SUPER2;
        sb.s_feature_incompat = FEATURE_INCOMPAT_64BIT | FEATURE_INCOMPAT_EXTENTS | FEATURE_INCOMPAT_LARGEDIR;
        sb.s_feature_ro_compat = FEATURE_RO_COMPAT_LARGE_FILES | FEATURE_RO_COMPAT_DIR_NLINK;
        sb.s_desc_size = DESC_SIZE_64BIT;
        sb.s_inode_size = INODE_SIZE;
        sb.s_rev_level = NEWEST_REVISION;
        sb.s_errors = ERRORS_DEFAULT;
        sb.s_first_ino = FIRST_NON_RESERVED_INODE;
        sb.s_max_mnt_count = u16::MAX;
        sb.s_mkfs_time = u32::try_from(chrono::Utc::now().timestamp()).unwrap();
        sb.s_uuid = *Uuid::new_v4().as_bytes();
        sb.s_volume_name[0..volume_label.len()].clone_from_slice(volume_label);

        // These have to have these values even if bigalloc is disabled
        sb.s_log_cluster_size = sb.s_log_block_size;
        sb.s_clusters_per_group = sb.s_blocks_per_group;

        // TODO how do we decide number of inodes? (ipg)
        // This is the same logic as used by mke2fs to determine the inode count
        let inode_bitmap_size = block_size * 8;
        let heuristic_inodes_per_group = sb.s_blocks_per_group * block_size / INODE_RATIO;
        sb.s_inodes_per_group = inode_bitmap_size.min(heuristic_inodes_per_group);

        // Same logic as used in mke2fs: If the last block group would have fewer than 50 data blocks, then reduce the
        // block count and ignore the remaining space
        // For some reason in tests we found that mkfs.ext4 didn't follow this logic and instead set sb.blocks_per_group
        // to a value lower than `block_size` * 8, but this is easier to implement.
        // We use the sparse_super2 logic from mke2fs, meaning that the last block group always has a super block copy.
        let mut block_count = fs_len / usize_from(block_size);
        let mut data_block_count = block_count - usize_from(sb.s_first_data_block);
        // set the intermediate value in `sb` because it is needed by the call to `sb.block_group_overhead`.
        LoHiMut::new(&mut sb.s_blocks_count_lo, &mut sb.s_blocks_count_hi).set(u64_from(block_count));
        let last_group_block_count = data_block_count % usize_from(sb.s_blocks_per_group);

        // `s_reserved_gdt_blocks`, `s_log_block_size`, `s_desc_size`, `s_inodes_per_group`, `s_inode_size`,
        // `s_blocks_per_group`, `s_blocks_count_hi` and `s_blocks_count_lo` must have a value before this call
        if last_group_block_count < sb.block_group_overhead(HasSuperBlock::YesBackup) + MIN_USABLE_BLOCKS_PER_GROUP {
            block_count -= last_group_block_count;
            data_block_count -= last_group_block_count;
            LoHiMut::new(&mut sb.s_blocks_count_lo, &mut sb.s_blocks_count_hi).set(u64_from(block_count));
        }

        if data_block_count == 0 {
            bail!(
                "Filesystem too small, it would have fewer than {} usable blocks.",
                MIN_USABLE_BLOCKS_PER_GROUP
            );
        }

        // Same logic as in mke2fs
        let block_group_count = data_block_count.div_ceil(usize_from(sb.s_blocks_per_group));
        let block_group_count = u32::try_from(block_group_count)
            // This can only happen with absurdly large filesystems in the petabye range
            .context("Filesystem too large, it would have more than 2^32 block groups.")?;
        sb.s_inodes_count = sb.s_inodes_per_group * block_group_count;
        // TODO overflow?

        if block_group_count > 1 {
            sb.s_backup_bgs[0] = 1;
            if block_group_count > 2 {
                sb.s_backup_bgs[1] = block_group_count - 1;
            }
        }
        Ok(sb)
    }

    // TODO test
    // TODO type
    pub fn free_inode_count(&self) -> u32 {
        let reserved_inodes_count = FIRST_NON_RESERVED_INODE - FIRST_EXISTING_INODE;
        self.s_inodes_count - reserved_inodes_count
    }

    pub fn block_group_overhead(&self, has_superblock: HasSuperBlock) -> usize {
        // block bitmap + inode bitmap + inode table
        let default_overhead = 2 + self.inode_table_block_count();
        default_overhead + self.superblock_copy_overhead(has_superblock)
    }

    pub fn superblock_copy_overhead(&self, has_superblock: HasSuperBlock) -> usize {
        match has_superblock {
            HasSuperBlock::YesOriginal | HasSuperBlock::YesBackup => {
                // superblock + group descriptor table
                1 + self.gdt_block_count() + usize::from(self.s_reserved_gdt_blocks)
            }
            HasSuperBlock::No => 0,
        }
    }

    fn gdt_block_count(&self) -> usize {
        let descriptors_per_gdt_block = self.block_size() / u32::from(self.s_desc_size);
        usize_from(self.block_group_count().div_ceil(descriptors_per_gdt_block))
    }

    pub fn inode_table_block_count(&self) -> usize {
        let inode_table_size = usize_from(self.s_inodes_per_group) * usize::from(self.s_inode_size);
        inode_table_size.div_ceil(usize_from(self.block_size()))
    }

    pub fn block_size(&self) -> u32 {
        1 << (self.s_log_block_size + BLOCK_SIZE_MIN_LOG2)
    }

    /// Includes a possible first padding block that does not belong to any block group
    pub fn block_count_with_padding(&self) -> usize {
        let block_count: u64 = LoHi::new(&self.s_blocks_count_lo, &self.s_blocks_count_hi).get();
        usize::try_from(block_count).expect("In `Self::new` the block count fit into a usize")
    }

    pub fn block_count_without_padding(&self) -> usize {
        self.block_count_with_padding() - usize_from(self.s_first_data_block)
    }

    pub fn block_group_count(&self) -> u32 {
        let count = self.block_count_without_padding().div_ceil(usize_from(self.s_blocks_per_group));
        u32::try_from(count).expect("We made sure in `Self::new` that the block group count fits into a u32.")
    }

    pub fn block_group_has_superblock(&self, block_group_idx: u32) -> HasSuperBlock {
        if block_group_idx == 0 {
            HasSuperBlock::YesOriginal
        } else if block_group_idx == self.s_backup_bgs[0] || block_group_idx == self.s_backup_bgs[1] {
            HasSuperBlock::YesBackup
        } else {
            HasSuperBlock::No
        }
    }

    pub fn first_data_block(&self) -> BlockIdx {
        BlockIdx_from(self.s_first_data_block)
    }

    // if the block size is FIRST_BLOCK_PADDING, every block group begins one block later than normal
    pub fn block_group_start_block(&self, block_group_idx: u32) -> BlockIdx {
        usize_from(self.s_blocks_per_group) * usize_from(block_group_idx) + self.first_data_block()
    }

    /// Returns the block ranges that contain filesystem metadata, i.e. the ones occupied by the fields of `BlockGroup`.
    pub fn block_group_overhead_ranges(&self) -> Ranges<BlockIdx> {
        let mut overhead_ranges = Vec::new();
        if self.first_block_is_padding() {
            overhead_ranges.push(0..1);
        }

        let block_group_overhead_ranges = (0..self.block_group_count()).map(|block_group_idx| {
            let has_sb_copy = self.block_group_has_superblock(block_group_idx);
            let overhead = self.block_group_overhead(has_sb_copy);

            let start_block_idx = self.block_group_start_block(block_group_idx);
            start_block_idx..start_block_idx + overhead
        });
        overhead_ranges.extend(block_group_overhead_ranges);
        Ranges::from(overhead_ranges)
    }

    pub fn first_block_is_padding(&self) -> bool {
        usize_from(self.block_size()) <= FIRST_BLOCK_PADDING
    }

    pub fn set_free_blocks_count(&mut self, count: u64) {
        LoHiMut::new(&mut self.s_free_blocks_count_lo, &mut self.s_free_blocks_count_hi).set(count);
    }
}
