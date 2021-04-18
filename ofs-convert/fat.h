#ifndef OFS_CONVERT_FAT_H
#define OFS_CONVERT_FAT_H

#include <stdint.h>

void set_meta_info(uint8_t *fs);
void read_boot_sector(uint8_t *fs);
void recursive_traverse(uint32_t cluster_no, uint16_t *long_name);

uint64_t fat_cl_to_e4blk(uint32_t cluster_no);
uint32_t e4blk_to_fat_cl(uint64_t block_no);
bool is_lfn(struct fat_dentry *dentry);
bool is_dir(struct fat_dentry *dentry);
bool is_invalid(struct fat_dentry *dentry);
bool is_dir_table_end(struct fat_dentry *dentry);
bool is_last_lfn_entry(struct fat_dentry *dentry);
bool is_dot_dir(struct fat_dentry *dentry);
bool has_lower_name(struct fat_dentry *dentry);
bool has_lower_extension(struct fat_dentry *dentry);
bool has_extension(struct fat_dentry *dentry);
uint8_t lfn_entry_sequence_no(struct fat_dentry *dentry);
uint32_t file_cluster_no(struct fat_dentry *dentry);
uint32_t *fat_entry(uint32_t cluster_no);
uint8_t *cluster_start(uint32_t cluster_no);
uint32_t fat_time_to_unix(uint16_t date, uint16_t time);
bool is_free_cluster(uint32_t cluster_entry);
void lfn_cpy(uint16_t *dest, uint8_t *src);
void read_short_name(struct fat_dentry *dentry, uint16_t *name);
uint32_t sector_count();
uint32_t data_cluster_count();
void read_volume_label(uint8_t* out);


// Index in the FAT of the first data cluster
constexpr uint32_t FAT_START_INDEX = 2;
constexpr uint32_t CLUSTER_ENTRY_MASK = 0x0FFFFFFF;
constexpr uint32_t FREE_CLUSTER = 0;
constexpr uint32_t FAT_END_OF_CHAIN = 0x0FFFFFF8;
constexpr uint8_t LFN_ENTRY_LENGTH = 13;

extern struct boot_sector boot_sector;
extern struct meta_info meta_info;

struct __attribute__((packed)) boot_sector {
    uint8_t jump_instruction[3];
    uint8_t oem_name[8];
    uint16_t bytes_per_sector;
    uint8_t sectors_per_cluster;
    uint16_t sectors_before_fat;
    uint8_t fat_count;
    uint16_t dir_entries;
    uint16_t sector_count;
    uint8_t media_descriptor;
    uint16_t unused2;
    uint16_t sectors_per_disk_track;
    uint16_t disk_heads;
    uint32_t hidden_sectors_before_partition;
    uint32_t total_sectors2;  // used if total_sectors would have overflown;
    uint32_t sectors_per_fat;
    uint16_t drive_description_flags;
    uint16_t version;
    uint32_t root_cluster_no;
    uint16_t fs_info_sector_no;
    uint16_t backup_boot_sector_no;
    uint8_t reserved[12];
    uint8_t physical_drive_no;
    uint8_t reserved2;
    uint8_t ext_boot_signature;
    uint32_t volume_id;
    uint8_t volume_label[11];
    uint64_t fs_type;
};

// This struct is not defined by FAT
struct fat_extent {
    uint32_t logical_start;  // First file cluster number that this extent covers
    uint16_t length;  // Number of clusters covered by extent
    uint32_t physical_start;  // Physical cluster number to which this extent points
};

struct meta_info {
    uint8_t* fs_start;
    uint32_t* fat_start;
    uint16_t fat_entries;
    uint32_t cluster_size;
    uint32_t dentries_per_cluster;
    uint32_t sectors_before_data;
    uint8_t* data_start;
};

struct fat_dentry {
    uint8_t short_name[8];
    uint8_t short_extension[3];
    uint8_t attrs;
    uint8_t short_name_case;
    uint8_t create_time_10_ms;
    uint16_t create_time;
    uint16_t create_date;
    uint16_t access_date;
    uint16_t first_cluster_high;
    uint16_t mod_time;
    uint16_t mod_date;
    uint16_t first_cluster_low;
    uint32_t file_size;
};

#endif //OFS_CONVERT_FAT_H
