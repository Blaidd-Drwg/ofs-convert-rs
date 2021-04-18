#include "extent-allocator.h"
#include "ext4.h"
#include "ext4_bg.h"
#include "ext4_extent.h"
#include "ext4_inode.h"
#include "fat.h"
#include "stream-archiver.h"
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

ext4_extent to_ext4_extent(fat_extent *fext) {
    ext4_extent eext;
    eext.ee_block = fext->logical_start;
    eext.ee_len = fext->length;
    set_lo_hi(eext.ee_start_lo, eext.ee_start_hi, fat_cl_to_e4blk(fext->physical_start));
    return eext;
}

uint16_t max_entries() {
    return (block_size() - sizeof(ext4_extent_header)) / sizeof(ext4_extent);
}

void append_to_new_idx_path(uint16_t depth, ext4_extent *ext_to_append, ext4_extent_idx *idx, uint32_t inode_no) {
    for (int i = depth; i > depth; i--) {
        fat_extent idx_ext = allocate_extent(1);
        register_extent(&idx_ext, inode_no, false);

        uint32_t block_no = fat_cl_to_e4blk(idx_ext.physical_start);
        *idx = {
            .ei_block = ext_to_append->ee_block,
            .ei_leaf_lo = block_no,
            .ei_leaf_hi = 0
        };

        ext4_extent_header *header = (ext4_extent_header *) block_start(idx->ei_leaf_lo);
        header->eh_entries = 1;
        header->eh_max = max_entries();
        header->eh_depth = i;

        idx = (ext4_extent_idx *) (header + 1);
    }
    ext4_extent *actual_extent = (ext4_extent *) idx;
    memcpy(actual_extent, ext_to_append, sizeof *ext_to_append);
}

bool append_in_block(ext4_extent_header *header, ext4_extent *ext) {
    if (header->eh_entries >= header->eh_max) return false;

    ext4_extent *new_entry = (ext4_extent *) (header + header->eh_entries + 1);
    memcpy(new_entry, ext, sizeof *ext);
    header->eh_entries++;
    return true;
}

bool append_to_extent_tree(ext4_extent *ext, ext4_extent_header *root_header, uint32_t inode_no) {
    if (root_header->eh_depth == 0) {
        bool success = append_in_block(root_header, ext);
        return success;
    }

    // attempt appending to an existing level 0 block
    uint16_t entry_count = root_header->eh_entries;
    ext4_extent_idx *last_child_entry = (ext4_extent_idx *) (root_header + entry_count);
    uint32_t child_block = from_lo_hi(last_child_entry->ei_leaf_lo, last_child_entry->ei_leaf_hi);
    ext4_extent_header *child_header = (ext4_extent_header *) block_start(child_block);
    if (append_to_extent_tree(ext, child_header, inode_no)) {
        return true;
    }

    // all existing level 0 blocks are full, create a new one
    if (entry_count < root_header->eh_max) {
        ext4_extent_idx *new_idx = (ext4_extent_idx *) (root_header + entry_count);
        append_to_new_idx_path(root_header->eh_depth - 1, ext, new_idx, inode_no);
        return true;
    } else {
        // the tree is already full
        return false;
    }
}

void make_tree_deeper(ext4_extent_header *root_header, uint32_t inode_no) {
    fat_extent idx_ext = allocate_extent(1);
    register_extent(&idx_ext, inode_no, false);

    uint64_t block_no = fat_cl_to_e4blk(idx_ext.physical_start);
    uint8_t *child_block = block_start(block_no);
    memcpy(child_block, root_header, 5 * sizeof *root_header);  // copy header and all nodes from the inode

    ext4_extent_header *child_header = (ext4_extent_header *) child_block;
    child_header->eh_max = max_entries();

    root_header->eh_depth++;
    root_header->eh_entries = 1;
    ext4_extent_idx *idx = (ext4_extent_idx *) (root_header + 1);
    idx->ei_block = 0;
    set_lo_hi(idx->ei_leaf_lo, idx->ei_leaf_hi, block_no);
}

void add_extent(ext4_extent *eext, uint32_t inode_no, ext4_inode *inode) {
    ext4_extent_header *header = &(inode->ext_header);
    bool success = append_to_extent_tree(eext, header, inode_no);

    // tree is full, add another level
    if (!success) {
        make_tree_deeper(header, inode_no);
        // attempt adding extent again, should succeed this time
        ext4_extent_header *new_root_header = &(inode->ext_header);
        append_to_extent_tree(eext, new_root_header, inode_no);
    }
}

void register_extent(fat_extent *fext, uint32_t inode_no, bool add_to_extent_tree) {
    ext4_inode *inode = &get_existing_inode(inode_no);
    ext4_extent eext = to_ext4_extent(fext);

    if (add_to_extent_tree) {
        add_extent(&eext, inode_no, inode);
    } else {
        visualizer_add_block_range({BlockRange::IdxNode, from_lo_hi(eext.ee_start_lo, eext.ee_start_hi), eext.ee_len});
    }

    uint32_t block_count = static_cast<uint32_t>(eext.ee_len) * block_size() / 512;  // number of 512-byte blocks allocated
    incr_lo_hi(inode->i_blocks_lo, inode->l_i_blocks_high, block_count);

    uint64_t extent_start_block = from_lo_hi(eext.ee_start_lo, eext.ee_start_hi);
    add_extent_to_block_bitmap(extent_start_block, extent_start_block + eext.ee_len);
}

void set_extents(uint32_t inode_number, fat_dentry *dentry, StreamArchiver *read_stream) {
    set_size(inode_number, dentry->file_size);
    fat_extent *current_extent = getNext<fat_extent>(read_stream);
    while (current_extent != NULL) {
        register_extent(current_extent, inode_number);
        current_extent = getNext<fat_extent>(read_stream);
    }
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
