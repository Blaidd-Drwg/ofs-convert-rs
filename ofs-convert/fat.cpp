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

uint32_t *fat_entry(uint32_t cluster_no) {
    return meta_info.fat_start + cluster_no;
}

uint8_t *cluster_start(uint32_t cluster_no) {
    return meta_info.data_start + (cluster_no - FAT_START_INDEX) * static_cast<uint64_t>(meta_info.cluster_size);
}

bool is_free_cluster(uint32_t cluster_entry) {
    return (cluster_entry & CLUSTER_ENTRY_MASK) == FREE_CLUSTER;
}

uint32_t file_cluster_no(struct fat_dentry *dentry) {
    uint16_t low = dentry->first_cluster_low;
    uint32_t high = dentry->first_cluster_high << 16;
    return high | low;
}

bool is_dir(const struct fat_dentry *dentry) {
    return dentry->attrs & 0x10;
}

bool is_lfn(struct fat_dentry *dentry) {
    return dentry->attrs & 0x0F;
}

bool is_invalid(struct fat_dentry *dentry) {
    return *(uint8_t *) dentry == 0xE5;
}

bool is_dir_table_end(struct fat_dentry *dentry) {
    return !dentry || *(uint8_t *) dentry == 0x00;
}

bool is_dot_dir(struct fat_dentry *dentry) {
    return dentry->short_name[0] == '.';
}

bool is_last_lfn_entry(struct fat_dentry *dentry) {
    return *(uint8_t *) dentry & 0x40;
}

bool has_lower_name(struct fat_dentry *dentry) {
    return dentry->short_name_case & 0x8;
}

bool has_lower_extension(struct fat_dentry *dentry) {
    return dentry->short_name_case & 0x10;
}

bool has_extension(struct fat_dentry *dentry) {
    return dentry->short_extension[0] != ' ';
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

void lfn_cpy(uint16_t *dest, uint8_t *src) {
    memcpy(dest, src + 1, 5 * sizeof(uint16_t));
    memcpy(dest + 5, src + 14, 6 * sizeof(uint16_t));
    memcpy(dest + 11, src + 28, 2 * sizeof(uint16_t));
}

uint8_t lfn_entry_sequence_no(struct fat_dentry *dentry) {
    return *(uint8_t *) dentry & 0x1F;
}

void read_short_name(struct fat_dentry *dentry, uint16_t *name) {
    bool lower_name = has_lower_name(dentry);
    bool lower_extension = has_lower_extension(dentry);

    uint8_t *n = dentry->short_name;
    for (int i = 0; i < 8 && n[i] != ' '; i++) {
        *name = lower_name ? tolower(n[i]) : n[i];
        name++;
    }

    if (has_extension(dentry)) {
        *name = '.';
        name++;

        uint8_t *e = dentry->short_extension;
        for (int i = 0; i < 3 && e[i] != ' '; i++) {
            *name = lower_extension ? tolower(e[i]) : e[i];
            name++;
        }
    }
    *name = 0;
}

void read_boot_sector(uint8_t *fs) {
    boot_sector = *(struct boot_sector*) fs;
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
