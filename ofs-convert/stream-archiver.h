#ifndef OFS_CONVERT_SAR_H
#define OFS_CONVERT_SAR_H

#include <stdint.h>

extern uint64_t pageSize;
struct Page {
    struct Page* next;
};

struct StreamArchiver {
    Page* page;
    uint64_t offsetInPage,
             elementIndex;
    struct Header {
        uint64_t elementCount;
    } *header;
};

void cutStreamArchiver(StreamArchiver* stream);
void* iterateStreamArchiver(StreamArchiver* stream, bool insert, uint64_t elementLength, uint64_t elementCount = 1);

template <typename T>
T *getNext(StreamArchiver *stream) {
    return static_cast<T*>(iterateStreamArchiver(stream, false, sizeof(T)));
}

#endif //OFS_CONVERT_SAR_H
