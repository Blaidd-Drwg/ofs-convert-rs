#include "extent-allocator.h"
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

uint16_t max_entries() {
    return (block_size() - sizeof(ext4_extent_header)) / sizeof(ext4_extent);
}

bool append_in_block(ext4_extent_header *header, ext4_extent *ext) {
    if (header->eh_entries >= header->eh_max) return false;

    ext4_extent *new_entry = (ext4_extent *) (header + header->eh_entries + 1);
    memcpy(new_entry, ext, sizeof *ext);
    header->eh_entries++;
    return true;
}

void register_extent(uint64_t extent_start_block, uint64_t extent_len, uint32_t inode_no) {
    ext4_inode *inode = &get_existing_inode(inode_no);

    uint32_t block_count = static_cast<uint32_t>(extent_len) * block_size() / 512;  // number of 512-byte blocks allocated
    incr_lo_hi(inode->i_blocks_lo, inode->l_i_blocks_high, block_count);

    add_extent_to_block_bitmap(extent_start_block, extent_start_block + extent_len);
}

ext4_extent last_extent(uint32_t inode_number) {
    ext4_inode *inode = &get_existing_inode(inode_number);
    ext4_extent_header *header = &(inode->ext_header);

    while(header->eh_depth) {
        ext4_extent_idx *last_idx = (ext4_extent_idx *) (header + header->eh_entries);
        header = (ext4_extent_header *) block_start(from_lo_hi(last_idx->ei_leaf_lo, last_idx->ei_leaf_hi));
    }

    return *(ext4_extent *) (header + header->eh_entries);
}
