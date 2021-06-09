use crate::fat::{BootSector, ClusterIdx};
use crate::lohi::{LoHiMut, LoHi};
use std::convert::TryFrom;
use std::io;
use std::ops::Range;
use num::Integer;
use uuid::Uuid;

const EXT4_ROOT_INODE: u32 = 2;
const EXT4_LOST_FOUND_INODE: u32 = 11;
const EXT4_FIRST_NON_RSV_INODE: u32 = 11;
const EXT4_MAGIC: u16 = 61267;
const EXT4_STATE_CLEANLY_UNMOUNTED: u16 = 1;
const EXT4_DYNAMIC_REV: u32 = 1;
const EXT4_BLOCK_SIZE_MIN_LOG2: u32 = 10;
const EXT4_64BIT_DESC_SIZE: u16 = 64;
const EXT4_ERRORS_DEFAULT: u16 = 1;
const EXT4_FEATURE_COMPAT_SPARSE_SUPER2: u32 = 512;
const EXT4_FEATURE_INCOMPAT_EXTENTS: u32 = 64;
const EXT4_FEATURE_INCOMPAT_64BIT: u32 = 128;
const EXT4_INODE_RATIO: u32 = 16384;
const EXT4_INODE_SIZE: u16 = 256;
// Simplified because we don't use clusters
const EXT4_MAX_BLOCKS_PER_GROUP: u32 = (1 << 16) - 8;
pub const GROUP_0_PADDING: u64 = 1024;
const MIN_GROUP_BLOCK_COUNT: u64 = 50; // the lowest number of data blocks a block group can have
const MIN_BLOCK_SIZE: usize = 1024;


