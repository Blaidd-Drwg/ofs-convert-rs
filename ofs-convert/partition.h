#ifndef OFS_CONVERT_PARTITION_H
#define OFS_CONVERT_PARTITION_H

#include <stdint.h>
#include <sys/stat.h>
#include <stddef.h>

struct Partition {
    size_t size;
    uint8_t* ptr;
};

#endif //OFS_CONVERT_PARTITION_H
