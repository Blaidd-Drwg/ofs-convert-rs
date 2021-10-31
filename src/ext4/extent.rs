use std::convert::TryFrom;
use std::mem::size_of;
use std::ops::Range;
use std::slice;

use anyhow::{bail, Context, Result};
use static_assertions::const_assert_eq;

use crate::allocator::{AllocatedClusterIdx, Allocator};
use crate::ext4::{BlockCount, BlockIdx, BlockSize, EXTENT_ENTRIES_IN_INODE};
use crate::lohi::{LoHi, LoHiMut};
use crate::util::{checked_add, FromU32, FromUsize};

const_assert_eq!(size_of::<Extent>(), size_of::<ExtentTreeElement>());
const_assert_eq!(size_of::<ExtentHeader>(), size_of::<ExtentTreeElement>());
const_assert_eq!(size_of::<ExtentIdx>(), size_of::<ExtentTreeElement>());

const EXTENT_TREE_LEAF_DEPTH: u16 = 0;
const EXTENT_MAGIC: u16 = 0xF30A;
const MAX_EXTENT_ENTRIES_PER_BLOCK: usize = u16::MAX as usize; // must fit into `ExtentHeader.max_entry_count`

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct Extent {
    pub logical_start: u32,
    pub len: u16,
    pub physical_start_hi: u16,
    pub physical_start_lo: u32,
}

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct ExtentIdx {
    /// logical_start of the first extent of the first leaf below self
    pub logical_start: u32,
    pub leaf_lo: u32,
    pub leaf_hi: u16,
    pub _padding: u16,
}

#[repr(C)]
#[derive(Copy, Clone)]
pub union ExtentTreeElement {
    pub header: ExtentHeader,
    pub idx: ExtentIdx,
    pub extent: Extent,
}

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct ExtentHeader {
    pub magic: u16,
    pub valid_entry_count: u16,
    pub max_entry_count: u16,
    pub depth: u16,
    pub generation: u32,
}

impl Extent {
    pub const MAX_LEN: usize = u16::MAX as usize;

    /// PANICS: Panics if `range.len() > Extent::MAX_LEN`.
    pub fn new(data_range: Range<BlockIdx>, logical_start: u32) -> Self {
        let mut instance = Self {
            logical_start,
            len: u16::try_from(data_range.len()).expect("Attempted to create an extent longer than 65535 blocks"),
            physical_start_hi: 0,
            physical_start_lo: 0,
        };
        LoHiMut::new(&mut instance.physical_start_lo, &mut instance.physical_start_hi)
            .set(u64::fromx(data_range.start));
        instance
    }

    pub fn start(&self) -> BlockIdx {
        let start: u64 = LoHi::new(&self.physical_start_lo, &self.physical_start_hi).get();
        BlockIdx::try_from(start).expect("Start was originally a BlockIdx")
    }

    pub fn end(&self) -> BlockIdx {
        self.start() + BlockIdx::from(self.len)
    }

    pub fn as_range(&self) -> Range<BlockIdx> {
        self.start()..self.end()
    }

    pub fn from_ranges<I>(data_ranges: I) -> Result<Vec<Self>>
    where I: IntoIterator<Item = Range<BlockIdx>> {
        let mut logical_start = 0u32;
        let mut extents = Vec::new();
        for mut range in data_ranges {
            while !range.is_empty() {
                let range_len = range.len().min(Self::MAX_LEN);
                let range_first_part = range.start..range.start + range_len;
                extents.push(Self::new(range_first_part, logical_start));
                logical_start = checked_add(logical_start, range_len).context("File too large")?;
                range.start += range_len;
            }
        }
        Ok(extents)
    }
}

impl ExtentIdx {
    pub fn new(logical_start: u32, physical_start: AllocatedClusterIdx) -> Self {
        Self {
            logical_start,
            leaf_lo: physical_start.into(),
            leaf_hi: 0,
            _padding: 0,
        }
    }

    /// SAFETY: Safe only if `self` is consistent, i.e. if the block with the referenced index contains a consistent
    /// extent tree level.
    unsafe fn level_mut<'a>(&'a mut self, allocator: &'a Allocator<'a>) -> ExtentTreeLevel<'a> {
        // SAFETY: Safe since `self.leaf_lo` came from an `AllocatedClusterIdx`, and since it only survives as long as
        // we have a mutable borrow on `self`, ensuring it cannot be duplicated.
        unsafe {
            let mut allocated_cluster_idx = AllocatedClusterIdx::new(self.leaf_lo);
            let block = allocator.cluster_mut(&mut allocated_cluster_idx);
            let (_, entries, _) = block.align_to_mut::<ExtentTreeElement>();
            ExtentTreeLevel::new(entries)
        }
    }
}

