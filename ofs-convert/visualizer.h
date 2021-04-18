#ifndef OFS_CONVERT_VISUALIZER_H
#define OFS_CONVERT_VISUALIZER_H

#include <stdlib.h>
#include <stdint.h>

#include "fat.h"

struct BlockRange {
    enum Type {
        #define ENTRY(name, color) name,
        #include "visualizer_types.h"
        #undef ENTRY
    } type;
    uint64_t begin, length, tag;
    BlockRange* next;
};

void visualizer_add_allocated_extent(const fat_extent& extent);
void visualizer_add_tag(uint64_t tag);
void visualizer_add_block_range(BlockRange to_add);
void visualizer_render_to_file(const char* path, uint32_t block_count);

#endif //OFS_CONVERT_VISUALIZER_H
