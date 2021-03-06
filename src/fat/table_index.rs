use std::convert::TryFrom;
use std::iter::Step;
use std::ops::Index;

use crate::fat::{BootSector, ClusterIdx};
use crate::util::FromU32;

/// The first FAT index belonging to the root directory. This corresponds to the first data cluster, i.e. the n-th FAT
/// entry corresponds to the (n-2)-th data cluster.
pub const ROOT_FAT_IDX: FatTableIndex = FatTableIndex(2);

/// An index identifying a FAT entry.
#[derive(PartialEq, Eq, Copy, Clone, PartialOrd, Ord)]
#[repr(transparent)]
pub struct FatTableIndex(u32);

impl FatTableIndex {
    pub const fn new(idx: u32) -> Self {
        Self(idx)
    }

    /// PANICS: Panics if `self` is a special value that does not represent an actual cluster
    pub fn to_data_cluster_idx(self) -> DataClusterIdx {
        assert!(!self.is_chain_end() && !self.is_zero_length_file() && !self.is_free());
        DataClusterIdx(self.0.checked_sub(ROOT_FAT_IDX.0).unwrap())
    }

    pub fn to_cluster_idx(self, boot_sector: &BootSector) -> ClusterIdx {
        let data_start_byte_idx = boot_sector.get_data_range().start;
        let data_start_cluster_idx =
            ClusterIdx::try_from(data_start_byte_idx / usize::fromx(boot_sector.cluster_size()))
                .expect("ClusterIdx must fit into u32");
        data_start_cluster_idx + u32::from(self.to_data_cluster_idx())
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
        &self[usize::from(idx)]
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

impl From<FatTableIndex> for usize {
    fn from(idx: FatTableIndex) -> Self {
        usize::fromx(idx.0)
    }
}


/// An index identifying a cluster in the data section of the filesystem.
#[derive(PartialEq, Eq, Copy, Clone, PartialOrd, Ord)]
pub struct DataClusterIdx(u32);
impl DataClusterIdx {
    pub fn to_fat_index(self) -> FatTableIndex {
        FatTableIndex(self.0 + ROOT_FAT_IDX.0)
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

impl Step for DataClusterIdx {
    fn steps_between(start: &Self, end: &Self) -> Option<usize> {
        end.0.checked_sub(start.0).map(|steps| usize::try_from(steps).ok())?
    }

    fn forward_checked(start: Self, count: usize) -> Option<Self> {
        let count = u32::try_from(count).ok()?;
        Some(DataClusterIdx(start.0.checked_add(count)?))
    }

    fn backward_checked(start: Self, count: usize) -> Option<Self> {
        let count = u32::try_from(count).ok()?;
        Some(DataClusterIdx(start.0.checked_sub(count)?))
    }
}
