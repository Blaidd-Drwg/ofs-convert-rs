#include "fat.h"
#include "ext4.h"
#include "ext4_dentry.h"
#include "ext4_extent.h"
#include "ext4_inode.h"
#include "extent-allocator.h"
#include "tree_builder.h"
#include "visualizer.h"

#include <bits/stdint-uintn.h>
#include <string.h>
#include <stdint.h>
#include <stdlib.h>
#include <unistd.h>
#include <sys/types.h>

void allocate_block(DentryWritePosition& dentry_write_position, AllocatorFunc allocate_block_callback, AllocatorData allocator_data);
void build_dot_dirs(DentryWritePosition& dentry_write_position, uint32_t parent_inode_no);

DentryWritePosition build_ext4_root(AllocatorFunc allocate_block_callback, AllocatorData allocator_data) {
    build_root_inode();
    DentryWritePosition dentry_write_position {
            .inode_no = EXT4_ROOT_INODE,
            .previous_dentry = NULL
    };
    allocate_block(dentry_write_position, allocate_block_callback, allocator_data);
    build_dot_dirs(dentry_write_position, EXT4_ROOT_INODE);
    return dentry_write_position;
}

void build_dot_dirs(DentryWritePosition& dentry_write_position, uint32_t parent_inode_no) {
    ext4_dentry dot_dentry = build_dot_dir_dentry(dentry_write_position.inode_no);
    auto dentry_block_start = block_start(dentry_write_position.block_no);
    auto dentry_space = (ext4_dentry *) (dentry_block_start + dentry_write_position.position_in_block);
    // the dot dirs are the first dentries, so we can assume there is enough space in the block
    memcpy(dentry_space, &dot_dentry, dot_dentry.rec_len);
    dentry_write_position.position_in_block += dot_dentry.rec_len;
    incr_links_count(dentry_write_position.inode_no);

    ext4_dentry dot_dot_dentry = build_dot_dot_dir_dentry(parent_inode_no);
    dentry_space = (ext4_dentry *) (dentry_block_start + dentry_write_position.position_in_block);
    memcpy(dentry_space, &dot_dot_dentry, dot_dot_dentry.rec_len);
    dentry_write_position.position_in_block += dot_dot_dentry.rec_len;
    dentry_write_position.previous_dentry = dentry_space;
    incr_links_count(parent_inode_no);
}

void build_lost_found(DentryWritePosition& dentry_write_position, AllocatorFunc allocate_block_callback, AllocatorData allocator_data) {
    build_lost_found_inode();
    ext4_dentry lost_found_dentry = build_lost_found_dentry();

    if (lost_found_dentry.rec_len > block_size() - dentry_write_position.position_in_block) {
        allocate_block(dentry_write_position, allocate_block_callback, allocator_data);
    }

    auto dentry_block_start = block_start(dentry_write_position.block_no);
    auto dentry_space = (ext4_dentry *) (dentry_block_start + dentry_write_position.position_in_block);
    *dentry_space = lost_found_dentry;
    dentry_write_position.position_in_block += lost_found_dentry.rec_len;
    dentry_write_position.previous_dentry = dentry_space;

    // Build . and .. dirs in lost+found
    DentryWritePosition lost_found_dentry_write_position {
            .inode_no = EXT4_LOST_FOUND_INODE,
            .position_in_block = 0,
            .block_count = 0,
            .previous_dentry = NULL,
    };
    allocate_block(lost_found_dentry_write_position, allocate_block_callback, allocator_data);
    build_dot_dirs(lost_found_dentry_write_position, EXT4_ROOT_INODE);
    finalize_dir(lost_found_dentry_write_position);

    // visualizer_add_block_range({BlockRange::Ext4Dir, fat_cl_to_e4blk(new_root_extent.physical_start), 1});
    // visualizer_add_block_range({BlockRange::Ext4Dir, fat_cl_to_e4blk(lost_found_dentry_extent.physical_start), 1});
}

