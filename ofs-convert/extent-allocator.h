#ifndef OFS_CONVERT_BLOCK_ALLOCATE_H
#define OFS_CONVERT_BLOCK_ALLOCATE_H

#include <stdint.h>

#include "fat.h"

struct extent_allocator {
    uint32_t index_in_fat,
             blocked_extent_count;
    fat_extent *blocked_extents, *blocked_extent_current;
};
extern extent_allocator allocator;

void init_extent_allocator(fat_extent *blocked_extents, uint32_t blocked_extent_count);
fat_extent allocate_extent(uint16_t max_length);
uint32_t find_first_blocked_extent(uint32_t physical_address);
fat_extent* find_next_blocked_extent(uint32_t& i, uint32_t physical_end);

#endif //OFS_CONVERT_BLOCK_ALLOCATE_H
