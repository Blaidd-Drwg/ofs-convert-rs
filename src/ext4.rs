use crate::fat::BootSector;
use std::convert::TryFrom;
use std::io;
use crate::lohi::LoHiMut;
use chrono;
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
    pub fn new(boot_sector: &BootSector) -> io::Result<Self> {
        // TODO bytes_per_* or *_size?
        let bytes_per_block = boot_sector.cluster_size();
        // TODO document why
        if bytes_per_block < 1024 {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "This tool only works for FAT partitions with cluster size >= 1kB"));
        }

        // SAFETY: safe as long as we set all fields needed for a consistent file system by hand
        let mut sb: SuperBlock = unsafe { std::mem::zeroed() };

        let log_block_size = f64::from(bytes_per_block).log2().round() as u32;
        if 2u32.pow(log_block_size) != bytes_per_block {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "FAT cluster size is not a power of 2"));
        }
        sb.s_log_block_size = log_block_size - EXT4_BLOCK_SIZE_MIN_LOG2;
        sb.s_first_data_block = if bytes_per_block == 1024 { 1 } else { 0 };
        sb.s_blocks_per_group = EXT4_MAX_BLOCKS_PER_GROUP.min(bytes_per_block * 8);
        let block_count = boot_sector.partition_size() / u64::from(bytes_per_block);
        LoHiMut::new(&mut sb.s_blocks_count_lo, &mut sb.s_blocks_count_hi).set(block_count);

        // TODO what
        // Same logic as used in mke2fs: If the last block group would have
        // fewer than 50 data blocks, then reduce the block count and ignore the
        // remaining space
        // For some reason in tests we found that mkfs.ext4 didn't follow this logic
        // and instead set sb.blocks_per_group to a value lower than
        // bytes_per_block * 8, but this is easier to implement.
        // We use the sparse_super2 logic from mke2fs, meaning that the last block
        // group always has a super block copy.
        // if block_count % sb.s_blocks_per_group < block_group_overhead(true) + 50 {
            // LoHi64 { lo: &mut sb.s_blocks_count_lo, hi: &mut sb.s_blocks_count_hi }.set(block_count);
        // }

        // Same logic as in mke2fs
        let block_group_count = block_count.div_ceil(&(u64::from(sb.s_blocks_per_group)));
        let block_group_count = u32::try_from(block_group_count).unwrap(); // TODO

        if block_group_count > 1 {
            sb.s_backup_bgs[0] = 1;
            if block_group_count > 2 {
                sb.s_backup_bgs[1] = block_group_count - 1;
            }
        }

        // This is the same logic as used by mke2fs to determine the inode count
        let min_inodes_per_group = bytes_per_block * 8; // Inodes per group need to fit into a one page bitmap
        sb.s_inodes_per_group = min_inodes_per_group.min(sb.s_blocks_per_group * bytes_per_block / EXT4_INODE_RATIO);
        sb.s_inodes_count = sb.s_inodes_per_group * block_group_count;

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

        Ok(sb)
    }
}
