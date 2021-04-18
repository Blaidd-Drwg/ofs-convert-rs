#include "extent-allocator.h"
#include <stdlib.h>
#include <cstdio>

#include "visualizer.h"

extent_allocator allocator;
uint8_t *allocation_bitmap;

int extent_sort_compare(const void* eA, const void* eB) {
    return reinterpret_cast<const fat_extent*>(eA)->physical_start
         - reinterpret_cast<const fat_extent*>(eB)->physical_start;
}

void set_used(uint32_t cluster_no) {
    uint32_t byte = cluster_no / 8;
    allocation_bitmap[byte] |= (1 << (cluster_no % 8));
}

bool is_free(uint32_t cluster_no) {
    uint32_t byte = cluster_no / 8;
    return !(1 & (allocation_bitmap[byte] >> (cluster_no % 8)));
}

void create_allocation_bitmap() {
    uint32_t bitmap_size = ((data_cluster_count() - 1) / 8) + 1;
    allocation_bitmap = (uint8_t *) calloc(bitmap_size, 1);

    for (uint32_t cluster_no = 0; cluster_no < FAT_START_INDEX; cluster_no++) {
        set_used(cluster_no);
    }

    for (uint32_t cluster_no = FAT_START_INDEX; cluster_no < data_cluster_count(); cluster_no++) {
        if (!is_free_cluster(*fat_entry(cluster_no))) {
            set_used(cluster_no);
        }
    }
}

void init_extent_allocator(fat_extent *blocked_extents, uint32_t blocked_extent_count) {
    create_allocation_bitmap();
    allocator.index_in_fat = 0;
    allocator.blocked_extents = blocked_extents;
    allocator.blocked_extent_count = blocked_extent_count;
    qsort(allocator.blocked_extents, allocator.blocked_extent_count, sizeof(fat_extent), extent_sort_compare);
    allocator.blocked_extent_current = allocator.blocked_extents;
}

bool fs_is_full() {
    return allocator.blocked_extent_current - allocator.blocked_extents > allocator.blocked_extent_count;
}

bool can_be_used() {
    ++(allocator.index_in_fat);
    if(allocator.index_in_fat < allocator.blocked_extent_current->physical_start)
        return is_free(allocator.index_in_fat);

    allocator.index_in_fat = allocator.blocked_extent_current->physical_start + allocator.blocked_extent_current->length;
    ++allocator.blocked_extent_current;

    if (fs_is_full()) {
        fprintf(stderr, "File system is too small. All your data is trashed now, sorry!");
        exit(1);
    }
    return false;
}

fat_extent allocate_extent(uint16_t max_length) {
    while(!can_be_used());
    fat_extent result = {0, 1, allocator.index_in_fat};
    set_used(allocator.index_in_fat);

    while(result.length < max_length && can_be_used()) {
        result.length = allocator.index_in_fat - result.physical_start + 1;
        set_used(allocator.index_in_fat);
    }

    visualizer_add_allocated_extent(result);
    return result;
}

uint32_t find_first_blocked_extent(uint32_t physical_address) {
    uint32_t begin = 0, mid, end = allocator.blocked_extent_count;
    while(begin < end) {
        mid = (begin+end)/2;
        fat_extent* blocked_extent = &allocator.blocked_extents[mid];
        if(blocked_extent->physical_start + blocked_extent->length < physical_address)
            begin = mid+1;
        else
            end = mid;
    }
    return begin;
}

fat_extent* find_next_blocked_extent(uint32_t& i, uint32_t physical_end) {
    if(i >= allocator.blocked_extent_count)
        return NULL;
    fat_extent* blocked_extent = &allocator.blocked_extents[i++];
    if(physical_end < blocked_extent->physical_start)
        return NULL;
    return blocked_extent;
}