impl ExtentHeader {
    pub fn new(all_entry_count: u16) -> Self {
        assert!(all_entry_count > 1);
        Self {
            magic: EXTENT_MAGIC,
            valid_entry_count: 0,
            max_entry_count: all_entry_count - 1, // the first entry is the header itself
            depth: 0,
            generation: 0,
        }
    }

    pub fn from_child(parent: Self, all_entry_count: u16) -> Self {
        Self {
            depth: parent.depth + 1,
            ..Self::new(all_entry_count)
        }
    }

    pub fn from_parent(parent: Self, all_entry_count: u16) -> Self {
        assert!(parent.depth > 0);
        Self {
            depth: parent.depth - 1,
            ..Self::new(all_entry_count)
        }
    }

    /// True if this is the lowest level of the extent tree, i.e. if the `ExtentOrIdx` following `self` are `Extent`s.
    pub fn is_leaf(&self) -> bool {
        self.depth == EXTENT_TREE_LEAF_DEPTH
    }

    /// True if no further entries can be added after this header
    pub fn is_full(&self) -> bool {
        self.valid_entry_count == self.max_entry_count
    }

    /// Performs a sanity check on whether the invariants on `self` hold.
    pub fn is_valid(&self) -> bool {
        let entry_count_is_valid = self.valid_entry_count <= self.max_entry_count;
        let non_leaf_has_at_least_one_child = self.depth == 0 || self.valid_entry_count > 0;
        entry_count_is_valid && non_leaf_has_at_least_one_child
    }
}

// SAFETY: Any entry `self.all_entries[i]` with `i >= self.header.valid_entry_count` is inconsistent. Reading such an
// entry is undefined behavior.
pub struct ExtentTreeLevel<'a> {
    header: &'a mut ExtentHeader,
    all_entries: &'a mut [ExtentTreeElement],
}

pub struct ExtentTree<'a> {
    /// The root level located inside the inode (`exxt_header` and `extents`)
    root: ExtentTreeLevel<'a>,
    allocator: &'a Allocator<'a>,
}

impl<'a> ExtentTree<'a> {
    pub fn new(root_level: ExtentTreeLevel<'a>, allocator: &'a Allocator<'a>) -> Self {
        Self { root: root_level, allocator }
    }

    pub fn required_block_count(extent_count: usize, block_size: BlockSize) -> BlockCount {
        if extent_count == 0 {
            return 0;
        }

        let extents_per_block = (usize::fromx(block_size) / size_of::<ExtentTreeElement>()) - 1;
        let level_count = 1
            + (extent_count as f64 / (EXTENT_ENTRIES_IN_INODE - 1) as f64)
                .log(extents_per_block as f64)
                .ceil() as u32;

        let mut result = 0;
        for level in 1..level_count {
            let blocks_in_level = extent_count.div_ceil(extents_per_block.pow(level));
            result += blocks_in_level;
        }
        result
    }

    pub fn add_extent(&mut self, extent: Extent) -> Result<Vec<BlockIdx>> {
        match self.root.add_extent(extent, self.allocator) {
            Ok(allocated_blocks) => Ok(allocated_blocks),
            Err(_) => {
                let block_for_previous_root = self.make_deeper()?;
                let mut allocated_blocks = self
                    .root
                    .add_extent(extent, self.allocator)
                    .expect("Unable to add new extent despite `make_deeper` succeeding");
                allocated_blocks.push(block_for_previous_root);
                Ok(allocated_blocks)
            }
        }
    }

