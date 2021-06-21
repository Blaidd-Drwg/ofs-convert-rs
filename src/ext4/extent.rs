use std::mem::size_of;
use std::ops::Range;
use std::slice;

use static_assertions::const_assert_eq;

use crate::allocator::{AllocatedClusterIdx, Allocator};
use crate::ext4::EXTENT_ENTRIES_IN_INODE;
use crate::fat::ClusterIdx;

const_assert_eq!(size_of::<Extent>(), size_of::<ExtentTreeElement>());
const_assert_eq!(size_of::<ExtentHeader>(), size_of::<ExtentTreeElement>());
const_assert_eq!(size_of::<ExtentIdx>(), size_of::<ExtentTreeElement>());

const EXTENT_TREE_LEAF_DEPTH: u16 = 0;
const EXTENT_MAGIC: u16 = 0xF30A;
const EXTENT_INODE_BLOCK_SIZE: usize = size_of::<[ExtentTreeElement; EXTENT_ENTRIES_IN_INODE]>();

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
    pub fn new(range: Range<ClusterIdx>, logical_start: u32) -> Self {
        Self {
            logical_start,
            len: range.len() as u16,
            physical_start_hi: 0, /* FAT uses 32 bits to address sectors, so there can't be a block with a higher
                                   * address */
            physical_start_lo: range.start,
        }
    }

    pub fn start(&self) -> u32 {
        self.physical_start_lo
    }

    pub fn end(&self) -> u32 {
        self.start() + self.len as u32
    }

    pub fn as_range(&self) -> Range<ClusterIdx> {
        self.start() as ClusterIdx..self.end() as ClusterIdx
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
        let mut allocated_cluster_idx = AllocatedClusterIdx::new(self.leaf_lo);
        let cluster = allocator.cluster_mut(&mut allocated_cluster_idx);
        let (_, entries, _) = cluster.align_to_mut::<ExtentTreeElement>();
        ExtentTreeLevel::new(entries)
    }
}

impl ExtentHeader {
    pub fn new(all_entry_count: u16) -> Self {
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

    pub fn add_extent(&mut self, extent: Extent) -> Vec<ClusterIdx> {
        match self.root.add_extent(extent, self.allocator) {
            Ok(allocated_blocks) => allocated_blocks,
            Err(_) => {
                let block_for_previous_root = self.make_deeper();
                let mut allocated_blocks =
                    self.root.add_extent(extent, self.allocator).expect("Unable to register extent");
                allocated_blocks.push(block_for_previous_root);
                allocated_blocks
            }
        }
    }

    fn make_deeper(&mut self) -> ClusterIdx {
        let mut new_block_idx = self.allocator.allocate_one();
        let cluster_idx = new_block_idx.as_cluster_idx();
        let new_block = self.allocator.cluster_mut(&mut new_block_idx);
        // SAFETY: Safe since we later overwrite the first `root_slice.len()` entries and mark all others as invalid
        let (_, new_entries, _) = unsafe { new_block.align_to_mut::<ExtentTreeElement>() };
        assert!(new_entries.len() >= EXTENT_ENTRIES_IN_INODE);
        self.root.header.max_entry_count = (new_entries.len() - 1) as u16;

        let root_slice = self.root.as_slice();
        new_entries[..root_slice.len()].copy_from_slice(root_slice);

        *self.root.header = ExtentHeader::from_child(*self.root.header, EXTENT_ENTRIES_IN_INODE as u16);
        self.root
            .append_extent_idx(ExtentIdx::new(0, new_block_idx))
            .expect("Unable to add ExtentIdx within the inode");
        cluster_idx
    }
}

impl<'a> ExtentTreeLevel<'a> {
    /// SAFETY: Safe if the entries in `entries` form a consistent extent tree level. In particular:
    /// - `entries[0]` must be an `ExtentHeader`;
    /// - every entry in `entries[1..header.valid_entry_count]` must be either:
    ///     - a valid `Extent` if `header.depth == 0`. In particular, for every entry `entry`:
    ///         - every block in `entry.as_range` must be a data block.
    ///     - a valid `ExtentIdx` if `header.depth > 0`. In particular, for every entry `entry`:
    ///         - `entry` must point to a block that also represents a consistent extent tree level;
    ///         - the header `child_header` of the block pointed to by `entry` must have `child_header.depth ==
    ///           header.depth - 1`.
    /// TODO what if entries.len() == 1? should be ok if depth == 0, otherwise not
    pub unsafe fn new(entries: &'a mut [ExtentTreeElement]) -> Self {
        let (header_slice, used_entries) = entries.split_at_mut(1);
        let header = &mut header_slice[0].header;
        assert_eq!(header.max_entry_count as usize, used_entries.len());

        Self { header, all_entries: used_entries }
    }

