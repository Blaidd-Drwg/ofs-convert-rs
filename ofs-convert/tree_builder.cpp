#include "fat.h"
#include "ext4.h"
#include "ext4_dentry.h"
#include "ext4_extent.h"
#include "ext4_inode.h"
#include "extent-allocator.h"
#include "stream-archiver.h"
#include "tree_builder.h"
#include "visualizer.h"
#include "extent_iterator.h"

#include <string.h>
#include <stdint.h>
#include <stdlib.h>
#include <unistd.h>
#include <sys/types.h>

void build_ext4_root() {
    build_root_inode();
}

void skip_child_count(StreamArchiver *read_stream) {
    while (getNext<uint32_t>(read_stream)) ;
}

void skip_dir_extents(StreamArchiver *read_stream) {
    while (getNext<fat_extent>(read_stream)) ;
}

ext4_dentry *build_dot_dirs(uint32_t dir_inode_no, uint32_t parent_inode_no, uint8_t *dot_dentry_p) {
    ext4_dentry dot_dentry = build_dot_dir_dentry(dir_inode_no);
    memcpy(dot_dentry_p, &dot_dentry, dot_dentry.rec_len);
    uint8_t *dot_dot_dentry_p = dot_dentry_p + dot_dentry.rec_len;

    ext4_dentry dot_dot_dentry = build_dot_dot_dir_dentry(parent_inode_no);
    memcpy(dot_dot_dentry_p, &dot_dot_dentry, dot_dot_dentry.rec_len);
    return (ext4_dentry *) dot_dot_dentry_p;
}

void build_lost_found() {
    fat_extent root_dentry_extent = allocate_extent(1);
    ext4_extent last_root_extent = last_extent(EXT4_ROOT_INODE);
    root_dentry_extent.logical_start = last_root_extent.ee_block + last_root_extent.ee_len;
    register_extent(&root_dentry_extent, EXT4_ROOT_INODE);

    build_lost_found_inode();
    ext4_dentry *dentry_address = (ext4_dentry *) cluster_start(root_dentry_extent.physical_start);
    ext4_dentry lost_found_dentry = build_lost_found_dentry();
    lost_found_dentry.rec_len = block_size();
    *dentry_address = lost_found_dentry;
    set_size(EXT4_ROOT_INODE, get_size(EXT4_ROOT_INODE) + block_size());

    // Build . and .. dirs in lost+found
    fat_extent lost_found_dentry_extent = allocate_extent(1);
    lost_found_dentry_extent.logical_start = 0;
    uint8_t *lost_found_dentry_p = cluster_start(lost_found_dentry_extent.physical_start);
    ext4_dentry *dot_dot_dentry = build_dot_dirs(EXT4_LOST_FOUND_INODE, EXT4_ROOT_INODE, lost_found_dentry_p);
    dot_dot_dentry->rec_len = block_size() - EXT4_DOT_DENTRY_SIZE;
    register_extent(&lost_found_dentry_extent, EXT4_LOST_FOUND_INODE);
    set_size(EXT4_LOST_FOUND_INODE, block_size());

    visualizer_add_block_range({BlockRange::Ext4Dir, fat_cl_to_e4blk(root_dentry_extent.physical_start), 1});
    visualizer_add_block_range({BlockRange::Ext4Dir, fat_cl_to_e4blk(lost_found_dentry_extent.physical_start), 1});
}

uint64_t next_dir_block(extent_iterator *iterator) {
    uint32_t cluster_no = next_cluster_no(iterator);
    if (!cluster_no)
        cluster_no = allocate_extent(1).physical_start;

    return fat_cl_to_e4blk(cluster_no);
}

void register_dir_extent(uint64_t block_no, uint32_t logical_no, uint32_t inode_no) {
    fat_extent extent = {logical_no, 1, e4blk_to_fat_cl(block_no)};
    register_extent(&extent, inode_no);
}

void build_ext4_metadata_tree(uint32_t dir_inode_no, uint32_t parent_inode_no, StreamArchiver *read_stream) {
    StreamArchiver extent_stream = *read_stream;
    extent_iterator iterator = init(&extent_stream);
    uint64_t dentry_block_no = next_dir_block(&iterator);
    uint8_t *dentry_block_start = block_start(dentry_block_no);

    skip_dir_extents(read_stream);
    uint32_t child_count = *getNext<uint32_t>(read_stream);
    getNext<uint32_t>(read_stream);  // consume cut

    uint32_t block_count = 1;

    ext4_dentry *previous_dentry = build_dot_dirs(dir_inode_no, parent_inode_no, dentry_block_start);
    int position_in_block = 2 * EXT4_DOT_DENTRY_SIZE;

    for (uint32_t i = 0; i < child_count; i++) {
        fat_dentry *f_dentry = getNext<fat_dentry>(read_stream);
        getNext<fat_dentry>(read_stream);  // consume cut

        uint32_t inode_number = build_inode(f_dentry);
        ext4_dentry *e_dentry = build_dentry(inode_number, read_stream);
        if (e_dentry->rec_len > block_size() - position_in_block) {
            previous_dentry->rec_len += block_size() - position_in_block;

            register_dir_extent(dentry_block_no, block_count - 1, dir_inode_no);
            block_count++;
            visualizer_add_block_range({BlockRange::Ext4Dir, dentry_block_no, 1});

            dentry_block_no = next_dir_block(&iterator);
            dentry_block_start = block_start(dentry_block_no);
            position_in_block = 0;
        }
        previous_dentry = (ext4_dentry *) (dentry_block_start + position_in_block);
        position_in_block += e_dentry->rec_len;

        memcpy(previous_dentry, e_dentry, e_dentry->rec_len);
        free(e_dentry);

        if (!is_dir(f_dentry)) {
            set_extents(inode_number, f_dentry, read_stream);
            skip_child_count(read_stream);
        } else {
            incr_links_count(dir_inode_no);
            build_ext4_metadata_tree(inode_number, dir_inode_no, read_stream);
        }
    }

    if (previous_dentry) {
        previous_dentry->rec_len += block_size() - position_in_block;
    }

    register_dir_extent(dentry_block_no, block_count - 1, dir_inode_no);
    visualizer_add_block_range({BlockRange::Ext4Dir, dentry_block_no, 1});
    set_size(dir_inode_no, block_count * block_size());
}