#[repr(C)]
#[derive(Copy, Clone)]
pub struct SuperBlock {
    pub s_inodes_count: u32,
    pub s_blocks_count_lo: u32,
    pub s_r_blocks_count_lo: u32,
    pub s_free_blocks_count_lo: u32,
    pub s_free_inodes_count: u32,
    pub s_first_data_block: u32,
    pub s_log_block_size: u32,
    pub s_log_cluster_size: u32,
    pub s_blocks_per_group: u32,
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
    pub s_inode_size: u16,
    pub s_block_group_nr: u16,
    pub s_feature_compat: u32,
    pub s_feature_incompat: u32,
    pub s_feature_ro_compat: u32,
    pub s_uuid: [u8; 16],
    pub s_volume_name: [u8; 16],
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
    pub fn from(boot_sector: &BootSector) -> io::Result<Self> {
        if boot_sector.get_data_range().start % boot_sector.cluster_size() != 0 {
            // We want to treat FAT clusters as ext4 blocks, but we can't if they're not aligned
            return Err(io::Error::new(io::ErrorKind::InvalidData, "The FAT partition's must be aligned to its cluster size (for more info, see the -a option in the mkfs.fat man page)."));
        }

        // SAFETY: This allows us to skip initializing a ton of fields to zero, but
        // CAUTION: some initialization steps rely on other fields already having been set,
        // so pay attention when refactoring/reoreding steps.
        let mut sb: SuperBlock = unsafe { std::mem::zeroed() };

        let block_size = boot_sector.cluster_size();
        if block_size < MIN_BLOCK_SIZE {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "The FAT partition's cluster size must be >= 1kB"));
        }

        let log_block_size = (block_size as f64).log2().round() as u32;
        if 2usize.pow(log_block_size) != block_size {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "FAT cluster size is not a power of 2"));
        }
        sb.s_log_block_size = log_block_size - EXT4_BLOCK_SIZE_MIN_LOG2;
        // check whether the entire first block is padding
        sb.s_first_data_block = if block_size as u64 <= GROUP_0_PADDING { 1 } else { 0 };
        sb.s_blocks_per_group = EXT4_MAX_BLOCKS_PER_GROUP.min(block_size as u32 * 8);

        sb.s_magic = EXT4_MAGIC;
        sb.s_state = EXT4_STATE_CLEANLY_UNMOUNTED;
        sb.s_feature_compat = EXT4_FEATURE_COMPAT_SPARSE_SUPER2;
        sb.s_feature_incompat = EXT4_FEATURE_INCOMPAT_64BIT | EXT4_FEATURE_INCOMPAT_EXTENTS;
        sb.s_desc_size = EXT4_64BIT_DESC_SIZE;
        sb.s_inode_size = EXT4_INODE_SIZE;
        sb.s_rev_level = EXT4_DYNAMIC_REV;
        sb.s_errors = EXT4_ERRORS_DEFAULT;
        sb.s_first_ino = EXT4_FIRST_NON_RSV_INODE;
        sb.s_max_mnt_count = u16::MAX;
        sb.s_mkfs_time = u32::try_from(chrono::Utc::now().timestamp()).unwrap();
        sb.s_uuid = *Uuid::new_v4().as_bytes();
        let volume_label = boot_sector.volume_label();
        sb.s_volume_name[0..volume_label.len()].clone_from_slice(volume_label);

        // These have to have these values even if bigalloc is disabled
        sb.s_log_cluster_size = sb.s_log_block_size;
        sb.s_clusters_per_group = sb.s_blocks_per_group;

        // This is the same logic as used by mke2fs to determine the inode count
        let min_inodes_per_group = block_size as u32 * 8; // Inodes per group need to fit into a one page bitmap
        sb.s_inodes_per_group = min_inodes_per_group.min(sb.s_blocks_per_group * (block_size as u32) / EXT4_INODE_RATIO);

        // Same logic as used in mke2fs: If the last block group would have
        // fewer than 50 data blocks, then reduce the block count and ignore the
        // remaining space
        // For some reason in tests we found that mkfs.ext4 didn't follow this logic
        // and instead set sb.blocks_per_group to a value lower than
        // `block_size` * 8, but this is easier to implement.
        // We use the sparse_super2 logic from mke2fs, meaning that the last block
        // group always has a super block copy.
        // TODO can it happen that block_count becomes zero?
        // TODO we would have to move data the falls outside of the bg into a bg, do we do that?
        let mut block_count = boot_sector.partition_size() / u64::try_from(block_size).unwrap();
        let mut data_block_count = block_count - sb.s_first_data_block as u64;
        // set the intermediate value in `sb` because it is needed by the call to `sb.block_group_overhead`.
        LoHiMut::new(&mut sb.s_blocks_count_lo, &mut sb.s_blocks_count_hi).set(block_count);
        let last_group_block_count = data_block_count % u64::from(sb.s_blocks_per_group);

        // method call requires `s_reserved_gdt_blocks`, `s_log_block_size`, `s_desc_size`, `s_inodes_per_group`, `s_inode_size`, `s_blocks_per_group`, `s_blocks_count_hi` and `s_blocks_count_lo` to be already set
        if last_group_block_count < sb.block_group_overhead(HasSuperBlock::YesBackup) + MIN_GROUP_BLOCK_COUNT {
            block_count -= last_group_block_count;
            data_block_count -= last_group_block_count;
            assert_ne!(data_block_count, 0);
            LoHiMut::new(&mut sb.s_blocks_count_lo, &mut sb.s_blocks_count_hi).set(block_count);
        }

        // Same logic as in mke2fs
        let block_group_count = data_block_count.div_ceil(&(u64::from(sb.s_blocks_per_group)));
        let block_group_count = u32::try_from(block_group_count).unwrap();
        sb.s_inodes_count = sb.s_inodes_per_group * block_group_count;

        if block_group_count > 1 {
            sb.s_backup_bgs[0] = 1;
            if block_group_count > 2 {
                sb.s_backup_bgs[1] = block_group_count - 1;
            }
        }
        Ok(sb)
    }

    // should I call bg has super from here instead?
    fn block_group_overhead(&self, has_superblock: HasSuperBlock) -> u64 {
        // block bitmap + inode bitmap + inode table
        let mut overhead = 2 + self.inode_table_block_count();
        match has_superblock {
            HasSuperBlock::YesOriginal | HasSuperBlock::YesBackup => {
                // superblock + group descriptor table
                overhead += 1 + self.gdt_block_count() + u64::from(self.s_reserved_gdt_blocks);
            },
            HasSuperBlock::No => ()
        }

        if has_superblock == HasSuperBlock::YesOriginal && self.block_size() <= GROUP_0_PADDING {
            // the entire first block is padding
            overhead += 1;
        }
        overhead
    }

    fn gdt_block_count(&self) -> u64 {
        // TODO
        // block size;
        // desc size;
        // bg count;
        // bg count * desc size = gdt size;
        // / block = gdt block count

        let descriptors_that_fit_into_a_block = self.block_size() / u64::from(self.s_desc_size);
        self.block_group_count().div_ceil(&descriptors_that_fit_into_a_block)
    }

    fn inode_table_block_count(&self) -> u64 {
        let inode_table_size = u64::from(self.s_inodes_per_group) * u64::from(self.s_inode_size);
        inode_table_size.div_ceil(&self.block_size())
    }

    pub fn block_size(&self) -> u64 {
        1 << (self.s_log_block_size + EXT4_BLOCK_SIZE_MIN_LOG2)
    }

    pub fn block_group_count(&self) -> u64 {
        let block_count: u64 = LoHi::new(&self.s_blocks_count_lo, &self.s_blocks_count_hi).get();
        block_count.div_ceil(&u64::from(self.s_blocks_per_group))
    }

    pub fn block_group_has_superblock(&self, block_group_idx: usize) -> HasSuperBlock {
        if block_group_idx == 0 {
            HasSuperBlock::YesOriginal
        } else if block_group_idx == self.s_backup_bgs[0] as usize || block_group_idx == self.s_backup_bgs[1] as usize {
            HasSuperBlock::YesBackup
        } else {
            HasSuperBlock::No
        }
    }

    // Caution: if the block size is 1024, every block group begins one block later than normal
    pub fn block_group_start_cluster(&self, block_group_idx: usize) -> ClusterIdx {
        self.s_blocks_per_group * block_group_idx as u32 + self.s_first_data_block
    }

    pub fn block_group_overhead_ranges(&self) -> Vec<Range<ClusterIdx>> {
        (0..self.block_group_count() as usize).map(|block_group_idx| {
            let has_sb_copy = self.block_group_has_superblock(block_group_idx);
            let overhead = self.block_group_overhead(has_sb_copy);

            let start_cluster_idx= self.block_group_start_cluster(block_group_idx);
            start_cluster_idx..start_cluster_idx + u32::try_from(overhead).unwrap()
        }).collect()
    }
}

#[derive(PartialEq)]
pub enum HasSuperBlock {
    YesOriginal,
    YesBackup,
    No
}