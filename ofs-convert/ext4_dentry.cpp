#include "fat.h"
#include "ext4.h"
#include "ext4_extent.h"
#include "ext4_dentry.h"
#include "extent-allocator.h"
#include "stream-archiver.h"
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

struct ext4_dentry *build_dentry(uint32_t inode_number, StreamArchiver *read_stream) {
    ext4_dentry *ext_dentry = (ext4_dentry *) malloc(sizeof *ext_dentry);
    ext_dentry->inode = inode_number;
    ext_dentry->name_len = 0;

    uint8_t *ext_name = ext_dentry->name;
    uint8_t *ext_name_limit = ext_name + EXT4_NAME_LEN - 1;
    uint16_t *segment = (uint16_t *) iterateStreamArchiver(read_stream, false,
                                                           LFN_ENTRY_LENGTH * sizeof *segment);
    while (segment != NULL) {
        int bytes_written = ucs2toutf8(ext_name + ext_dentry->name_len, ext_name_limit,
                                       segment, LFN_ENTRY_LENGTH);
        ext_dentry->name_len += bytes_written;
        segment = (uint16_t *) iterateStreamArchiver(read_stream, false,
                                                     LFN_ENTRY_LENGTH * sizeof *segment);
    }
    ext_dentry->name[ext_dentry->name_len] = '\0';
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