    fn make_deeper(&mut self) -> Result<BlockIdx> {
        let mut new_block_idx = self.allocator.allocate_one()?;
        let block_idx = new_block_idx.as_block_idx();
        let new_block = self.allocator.cluster_mut(&mut new_block_idx);
        // SAFETY: Safe since we later overwrite the first `root_slice.len()` entries and mark all others as invalid
        let (_, mut new_entries, _) = unsafe { new_block.align_to_mut::<ExtentTreeElement>() };
        let entry_count = new_entries.len().min(MAX_EXTENT_ENTRIES_PER_BLOCK);
        new_entries = &mut new_entries[..entry_count];
        assert!(entry_count >= usize::from(EXTENT_ENTRIES_IN_INODE));
        self.root.header.max_entry_count = u16::try_from(entry_count - 1).unwrap();

        let root_slice = self.root.as_slice();
        new_entries[..root_slice.len()].copy_from_slice(root_slice);

        *self.root.header = ExtentHeader::from_child(*self.root.header, EXTENT_ENTRIES_IN_INODE);
        self.root
            .append_extent_idx(ExtentIdx::new(0, new_block_idx))
            .expect("Unable to add ExtentIdx within the inode");
        Ok(block_idx)
    }
}

impl<'a> ExtentTreeLevel<'a> {
    /// SAFETY: Safe if the entries in `entries` form a consistent extent tree level. In particular:
    /// - `entries[0]` must be a valid `ExtentHeader`;
    /// - every entry in `entries[1..header.valid_entry_count]` must be either:
    ///     - a valid `Extent` if `header.depth == 0`. In particular, for every entry `entry`:
    ///         - every block in `entry.as_range` must be a data block.
    ///     - a valid `ExtentIdx` if `header.depth > 0`. In particular, for every entry `entry`:
    ///         - `entry` must point to a block that also represents a consistent extent tree level;
    ///         - the header `child_header` of the block pointed to by `entry` must have `child_header.depth ==
    ///           header.depth - 1`.
    pub unsafe fn new(entries: &'a mut [ExtentTreeElement]) -> Self {
        let (header_slice, used_entries) = entries.split_at_mut(1);
        let header = unsafe { &mut header_slice[0].header };
        assert!(header.is_valid());
        assert_eq!(usize::from(header.max_entry_count), used_entries.len());

        Self { header, all_entries: used_entries }
    }

    fn as_slice(&mut self) -> &mut [ExtentTreeElement] {
        // SAFETY: safe because it reconstructs the slice with which `self` was constructed
        unsafe {
            let header_ptr = self.header as *mut _ as *mut ExtentTreeElement;
            slice::from_raw_parts_mut(header_ptr, 1 + self.all_entries.len())
        }
    }

    /// Returns the `BlockIdx`s of the extent tree blocks allocated for this operation, or None if the tree below
    /// `self` is already full.
    pub fn add_extent(&mut self, extent: Extent, allocator: &Allocator<'a>) -> Result<Vec<BlockIdx>> {
        // try to append directly to self
        if self.header.is_leaf() {
            // if this did not work, there is nothing we as a leaf can do about it
            return self.append_extent(extent).map(|_| Vec::new());
        }

        // we are not a leaf, try to append to the last child below us
        if let Ok(allocated_blocks) = self.last_child_level(allocator).add_extent(extent, allocator) {
            return Ok(allocated_blocks);
        }

        // all leaves below us are full, try adding a new leaf; if we have no space left for a new leaf, give up
        self.add_extent_with_new_leaf(extent, allocator)
    }

    fn valid_entries_mut(&mut self) -> &mut [ExtentTreeElement] {
        &mut self.all_entries[..usize::from(self.header.valid_entry_count)]
    }

