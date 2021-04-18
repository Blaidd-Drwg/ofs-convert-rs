#include "ext4_extent.h"
#include "fat.h"
#include "visualizer.h"
#include "stream-archiver.h"
#include "extent-allocator.h"
#include "extent_iterator.h"

#include <ctype.h>
#include <stdio.h>
#include <string.h>

extern uint64_t pageSize;

struct cluster_read_state {
    extent_iterator iterator;
    uint32_t cluster_dentry;
    fat_dentry *current_cluster;
};

cluster_read_state init_read_state(extent_iterator iterator) {
    cluster_read_state state;
    uint32_t cluster_no = next_cluster_no(&iterator);
    state.current_cluster = cluster_no ? reinterpret_cast<fat_dentry *>(cluster_start(cluster_no)) : NULL;
    state.cluster_dentry = 0;
    state.iterator = iterator;
    return state;
}

fat_dentry* next_dentry(cluster_read_state* state) {
    fat_dentry* ret;
    do {
        if (state->cluster_dentry >= meta_info.dentries_per_cluster)
            *state = init_read_state(state->iterator);

        if (!state->current_cluster)
            return NULL;

        ret = state->current_cluster + state->cluster_dentry;
        state->cluster_dentry++;
    } while (is_invalid(ret) || is_dot_dir(ret));
    return ret;
}

void reserve_name(uint16_t* pointers[], int count, StreamArchiver* write_stream) {
    for (int i = 0; i < count; i++) {
        pointers[i] = reinterpret_cast<uint16_t*>(iterateStreamArchiver(write_stream, true, LFN_ENTRY_LENGTH * sizeof(uint16_t)));
    }
    cutStreamArchiver(write_stream);
}

fat_dentry* reserve_dentry(StreamArchiver* write_stream) {
    void* ptr = iterateStreamArchiver(write_stream, true, sizeof(fat_dentry));
    cutStreamArchiver(write_stream);
    return reinterpret_cast<fat_dentry*>(ptr);
}

fat_extent* reserve_extent(StreamArchiver* write_stream) {
    void* ptr = iterateStreamArchiver(write_stream, true, sizeof(fat_extent));
    return reinterpret_cast<fat_extent*>(ptr);
}

uint32_t* reserve_children_count(StreamArchiver* write_stream) {
    void* ptr = iterateStreamArchiver(write_stream, true, sizeof(uint32_t));
    cutStreamArchiver(write_stream);
    return reinterpret_cast<uint32_t*>(ptr);
}

void resettle_extent(uint32_t cluster_no, bool is_dir_flag, StreamArchiver* write_stream, fat_extent& input_extent) {
    for(uint16_t i = 0; i < input_extent.length; ) {
        fat_extent fragment = allocate_extent(input_extent.length - i);
        fragment.logical_start = input_extent.logical_start + i;
        *reserve_extent(write_stream) = fragment;
        memcpy(cluster_start(fragment.physical_start), cluster_start(input_extent.physical_start + i), fragment.length * meta_info.cluster_size);
        if (!is_dir_flag) {
            visualizer_add_block_range({BlockRange::ResettledPayload, fat_cl_to_e4blk(fragment.physical_start), fragment.length, cluster_no});
        }

        i += fragment.length;
    }
}

void find_blocked_extent_fragments(uint32_t cluster_no, bool is_dir_flag, StreamArchiver* write_stream, const fat_extent& input_extent) {
    uint32_t input_physical_end = input_extent.physical_start + input_extent.length,
             fragment_physical_start = input_extent.physical_start,
             i = find_first_blocked_extent(input_extent.physical_start);
    fat_extent* blocked_extent = find_next_blocked_extent(i, input_physical_end);
    while(fragment_physical_start < input_physical_end) {
        uint32_t fragment_physical_end = input_physical_end;
        bool is_blocked = blocked_extent && blocked_extent->physical_start <= fragment_physical_start;
        if(is_blocked) {
            uint32_t blocked_physical_end = blocked_extent->physical_start + blocked_extent->length;
            if(blocked_physical_end < fragment_physical_end)
                fragment_physical_end = blocked_physical_end;
            blocked_extent = find_next_blocked_extent(i, input_physical_end);
        } else if(blocked_extent)
            fragment_physical_end = blocked_extent->physical_start;

        fat_extent fragment;
        fragment.physical_start = fragment_physical_start;
        fragment.length = fragment_physical_end - fragment.physical_start;
        fragment.logical_start = input_extent.logical_start + (fragment.physical_start - input_extent.physical_start);
        fragment_physical_start = fragment_physical_end;
        if (!is_dir_flag) {
            visualizer_add_block_range({BlockRange::OriginalPayload, fat_cl_to_e4blk(fragment.physical_start), fragment.length, cluster_no});
        }

        if(is_blocked)
            resettle_extent(cluster_no, is_dir_flag, write_stream, fragment);
        else
            *reserve_extent(write_stream) = fragment;
    }
}

