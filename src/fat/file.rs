use std::ops::Range;

use crate::fat::{ClusterIdx, FatDentry};

pub type Extent = Range<ClusterIdx>;

pub struct FatFile {
    pub name: String,
    pub dentry: FatDentry,
    pub data_ranges: Vec<Extent>,
}