void register_dir_extent(uint64_t block_no, uint32_t logical_no, uint32_t inode_no) {
    fat_extent extent = {logical_no, 1, static_cast<uint32_t>(block_no)};
    register_extent(&extent, inode_no);
}


void allocate_block(DentryWritePosition& dentry_write_position, AllocatorFunc allocate_block_callback, AllocatorData allocator_data) {
    if (dentry_write_position.previous_dentry) {
        dentry_write_position.previous_dentry->rec_len += block_size() - dentry_write_position.position_in_block;
    }

    dentry_write_position.block_no = allocate_block_callback(allocator_data);
    dentry_write_position.position_in_block = 0;
    dentry_write_position.block_count++;
    dentry_write_position.previous_dentry = NULL;

    register_dir_extent(dentry_write_position.block_no, dentry_write_position.block_count - 1, dentry_write_position.inode_no);
//    visualizer_add_block_range({BlockRange::Ext4Dir, dentry_write_position.block_no, 1});
}

uint32_t build_file(
        const fat_dentry* f_dentry,
        const uint8_t name[],
        size_t name_len,
        DentryWritePosition& dentry_write_position,
        AllocatorFunc allocate_block_callback,
        AllocatorData allocator_data
        ) {
    uint32_t inode_no = build_inode(f_dentry);
    ext4_dentry *e_dentry = build_dentry(inode_no, name, name_len);
    if (e_dentry->rec_len > block_size() - dentry_write_position.position_in_block) {
        allocate_block(dentry_write_position, allocate_block_callback, allocator_data);
    }

    auto dentry_block_start = block_start(dentry_write_position.block_no);
    auto dentry_space = (ext4_dentry *) (dentry_block_start + dentry_write_position.position_in_block);

    memcpy(dentry_space, e_dentry, e_dentry->rec_len);
    dentry_write_position.position_in_block += e_dentry->rec_len;
    dentry_write_position.previous_dentry = dentry_space;
    free(e_dentry);
    return inode_no;
}

void build_regular_file(
        const fat_dentry* f_dentry,
        const uint8_t name[],
        size_t name_len,
        DentryWritePosition& dentry_write_position,
        AllocatorFunc allocate_block_callback,
        AllocatorData allocator_data,
        const fat_extent extents[],
        size_t extent_count
        ) {
    uint32_t inode_no = build_file(
            f_dentry,
            name,
            name_len,
            dentry_write_position,
            allocate_block_callback,
            allocator_data
    );
    set_extents(inode_no, f_dentry, extents, extent_count);
}

// parent_dir_inode_no
DentryWritePosition build_directory(
        const fat_dentry* f_dentry,
        const uint8_t name[],
        size_t name_len,
        DentryWritePosition& parent_dentry_write_position,
        AllocatorFunc allocate_block_callback,
        AllocatorData allocator_data
        ) {
    uint32_t inode_no = build_file(
            f_dentry,
            name,
            name_len,
            parent_dentry_write_position,
            allocate_block_callback,
            allocator_data
    );

    DentryWritePosition dentry_write_position {
        .inode_no = inode_no,
        .previous_dentry = NULL,
    };
    allocate_block(dentry_write_position, allocate_block_callback, allocator_data);

    build_dot_dirs(dentry_write_position, parent_dentry_write_position.inode_no);
    return dentry_write_position;
}

void finalize_dir(DentryWritePosition& dentry_write_position) {
    // make last dentry take up entire block
    if (dentry_write_position.previous_dentry) {
        dentry_write_position.previous_dentry->rec_len += block_size() - dentry_write_position.position_in_block;
        dentry_write_position.position_in_block = block_size();
    }
    // visualizer_add_block_range({BlockRange::Ext4Dir, dentry_block_no, 1});
    set_size(dentry_write_position.inode_no, dentry_write_position.block_count * block_size());
}
