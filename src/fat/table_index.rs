use std::convert::TryFrom;
use std::ops::{Add, AddAssign, Index};

use crate::fat::{BootSector, ClusterIdx, ROOT_FAT_IDX};

/// An index identifying a FAT entry.
#[derive(PartialEq, Eq, Copy, Clone, PartialOrd, Ord)]
#[repr(transparent)]
pub struct FatTableIndex(u32);
impl Add<u32> for FatTableIndex {
    type Output = Self;
    fn add(self, other: u32) -> Self {
        Self(self.0 + other)
    }
}

impl AddAssign<u32> for FatTableIndex {
    fn add_assign(&mut self, other: u32) {
        self.0 += other;
    }
}

impl FatTableIndex {
    pub const fn new(idx: u32) -> Self {
        Self(idx)
    }

    pub fn to_data_cluster_idx(self) -> DataClusterIdx {
        assert!(self.0 >= ROOT_FAT_IDX.0);
        DataClusterIdx::new(self.0 - ROOT_FAT_IDX.0)
    }

    pub fn to_cluster_idx(self, boot_sector: &BootSector) -> ClusterIdx {
        let data_start_byte_idx = boot_sector.get_data_range().start;
        let data_start_cluster_idx = data_start_byte_idx
            / (usize::from(boot_sector.bytes_per_sector) * usize::from(boot_sector.sectors_per_cluster));
        u32::from(self.to_data_cluster_idx()) + u32::try_from(data_start_cluster_idx).unwrap()
    }

    /// True if `self.0` is a special value representing the end of a FAT chain.
    pub fn is_chain_end(self) -> bool {
        const FAT_END_OF_CHAIN: u32 = 0x0FFFFFF8;
        self.0 >= FAT_END_OF_CHAIN
    }

    /// True if `self.0` is a special value representing a file with no data.
    pub fn is_zero_length_file(self) -> bool {
        self.0 == 0
    }

    // TODO move to struct FatTable
    /// True if `self.0` is a special value representing a free cluster.
    pub fn is_free(self) -> bool {
        const FREE_CLUSTER: u32 = 0;
        const CLUSTER_ENTRY_MASK: u32 = 0x0FFFFFFF;
        self.0 & CLUSTER_ENTRY_MASK == FREE_CLUSTER
    }
}

impl Index<FatTableIndex> for [FatTableIndex] {
    type Output = FatTableIndex;
    fn index(&self, idx: FatTableIndex) -> &Self::Output {
        &self[idx.0 as usize]
    }
}

impl TryFrom<usize> for FatTableIndex {
    type Error = std::num::TryFromIntError;
    fn try_from(idx: usize) -> Result<Self, Self::Error> {
        Ok(Self(u32::try_from(idx)?))
    }
}

impl From<FatTableIndex> for u32 {
    fn from(idx: FatTableIndex) -> Self {
        idx.0
    }
}


/// An index identifying a cluster in the data section of the partition.
// TODO unsafe: must be valid
#[derive(PartialEq, Eq, Copy, Clone, PartialOrd, Ord)]
pub struct DataClusterIdx(u32);
impl DataClusterIdx {
    pub const fn new(idx: u32) -> Self {
        Self(idx)
    }

    pub fn to_fat_index(self) -> FatTableIndex {
        FatTableIndex::new(self.0 + ROOT_FAT_IDX.0)
    }

    pub fn to_ne_bytes(self) -> [u8; 4] {
        self.0.to_ne_bytes()
    }
}

impl From<DataClusterIdx> for u32 {
    fn from(idx: DataClusterIdx) -> Self {
        idx.0
    }
}

impl From<DataClusterIdx> for usize {
    fn from(idx: DataClusterIdx) -> Self {
        idx.0 as Self
    }
}
