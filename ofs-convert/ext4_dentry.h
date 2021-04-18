#ifndef OFS_EXT4_DENTRY_H
#define OFS_EXT4_DENTRY_H
#include <stdint.h>

struct StreamArchiver;

constexpr int EXT4_NAME_LEN = 255;
constexpr int EXT4_DOT_DENTRY_SIZE = 12;

struct ext4_dentry {
    uint32_t inode;     /* Inode number */
    uint16_t rec_len;   /* Directory entry length */
    uint16_t name_len;  /* Name length */
    uint8_t  name[EXT4_NAME_LEN];    /* File name */
};

ext4_dentry *build_dentry(uint32_t inode_number, StreamArchiver *read_stream);
ext4_dentry build_dot_dir_dentry(uint32_t dir_inode_number);
ext4_dentry build_dot_dot_dir_dentry(uint32_t parent_inode_number);
ext4_dentry build_lost_found_dentry();

#endif //OFS_EXT4_DENTRY_H
