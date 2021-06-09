#include "ext4.h"
#include "ext4_bg.h"
#include "partition.h"
#include "visualizer.h"
#include "tree_builder.h"

#include <stdlib.h>
#include <stdio.h>

void initialize(uint8_t* fs_start, ext4_super_block _sb, struct boot_sector _boot_sector) {
    sb = _sb;
    boot_sector = _boot_sector;
    set_meta_info(fs_start);
}

DentryWritePosition start_writing(AllocatorFunc allocate_block_callback, AllocatorData allocator_data) {
    init_ext4_group_descs();
    return build_ext4_root(allocate_block_callback, allocator_data);
}

void end_writing(DentryWritePosition dentry_write_position, AllocatorFunc allocate_block_callback, AllocatorData allocator_data) {
    build_lost_found(dentry_write_position, allocate_block_callback, allocator_data);
    finalize_dir(dentry_write_position);
    finalize_block_groups_on_disk();
    // visualizer_render_to_file("partition.svg", partition.size / meta_info.cluster_size);
}
