#ifndef OFS_CONVERT_EXTENT_ITERATOR_H
#define OFS_CONVERT_EXTENT_ITERATOR_H

#include <stdint.h>
#include "fat.h"
#include "stream-archiver.h"

struct extent_iterator {
    fat_extent *current_extent;
    uint32_t current_cluster;
    StreamArchiver *extent_stream;
};

extent_iterator init(StreamArchiver *extent_stream);
uint32_t next_cluster_no(extent_iterator *iterator);
#endif //OFS_CONVERT_EXTENT_ITERATOR_H
