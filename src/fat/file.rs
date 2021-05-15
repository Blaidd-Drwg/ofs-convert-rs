use crate::fat::FatDentry;
use std::ops::Range;

pub type Extent = Range<u32>;

pub struct FatFile {
	pub name: String,
	pub dentry: FatDentry,
	pub data_ranges: Vec<Extent>,
}