    /// PANICS: Panics if `self` is a leaf level.
    fn last_child_level<'b>(&'b mut self, allocator: &'b Allocator<'b>) -> ExtentTreeLevel<'b> {
        assert!(
            !self.header.is_leaf(),
            "Attempted to access the child of a leaf level in the extent tree"
        );
        // SAFETY: Safe because if `self` is not a leaf level, all of its entries are `ExtentIdx`s, and we access one
        // that is valid.
        unsafe {
            self.valid_entries_mut()
                .last_mut()
                .expect("Non-leaf extent tree level has no children")
                .idx
                .level_mut(allocator)
        }
    }

    /// Returns the `BlockIdx`s of the extent tree blocks allocated for this operation.
    fn add_extent_with_new_leaf(&mut self, extent: Extent, allocator: &Allocator<'_>) -> Result<Vec<BlockIdx>> {
        let allocated_block = self.add_child_level(extent.logical_start, allocator)?;
        let mut child_level = self.last_child_level(allocator);
        if child_level.header.is_leaf() {
            child_level
                .append_extent(extent)
                .expect("Unable to append extent to newly added extent tree leaf level");
            Ok(vec![allocated_block])
        } else {
            let mut allocated_blocks = child_level
                .add_extent_with_new_leaf(extent, allocator)
                .expect("Unable to append extent below a newly added extent tree level");
            allocated_blocks.push(allocated_block);
            Ok(allocated_blocks)
        }
    }

    /// Returns the `BlockIdx` of the block allocated for the new child level, or None if no child level can be added
    /// because `self` is full.
    fn add_child_level(&mut self, logical_start: u32, allocator: &Allocator<'_>) -> Result<BlockIdx> {
        if self.header.is_full() {
            bail!("Extent tree level full, cannot add new child level");
        }

        let mut new_child_block_idx = allocator.allocate_one()?;
        let block_idx = new_child_block_idx.as_block_idx();
        let new_child_block = allocator.cluster_mut(&mut new_child_block_idx);
        // SAFETY: Safe because we replace the header and regard all other entries as invalid.
        let (_, mut entries, _) = unsafe { new_child_block.align_to_mut::<ExtentTreeElement>() };
        let entry_count = entries.len().min(MAX_EXTENT_ENTRIES_PER_BLOCK);
        entries = &mut entries[..entry_count];

        entries[0].header = ExtentHeader::from_parent(*self.header, u16::try_from(entry_count).unwrap());

        self.append_extent_idx(ExtentIdx::new(logical_start, new_child_block_idx))
            .and(Ok(block_idx))
    }

    /// PANICS: Panics if `self` is not a leaf level
    pub fn append_extent(&mut self, extent: Extent) -> Result<()> {
        assert!(
            self.header.is_leaf(),
            "Attempted to append an extent to a non-leaf level of the extent tree"
        );
        self.append_entry(ExtentTreeElement { extent })
    }

    /// PANICS: Panics if `self` is a leaf level
    pub fn append_extent_idx(&mut self, idx: ExtentIdx) -> Result<()> {
        assert!(
            !self.header.is_leaf(),
            "Attempted to append an extent index to a leaf level of the extent tree"
        );
        self.append_entry(ExtentTreeElement { idx })
    }

    /// Appends an entry to `self.entries`, returns Err is `entries` is already full.
    fn append_entry(&mut self, entry: ExtentTreeElement) -> Result<()> {
        if self.header.is_full() {
            bail!("Extent tree level full, cannot append new entry")
        } else {
            let idx = usize::from(self.header.valid_entry_count);
            self.all_entries[idx] = entry;
            self.header.valid_entry_count += 1;
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inode_extents() {
        assert_eq!(
            ExtentTree::required_block_count(usize::from(EXTENT_ENTRIES_IN_INODE) - 1, 1024),
            0,
        );
    }

    #[test]
    fn full_four_levels() {
        const LEVEL_COUNT: usize = 4;
        const BLOCK_SIZE: BlockSize = 4096;
        let (extent_count, block_count) = perfect_extent_tree(LEVEL_COUNT, BLOCK_SIZE);
        assert_eq!(ExtentTree::required_block_count(extent_count, BLOCK_SIZE), block_count,);
    }

    #[test]
    fn full_four_levels_plus_one() {
        // compute extent and block counts of a full three-level tree first
        const LEVEL_COUNT: usize = 4;
        const BLOCK_SIZE: BlockSize = 4096;
        let (mut extent_count, mut block_count) = perfect_extent_tree(LEVEL_COUNT, BLOCK_SIZE);

        // adding one extent makes the tree deeper (1 block) and adds one more path towards a leaf starting at the
        // second level (3 blocks)
        extent_count += 1;
        block_count += 1 + 3;
        assert_eq!(ExtentTree::required_block_count(extent_count, BLOCK_SIZE), block_count,);
    }

    /// Returns the extent count and block count of an extent tree with `level_count` levels in which adding one more
    /// extent would require adding another level.
    fn perfect_extent_tree(level_count: usize, block_size: BlockSize) -> (usize, usize) {
        assert!(level_count > 0);
        let extents_per_block = (usize::fromx(block_size) / size_of::<ExtentTreeElement>()) - 1;
        assert!(extents_per_block > 1);
        let mut current_level_extent_count = usize::from(EXTENT_ENTRIES_IN_INODE) - 1;
        let mut current_level_block_count = 0; // inode
        for _current_level in 1..level_count {
            current_level_block_count += current_level_extent_count;
            current_level_extent_count *= extents_per_block;
        }
        (current_level_extent_count, current_level_block_count)
    }
}
