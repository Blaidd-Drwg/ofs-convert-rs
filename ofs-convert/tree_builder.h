#include <stdint.h>

struct StreamArchiver;

void build_ext4_root();
void build_lost_found();
void build_ext4_metadata_tree(uint32_t dir_inode_no, uint32_t parent_inode_no, StreamArchiver *read_stream);

