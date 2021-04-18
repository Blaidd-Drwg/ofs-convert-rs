#ifndef OFS_EXT4_INODE_H
#define OFS_EXT4_INODE_H
#include "ext4_extent.h"
#include <stdint.h>


constexpr uint16_t S_IFDIR = 0x4000;
constexpr uint16_t S_IFREG = 0x8000;
constexpr uint16_t ROOT_UID = 0;
constexpr uint16_t ROOT_GID = 0;

struct ext4_inode {
    uint16_t    i_mode;        /* File mode */
    uint16_t    i_uid;        /* Low 16 bits of Owner Uid */
    uint32_t    i_size_lo;    /* Size in bytes */
    uint32_t    i_atime;    /* Access time */
    uint32_t    i_ctime;    /* Inode Change time */
    uint32_t    i_mtime;    /* Modification time */
    uint32_t    i_dtime;    /* Deletion Time */
    uint16_t    i_gid;        /* Low 16 bits of Group Id */
    uint16_t    i_links_count;    /* Links count */
    uint32_t    i_blocks_lo;    /* Blocks count */
    uint32_t    i_flags;    /* File flags */
    uint32_t    l_i_version;
    ext4_extent_header ext_header;
    ext4_extent extents[4];
    uint32_t    i_generation;    /* File version (for NFS) */
    uint32_t    i_file_acl_lo;    /* File ACL */
    uint32_t    i_size_high;
    uint32_t    i_obso_faddr;    /* Obsoleted fragment address */
    uint16_t    l_i_blocks_high; /* were l_i_reserved1 */
    uint16_t    l_i_file_acl_high;
    uint16_t    l_i_uid_high;    /* these 2 fields */
    uint16_t    l_i_gid_high;    /* were reserved2[0] */
    uint16_t    l_i_checksum_lo;/* crc32c(uuid+inum+inode) LE */
    uint16_t    l_i_reserved;
    uint16_t    i_extra_isize;
    uint16_t    i_checksum_hi;    /* crc32c(uuid+inum+inode) BE */
    uint32_t    i_ctime_extra;  /* extra Change time      (nsec << 2 | epoch) */
    uint32_t    i_mtime_extra;  /* extra Modification time(nsec << 2 | epoch) */
    uint32_t    i_atime_extra;  /* extra Access time      (nsec << 2 | epoch) */
    uint32_t    i_crtime;       /* File Creation time */
    uint32_t    i_crtime_extra; /* extra FileCreationtime (nsec << 2 | epoch) */
    uint32_t    i_version_hi;    /* high 32 bits for 64-bit version */
    uint32_t    i_projid;    /* Project ID */
};

uint32_t build_inode(fat_dentry *dentry);
void build_root_inode();
void build_lost_found_inode();
void set_size(uint32_t inode_number, uint64_t size);
uint64_t get_size(uint32_t inode_number);
void incr_links_count(uint32_t inode_no);

#endif //OFS_EXT4_INODE_H
