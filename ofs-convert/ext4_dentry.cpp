#include "fat.h"
#include "ext4.h"
#include "ext4_extent.h"
#include "ext4_dentry.h"
#include "util.h"

#include <string.h>
#include <stdint.h>
#include <stdlib.h>
#include <unistd.h>
#include <sys/types.h>

uint32_t next_multiple_of_four(uint32_t n) {
    return ceildiv(n, 4u) * 4;
}

// Adapted from https://www.cprogramming.com/tutorial/utf8.c
int ucs2toutf8(uint8_t *dest, uint8_t *dest_end, uint16_t *src, int src_size) {
    uint16_t ch;
    uint8_t *pos = dest;
    for (int i = 0; i < src_size && src[i]; i++) {
        ch = src[i];
        if (ch < 0x80) {
            if (pos >= dest_end)
                return pos - dest;
            *pos++ = (char)ch;
        } else if (ch < 0x800) {
            if (pos >= dest_end-1)
                return pos - dest;
            *pos++ = (ch>>6) | 0xC0;
            *pos++ = (ch & 0x3F) | 0x80;
        } else {
            if (pos >= dest_end-2)
                return pos - dest;
            *pos++ = (ch>>12) | 0xE0;
            *pos++ = ((ch>>6) & 0x3F) | 0x80;
            *pos++ = (ch & 0x3F) | 0x80;
        }
    }
    return pos - dest;
}

// TODO check for EXT4_NAME_LIMIT
struct ext4_dentry *build_dentry(uint32_t inode_number, const uint8_t name[], size_t name_len) {
    ext4_dentry *ext_dentry = (ext4_dentry *) malloc(sizeof *ext_dentry);
    ext_dentry->inode = inode_number;
    memcpy(&ext_dentry->name, name, name_len);
    ext_dentry->name[name_len] = 0;
    ext_dentry->name_len = name_len; // terminating null byte isn't counted
    ext_dentry->rec_len = next_multiple_of_four(ext_dentry->name_len + 8);
    return ext_dentry;
}

ext4_dentry build_special_dentry(uint32_t inode_no, const char *name) {
    ext4_dentry dentry;
    dentry.inode = inode_no;
    dentry.name_len = strlen(name);
    strcpy((char *) dentry.name, name);
    dentry.rec_len = next_multiple_of_four(dentry.name_len + 8);
    return dentry;
}

ext4_dentry build_dot_dir_dentry(uint32_t dir_inode_no) {
    return build_special_dentry(dir_inode_no, ".");
}

ext4_dentry build_dot_dot_dir_dentry(uint32_t parent_inode_no) {
    return build_special_dentry(parent_inode_no, "..");
}

ext4_dentry build_lost_found_dentry() {
    return build_special_dentry(EXT4_LOST_FOUND_INODE, "lost+found");
}
