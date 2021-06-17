#include <ctype.h>
#include <stdio.h>
#include <string.h>
#include <stdlib.h>
#include <sys/queue.h>
#include <stdint.h>
#include <stdbool.h>
#include <time.h>

#include "fat.h"
#include "partition.h"
#include "visualizer.h"

struct boot_sector boot_sector;
struct meta_info meta_info;

uint64_t fat_cl_to_e4blk(uint32_t cluster_no) {
    return (cluster_no - FAT_START_INDEX) + meta_info.sectors_before_data / boot_sector.sectors_per_cluster;
}

// returns 0 if block is before the first data cluster
uint32_t e4blk_to_fat_cl(uint64_t block_no) {
    int64_t cluster_no = block_no + FAT_START_INDEX - meta_info.sectors_before_data / boot_sector.sectors_per_cluster;
    return (cluster_no < FAT_START_INDEX) ? 0 : static_cast<uint32_t >(cluster_no);
}

bool is_dir(const struct fat_dentry *dentry) {
    return dentry->attrs & 0x10;
}

uint32_t fat_time_to_unix(uint16_t date, uint16_t time) {
    tm datetm;
    memset(&datetm, 0, sizeof datetm);

    datetm.tm_year = ((date & 0xFE00) >> 9) + 80;
    datetm.tm_mon= ((date & 0x1E0) >> 5) - 1;
    datetm.tm_mday = date & 0x1F;
    datetm.tm_hour = (time & 0xF800) >> 11;
    datetm.tm_min = (time & 0x7E0) >> 5;
    datetm.tm_sec = (time & 0x1F) * 2;
    return static_cast<uint32_t>(timegm(&datetm));
}

void set_meta_info(uint8_t *fs_start) {
    meta_info.fs_start = fs_start;
    meta_info.fat_start = (uint32_t *) (fs_start + boot_sector.sectors_before_fat * boot_sector.bytes_per_sector);
    meta_info.fat_entries = boot_sector.sectors_per_fat / boot_sector.sectors_per_cluster;
    meta_info.cluster_size = boot_sector.sectors_per_cluster * boot_sector.bytes_per_sector;
    meta_info.dentries_per_cluster = meta_info.cluster_size / sizeof(struct fat_dentry);
    meta_info.sectors_before_data = boot_sector.sectors_before_fat + boot_sector.sectors_per_fat * boot_sector.fat_count;
    meta_info.data_start = fs_start + meta_info.sectors_before_data * boot_sector.bytes_per_sector;

    visualizer_add_block_range({
        BlockRange::FAT,
        boot_sector.sectors_before_fat / static_cast<uint64_t>(boot_sector.sectors_per_cluster),
        boot_sector.sectors_per_fat * boot_sector.fat_count / static_cast<uint64_t>(boot_sector.sectors_per_cluster)
    });

    if (meta_info.sectors_before_data % boot_sector.sectors_per_cluster != 0) {
        fprintf(stderr, "FAT clusters are not aligned. Cannot convert in-place");
        exit(1);
    }
}

uint32_t sector_count() {
    return boot_sector.sector_count == 0
           ? boot_sector.total_sectors2
           : boot_sector.sector_count;

}

uint32_t data_cluster_count() {
    return ((sector_count() - meta_info.sectors_before_data) / boot_sector.sectors_per_cluster) + FAT_START_INDEX;
}

void read_volume_label(uint8_t* out) {
    if (boot_sector.ext_boot_signature == 0x28) {
        out[0] = 0;
    } else {
        size_t i = 10;
        while (boot_sector.volume_label[i] == ' ') {
            i--;
        }

        out[i + 1] = 0;
        memcpy(out, boot_sector.volume_label, i + 1);
    }
}
