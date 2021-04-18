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

int main(int argc, const char** argv) {
    if (argc < 2) {
        printf("Wrong usage\n");
        exit(1);
    }
    Partition partition = {.path = argv[1]};
    if (!openPartition(&partition)) {
        fprintf(stderr, "Failed to open partition");
        return 1;
    }

    read_boot_sector(partition.ptr);
    set_meta_info(partition.ptr);

    init_ext4_sb();
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

    closePartition(&partition);
    visualizer_render_to_file("partition.svg", partition.fileStat.st_size / meta_info.cluster_size);
}
