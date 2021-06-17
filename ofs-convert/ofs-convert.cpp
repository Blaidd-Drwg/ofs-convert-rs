#include "ext4.h"
#include "ext4_bg.h"
#include "partition.h"
#include "visualizer.h"

#include <stdlib.h>
#include <stdio.h>

void initialize(uint8_t* fs_start, ext4_super_block _sb, struct boot_sector _boot_sector) {
    sb = _sb;
    boot_sector = _boot_sector;
    set_meta_info(fs_start);
}

void start_writing() {
    init_ext4_group_descs();
}

void end_writing() {
    finalize_block_groups_on_disk();
}
