#include "ext4_dentry.h"
#include "fat.h"
#include <stdint.h>

struct StreamArchiver;

typedef void* AllocatorData;
typedef uint32_t (*AllocatorFunc)(AllocatorData);

struct DentryWritePosition {
    uint32_t inode_no;
    uint32_t block_no;
    uint32_t position_in_block;
    uint32_t block_count;
    ext4_dentry *previous_dentry;
};

DentryWritePosition build_ext4_root(AllocatorFunc allocate_block_callback, AllocatorData allocator_data);
void build_lost_found(DentryWritePosition& dentry_write_position, AllocatorFunc allocate_block_callback, AllocatorData allocator_data);
void build_ext4_metadata_tree(uint32_t dir_inode_no, uint32_t parent_inode_no, StreamArchiver *read_stream);

void build_regular_file(
        const fat_dentry* f_dentry,
        const uint8_t name[],
        size_t name_len,
        DentryWritePosition& dentry_write_position,
        AllocatorFunc allocate_block_callback,
        AllocatorData allocator_data,
        const fat_extent extents[],
        size_t extent_count
    );

DentryWritePosition build_directory(
        const fat_dentry* f_dentry,
        const uint8_t name[],
        size_t name_len,
        DentryWritePosition& parent_dentry_write_position,
        AllocatorFunc allocate_block_callback,
        AllocatorData allocator_data
        );

void finalize_dir(DentryWritePosition& dentry_write_position);
