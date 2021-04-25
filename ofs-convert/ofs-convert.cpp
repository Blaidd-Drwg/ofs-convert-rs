#include "ext4.h"
#include "ext4_bg.h"
#include "extent-allocator.h"
#include "metadata_reader.h"
#include "partition.h"
#include "visualizer.h"
#include "stream-archiver.h"
#include "tree_builder.h"

#include <stdlib.h>
#include <stdio.h>

int c_main(Partition partition, ext4_super_block _sb, struct boot_sector _boot_sector) {
    sb = _sb;
    boot_sector = _boot_sector;

    set_meta_info(partition.ptr);

    int bg_count = block_group_count();
    init_extent_allocator(create_block_group_meta_extents(bg_count), bg_count);

    StreamArchiver write_stream;
    init_stream_archiver(&write_stream, meta_info.cluster_size);
    StreamArchiver extent_stream = write_stream;
    StreamArchiver read_stream = write_stream;

    aggregate_extents(boot_sector.root_cluster_no, true, &write_stream);
    traverse(&extent_stream, &write_stream);


    init_ext4_group_descs();
    build_ext4_root();
    build_ext4_metadata_tree(EXT4_ROOT_INODE, EXT4_ROOT_INODE, &read_stream);
    build_lost_found();
    finalize_block_groups_on_disk();

    visualizer_render_to_file("partition.svg", partition.size / meta_info.cluster_size);
    return 0;
}
