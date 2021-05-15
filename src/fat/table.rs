use crate::fat::{ClusterIdx, FatClusterIter};
use std::ops::Deref;

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
    const FAT_END_OF_CHAIN: u32 = 0x0FFFFFF8;

    pub fn new(table_data: &'a [ClusterIdx]) -> Self {
        Self { table_data }
    }

    /// Given a reference to the first FAT entry of a file, returns an iterator of all the file's
    /// clusters (including the one in the first FAT entry)
    pub fn file_cluster_iter(self, first_cluster_idx: ClusterIdx) -> impl Iterator<Item = ClusterIdx> + 'a {
        FatClusterIter { current_cluster_idx: first_cluster_idx, fat_table: self }
    }
}