    fn as_slice(&mut self) -> &mut [ExtentTreeElement] {
        // SAFETY: safe because it reconstructs the slice with which `self` was constructed
        unsafe {
            let header_ptr = self.header as *mut _ as *mut ExtentTreeElement;
            slice::from_raw_parts_mut(header_ptr, 1 + self.all_entries.len())
        }
    }

    pub fn add_extent(&mut self, extent: Extent, allocator: &Allocator<'a>) -> Result<Vec<ClusterIdx>, ()> {
        // try to append directly to self
        if self.header.is_leaf() {
            // if this did not work, there is nothing we as a leaf can do about it
            return self.append_extent(extent).map(|_| Vec::new());
        }

        // we are not a leaf, try to append to the last child below us
        if let Ok(allocated_blocks) = self.last_child_level(allocator).add_extent(extent, allocator) {
            return Ok(allocated_blocks);
        }

        // all leaves below us are full, try adding a new leaves; if we have no space left for a new leaf, give up
        self.add_extent_with_new_leaf(extent, allocator)
    }

    fn valid_entries_mut(&mut self) -> &mut [ExtentTreeElement] {
        &mut self.all_entries[..self.header.valid_entry_count as usize]
    }

    fn last_child_level<'b>(&'b mut self, allocator: &'b Allocator<'b>) -> ExtentTreeLevel<'b> {
        assert!(!self.header.is_leaf());
        // SAFETY: Safe because if `self` is not a leaf level, all of its entries are `ExtentIdx`s, and we access one
        // that is valid.
        unsafe {
            self.valid_entries_mut()
                .last_mut()
                .unwrap() // we are not a leaf, so we have at least one child
                .idx
                .level_mut(allocator)
        }
    }

    fn add_extent_with_new_leaf(&mut self, extent: Extent, allocator: &Allocator<'_>) -> Result<Vec<ClusterIdx>, ()> {
        if self.header.is_full() {
            return Err(());
        }

        let allocated_block = self.add_child_level(extent.logical_start, allocator);
        let mut child_level = self.last_child_level(allocator);
        if child_level.header.is_leaf() {
            child_level.append_extent(extent).map(|_| vec![allocated_block])
        } else {
            child_level
                .add_extent_with_new_leaf(extent, allocator)
                .map(|mut allocated_blocks| {
                    allocated_blocks.push(allocated_block);
                    allocated_blocks
                })
        }
    }

    fn add_child_level(&mut self, logical_start: u32, allocator: &Allocator<'_>) -> ClusterIdx {
        let mut new_child_block_idx = allocator.allocate_one();
        let cluster_idx = new_child_block_idx.as_cluster_idx();
        let new_child_block = allocator.cluster_mut(&mut new_child_block_idx);
        // SAFETY: Safe because we replace the header and regard all other entries as invalid.
        let (_, entries, _) = unsafe { new_child_block.align_to_mut::<ExtentTreeElement>() };

        entries[0].header = ExtentHeader::from_parent(*self.header, entries.len() as u16);

        self.append_extent_idx(ExtentIdx::new(logical_start, new_child_block_idx))
            .expect("Failed to add an ExtentIdx to an empty tree level");
        cluster_idx
    }

    pub fn append_extent(&mut self, extent: Extent) -> Result<(), ()> {
        assert!(self.header.is_leaf());
        self.append_entry(ExtentTreeElement { extent })
    }

    pub fn append_extent_idx(&mut self, idx: ExtentIdx) -> Result<(), ()> {
        assert!(!self.header.is_leaf());
        self.append_entry(ExtentTreeElement { idx })
    }

    /// Appends an entry to `self.entries`, returns Err is `entries` is already full.
    fn append_entry(&mut self, entry: ExtentTreeElement) -> Result<(), ()> {
        if self.header.is_full() {
            Err(())
        } else {
            let idx = self.header.valid_entry_count as usize;
            self.all_entries[idx] = entry;
            self.header.valid_entry_count += 1;
            Ok(())
        }
    }
}
