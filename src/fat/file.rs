use std::ops::RangeInclusive;

use crate::fat::{DataClusterIdx, FatDentry};

pub struct FatFile {
    pub name: String,
    pub dentry: FatDentry,
    pub data_ranges: Vec<RangeInclusive<DataClusterIdx>>,
}
