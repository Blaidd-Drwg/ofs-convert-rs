#include <stdint.h>
#include "fat.h"
#include <string.h>

struct StreamArchiver;

void init_stream_archiver(StreamArchiver* stream, uint32_t clusterSize);
void aggregate_extents(uint32_t cluster_no, bool is_dir_flag, StreamArchiver* write_stream);
void traverse(StreamArchiver* dir_extent_stream, StreamArchiver* write_stream);


void add_regular_file(
		StreamArchiver* write_stream,
		fat_dentry dentry,
		const uint16_t lfn_entries[],
		size_t lfn_entry_count,
		const fat_extent extents[],
		size_t extent_count);
uint32_t* add_dir(
		StreamArchiver* write_stream,
		fat_dentry dentry,
		const uint16_t lfn_entries[],
		size_t lfn_entry_count,
		const fat_extent extents[],
		size_t extent_count);