void aggregate_extents(uint32_t cluster_no, bool is_dir_flag, StreamArchiver* write_stream) {
    if(!is_dir_flag)
        visualizer_add_tag(cluster_no);
    fat_extent current_extent {0, 1, cluster_no};
    uint32_t next_cluster_no = *fat_entry(cluster_no);

    while(cluster_no) {  // if cluster_no == 0, it's a zero-length file
        bool is_end = next_cluster_no >= FAT_END_OF_CHAIN,
             is_consecutive = next_cluster_no == current_extent.physical_start + current_extent.length,
             has_max_length = current_extent.length == EXT4_MAX_INIT_EXTENT_LEN;
        if(is_end || !is_consecutive || has_max_length) {
            find_blocked_extent_fragments(cluster_no, is_dir_flag, write_stream, current_extent);
            current_extent.logical_start += current_extent.length;
            current_extent.length = 1;
            current_extent.physical_start = next_cluster_no;
        } else
            ++current_extent.length;
        if(is_end)
            break;
        next_cluster_no = *fat_entry(next_cluster_no);
    }
    cutStreamArchiver(write_stream);
}

fat_dentry* read_lfn(fat_dentry* first_entry, StreamArchiver* extent_stream, uint16_t* name[], int lfn_entry_count, struct cluster_read_state* state) {
    uint8_t* entry = reinterpret_cast<uint8_t*>(first_entry);
    for (int i = lfn_entry_count - 1; i >= 0; i--) {
        lfn_cpy(name[i], entry);
        entry = reinterpret_cast<uint8_t*>(next_dentry(state));
    }
    return reinterpret_cast<fat_dentry*>(entry);
}

void traverse(StreamArchiver* dir_extent_stream, StreamArchiver* write_stream) {
    uint32_t* children_count = reserve_children_count(write_stream);
    *children_count = 0;
    cluster_read_state state = init_read_state(init(dir_extent_stream));
    fat_dentry* current_dentry = next_dentry(&state);

    while (!is_dir_table_end(current_dentry)) {
        fat_dentry* dentry = reserve_dentry(write_stream);

        bool has_long_name = is_lfn(current_dentry);
        if (has_long_name) {
            int lfn_entry_count = lfn_entry_sequence_no(current_dentry);
            uint16_t* name[lfn_entry_count];
            reserve_name(name, lfn_entry_count, write_stream);
            current_dentry = read_lfn(current_dentry, dir_extent_stream, name, lfn_entry_count, &state);
        } else {
            uint16_t* name[1];
            reserve_name(name, 1, write_stream);
            read_short_name(current_dentry, name[0]);
        }

        // current_dentry is the actual dentry now
        memcpy(dentry, current_dentry, sizeof *current_dentry);

        uint32_t cluster_no = file_cluster_no(current_dentry);
        StreamArchiver read_extent_stream = *write_stream;
        bool is_dir_flag = is_dir(current_dentry);
        aggregate_extents(cluster_no, is_dir_flag, write_stream);
        if (is_dir_flag) {
            traverse(&read_extent_stream, write_stream);
        } else {
            *reserve_children_count(write_stream) = -1;
        }

        (*children_count)++;
        current_dentry = next_dentry(&state);
    }
}

void init_stream_archiver(StreamArchiver* stream, uint32_t clusterSize) {
    pageSize = clusterSize;
    memset(stream, 0, sizeof *stream);
    cutStreamArchiver(stream);
}
