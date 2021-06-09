use crate::fat::{ClusterIdx, FatDentry};
use std::ops::Range;

pub type Extent = Range<ClusterIdx>;

pub struct FatFile {
    pub name: String,
    pub dentry: FatDentry,
    pub data_ranges: Vec<Extent>,
}
