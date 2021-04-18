#ifndef OFS_EXT4_EXTENT_H
#define OFS_EXT4_EXTENT_H
#include <stdint.h>

constexpr uint16_t EH_MAGIC = 0xF30A;

constexpr uint16_t EXT4_MAX_INIT_EXTENT_LEN = 32768;

struct fat_extent;
struct fat_dentry;
struct StreamArchiver;
struct ext4_super_block;

struct ext4_extent_header {
    uint16_t eh_magic = EH_MAGIC;
    uint16_t eh_entries; /* number of valid entries */
    uint16_t eh_max;  /* capacity of store in entries */
    uint16_t eh_depth; /* has tree real underlying blocks? */
    uint32_t eh_generation = 0; /* generation of the tree */
};

struct ext4_extent {
    uint32_t ee_block; /* first logical block extent covers */
    uint16_t ee_len;  /* number of blocks covered by extent */
    uint16_t ee_start_hi; /* high 16 bits of physical block */
    uint32_t ee_start_lo; /* low 32 bits of physical block */
};

struct ext4_extent_idx {
    uint32_t ei_block; /* index covers logical blocks from 'block' */
    uint32_t ei_leaf_lo; /* pointer to the physical block of the next *
     * level. leaf or next index could be there */
    uint16_t ei_leaf_hi; /* high 16 bits of physical block */
    uint16_t ei_unused;
};

struct ext4_extent_tail {
    uint32_t et_checksum; /* crc32c(uuid+inum+extent_block) */
};

ext4_extent_header init_extent_header();
void register_extent(fat_extent *ext, uint32_t inode_number, bool add_to_extent_tree = true);
void set_extents(uint32_t inode_number, fat_dentry *dentry, StreamArchiver *read_stream);
ext4_extent last_extent(uint32_t inode_number);

#endif //OFS_EXT4_EXTENT_H
