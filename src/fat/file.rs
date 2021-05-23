use crate::fat::{ClusterIdx, FatTableIndex, FatDentry};
use std::ops::Range;

pub type Extent = Range<FatTableIndex>;

pub struct FatFile {
    pub name: String,
    pub lfn_entries: Vec<Vec<u16>>, // temporary addition for C compatibility
    pub dentry: FatDentry,
    pub data_ranges: Vec<Extent>,
}
