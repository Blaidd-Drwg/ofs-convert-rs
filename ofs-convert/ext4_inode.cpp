#include "fat.h"
#include "ext4.h"
#include "ext4_bg.h"
#include "ext4_extent.h"
#include "ext4_inode.h"
#include "util.h"

#include <string.h>
#include <stdint.h>
#include <stdlib.h>
#include <unistd.h>
#include <sys/types.h>
#include <time.h>

uint32_t first_free_inode_no = EXT4_FIRST_NON_RSV_INODE + 1;  // account for lost+found

void build_root_inode() {
    ext4_inode inode;
    memset(&inode, 0, sizeof inode);
    inode.i_mode = static_cast<uint16_t>(0755) | S_IFDIR;
    inode.i_uid = geteuid() & 0xFFFF;
    inode.l_i_uid_high = geteuid() >> 16;
    inode.i_gid = getegid() & 0xFFFF;
    inode.l_i_gid_high = getegid() >> 16;
    inode.i_atime = (uint32_t) time(NULL);
    inode.i_ctime = (uint32_t) time(NULL);
    inode.i_mtime = (uint32_t) time(NULL);
    inode.i_links_count = 0;
    inode.i_flags = 0x80000;  // uses extents
    inode.ext_header = init_extent_header();

    add_reserved_inode(inode, EXT4_ROOT_INODE);
}

void build_lost_found_inode() {
    ext4_inode inode;
    memset(&inode, 0, sizeof inode);
    inode.i_mode = static_cast<uint16_t>(0755) | S_IFDIR;
    inode.i_uid = ROOT_UID;
    inode.i_gid = ROOT_GID;
    inode.i_atime = (uint32_t) time(NULL);
    inode.i_ctime = (uint32_t) time(NULL);
    inode.i_mtime = (uint32_t) time(NULL);
    inode.i_links_count = 1;
    inode.i_flags = 0x80000;  // uses extents
    inode.ext_header = init_extent_header();

    add_reserved_inode(inode, EXT4_LOST_FOUND_INODE);
}

void set_size(uint32_t inode_no, uint64_t size) {
    ext4_inode& inode = get_existing_inode(inode_no);
    set_lo_hi(inode.i_size_lo, inode.i_size_high, size);
}

uint64_t get_size(uint32_t inode_no) {
    ext4_inode& inode = get_existing_inode(inode_no);
    return from_lo_hi(inode.i_size_lo, inode.i_size_high);
}

void incr_links_count(uint32_t inode_no) {
    ext4_inode& inode = get_existing_inode(inode_no);
    inode.i_links_count++;
}
