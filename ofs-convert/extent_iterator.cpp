#include <stdio.h>
#include "extent_iterator.h"
#include "stream-archiver.h"
#include "fat.h"

extent_iterator init(StreamArchiver *extent_stream) {
    extent_iterator iterator;
    iterator.current_cluster = 0;
    iterator.current_extent = getNext<fat_extent>(extent_stream);
    iterator.extent_stream = extent_stream;
    return iterator;
}

uint32_t next_cluster_no(extent_iterator *iterator) {
    if (!iterator->current_extent) {
        return 0;
    } else if (iterator->current_cluster >= iterator->current_extent->length) {
        *iterator = init(iterator->extent_stream);
        return next_cluster_no(iterator);
    }

    uint32_t cluster_no = iterator->current_extent->physical_start + iterator->current_cluster;
    iterator->current_cluster++;
    return cluster_no;
}
