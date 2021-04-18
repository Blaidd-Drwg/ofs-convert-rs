#include <stdint.h>

struct StreamArchiver;

void init_stream_archiver(StreamArchiver* stream, uint32_t clusterSize);
void aggregate_extents(uint32_t cluster_no, bool is_dir_flag, StreamArchiver* write_stream);
void traverse(StreamArchiver* dir_extent_stream, StreamArchiver* write_stream);
