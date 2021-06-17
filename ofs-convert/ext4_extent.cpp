#include "ext4.h"
#include "ext4_bg.h"
#include "ext4_extent.h"
#include "ext4_inode.h"
#include "fat.h"
#include "util.h"
#include "visualizer.h"

#include <string.h>

ext4_extent_header init_extent_header() {
    ext4_extent_header header;
    header.eh_entries = 0;
    header.eh_max = 4;
    header.eh_depth = 0;
    header.eh_generation = 0;
    return header;
}

ext4_extent to_ext4_extent(const fat_extent *fext) {
    ext4_extent eext;
    eext.ee_block = fext->logical_start;
    eext.ee_len = fext->length;
    set_lo_hi(eext.ee_start_lo, eext.ee_start_hi, fext->physical_start);
    return eext;
}

void register_extent(uint64_t extent_start_block, uint64_t extent_len, uint32_t inode_no) {
    ext4_inode *inode = &get_existing_inode(inode_no);

    uint32_t block_count = static_cast<uint32_t>(extent_len) * block_size() / 512;  // number of 512-byte blocks allocated
    incr_lo_hi(inode->i_blocks_lo, inode->l_i_blocks_high, block_count);

    add_extent_to_block_bitmap(extent_start_block, extent_start_block + extent_len);
}
