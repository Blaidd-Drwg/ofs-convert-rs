#ifndef OFS_CONVERT_PARTITION_H
#define OFS_CONVERT_PARTITION_H

#include <stdint.h>
#include <sys/stat.h>

struct Partition {
    const char* path;
    int mmapFlags, file;
    struct stat fileStat;
    uint8_t* ptr;
};

void closePartition(Partition* partition);
bool openPartition(Partition* partition);

#endif //OFS_CONVERT_PARTITION_H
