use crate::fat::ClusterIdx;
use std::ops::Deref;
use std::convert::TryFrom;
const FAT_END_OF_CHAIN: u32 = 0x0FFFFFF8;

#[derive(Copy, Clone)]
pub struct FatTable<'a> {
    table_data: &'a [ClusterIdx],
}

impl<'a> Deref for FatTable<'a> {
    type Target = [ClusterIdx];
    fn deref(&self) -> &Self::Target {
        self.table_data
    }
}

impl<'a> FatTable<'a> {
    pub fn new(table_data: &'a [ClusterIdx]) -> Self {
        Self { table_data }
    }

    /// Given a reference to the first FAT entry of a file, returns an iterator of all the file's
    /// clusters (including the one in the first FAT entry)
    pub fn file_cluster_iter(self, first_cluster_idx: ClusterIdx) -> impl Iterator<Item = ClusterIdx> + 'a {
        FatEntryIter { current_cluster_idx: first_cluster_idx, fat_table: self }
    }
}


pub struct FatEntryIter<'a> {
    current_cluster_idx: ClusterIdx,
    fat_table: FatTable<'a>,
}

impl<'a> FatEntryIter<'a> {
    pub fn new(start_cluster_idx: ClusterIdx, fat_table: FatTable<'a>) -> Self {
        Self { current_cluster_idx: start_cluster_idx, fat_table }
    }

    /// True if this is the last cluster of a file
    fn is_chain_end(cluster_idx: ClusterIdx) -> bool {
        cluster_idx >= FAT_END_OF_CHAIN
    }

    /// True if the file this cluster belongs to has size 0
    fn is_zero_length(cluster_idx: ClusterIdx) -> bool {
        cluster_idx == 0
    }
}

impl<'a> Iterator for FatEntryIter<'a> {
    type Item = ClusterIdx;
    fn next(&mut self) -> Option<Self::Item> {
        if Self::is_chain_end(self.current_cluster_idx) || Self::is_zero_length(self.current_cluster_idx) {
            None
        } else {
            let result = self.current_cluster_idx;
            self.current_cluster_idx = self.fat_table[usize::try_from(result).unwrap()];
            Some(result)
        }
    }
}
