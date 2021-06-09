use std::cell::{Cell, Ref, RefCell, RefMut};
use std::convert::TryFrom;
use std::io;
use std::iter::Step;
use std::ops::Range;

use crate::fat::ClusterIdx;
use crate::ranges::{NotCoveredRange, Ranges};

// TODO after reading, make directory dataclusters free
// TODO ensure DataClusterIdx can also not be constructed
#[derive(Clone, Copy, PartialEq, PartialOrd)]
pub struct AllocatedClusterIdx(ClusterIdx);
impl AllocatedClusterIdx {
    pub fn to_ne_bytes(self) -> [u8; 4] {
        self.0.to_ne_bytes()
    }
}

impl From<AllocatedClusterIdx> for u32 {
    fn from(idx: AllocatedClusterIdx) -> Self {
        idx.0
    }
}

impl From<AllocatedClusterIdx> for usize {
    fn from(idx: AllocatedClusterIdx) -> Self {
        idx.0 as usize
    }
}

impl Step for AllocatedClusterIdx {
    fn steps_between(start: &Self, end: &Self) -> Option<usize> {
        if start.0 > end.0 {
            None
        } else {
            Some((end.0 - start.0) as usize)
        }
    }

    fn forward_checked(start: Self, count: usize) -> Option<Self> {
        let to_add = u32::try_from(count).ok()?;
        start.0.checked_add(to_add).map(Self)
    }

    fn backward_checked(start: Self, count: usize) -> Option<Self> {
        let to_sub = u32::try_from(count).ok()?;
        start.0.checked_sub(to_sub).map(Self)
    }
}

// TODO modify to allow borrowing different clusters at the same time
/// Allocates clusters that are not marked as in use (specifically, clusters that are marked as
/// free in the FAT and which will not be overwritten by Ext4 block group metadata). Callers are
/// guaranteed that a cluster allocated to them will not be accessed anywhere else. They can access
/// such a cluster through the methods `cluster` and `cluster_mut`. The current implementation
/// panics if any two clusters are borrowed at the same time, which is kinda whack tbh.
#[derive(Debug)]
pub struct Allocator<'a> {
    partition_data: RefCell<&'a mut [u8]>,
    /// the cluster that the Allocator will try to allocate next
    cursor: Cell<ClusterIdx>,
    /// clusters before this index are marked as used and cannot be accessed over the methods `cluster` and
    /// `cluster_mut`
    first_valid_index: ClusterIdx,
    /// clusters that will not be allocated
    used_ranges: Ranges<ClusterIdx>,
    cluster_size: usize,
}

impl<'a> Allocator<'a> {
    pub fn new(partition_data: &'a mut [u8], cluster_size: usize, used_ranges: Ranges<ClusterIdx>) -> Self {
        Self {
            partition_data: RefCell::new(partition_data),
            cursor: Cell::new(0),
            first_valid_index: 0,
            used_ranges,
            cluster_size,
        }
    }

    pub fn forbid(&mut self, range: Range<ClusterIdx>) {
        self.used_ranges.insert(range);
    }

    /// Splits the Allocator into an AllocatedReader and an Allocator: the AllocatedReader can
    /// only read clusters that were allocated by `self`, the Allocator can only write and read
    /// clusters that could have been allocated by `self` but were not yet allocated.
    pub fn split_into_reader(self) -> (AllocatedReader<'a>, Self) {
        let cursor_byte = self.cursor.get() as usize * self.cluster_size;
        let (allocated, free) = self.partition_data.take().split_at_mut(cursor_byte);
        // TODO free ranges used for FAT dentries

        let reader = AllocatedReader {
            partition_data: allocated,
            cluster_size: self.cluster_size,
        };

        let allocator = Self {
            partition_data: RefCell::new(free),
            first_valid_index: self.cursor.get(),
            cursor: self.cursor,
            used_ranges: self.used_ranges,
            cluster_size: self.cluster_size,
        };

        (reader, allocator)
    }

    pub fn allocate_one(&mut self) -> AllocatedClusterIdx {
        self.allocate(1).start
    }

    /// Returns a cluster range that may be exclusively used by the caller with 1 <= `range.len()` <= `max_length`.
    // TODO error handling
    pub fn allocate(&self, max_length: usize) -> Range<AllocatedClusterIdx> {
        let free_range = self
            .find_next_free_range(self.cursor.get())
            .expect("Oh no, no more free blocks :(((");
        let range_end = free_range.end.min(free_range.start + max_length as u32);
        self.cursor.set(range_end);
        AllocatedClusterIdx(free_range.start)..AllocatedClusterIdx(range_end)
    }

    pub fn cluster(&'a self, idx: AllocatedClusterIdx) -> Ref<'a, [u8]> {
        let start_byte = self
            .cluster_start_byte(idx)
            .expect("Attempted to access an allocated cluster that has been made invalid");
        Ref::map(self.partition_data.borrow(), |data| {
            &data[start_byte..start_byte + self.cluster_size]
        })
    }

    pub fn cluster_mut(&self, idx: AllocatedClusterIdx) -> RefMut<[u8]> {
        let start_byte = self
            .cluster_start_byte(idx)
            .expect("Attempted to access an allocated cluster that has been made invalid");
        RefMut::map(self.partition_data.borrow_mut(), |data| {
            &mut data[start_byte..start_byte + self.cluster_size]
        })
    }

    /// Returns the position in `self.partition_data` at which the cluster `idx` starts or None if
    /// the cluster is not in `self.partition_data`.
    fn cluster_start_byte(&self, idx: AllocatedClusterIdx) -> Option<usize> {
        ClusterIdx::from(idx)
            .checked_sub(self.first_valid_index)
            .map(|relative_cluster_idx| self.cluster_size * relative_cluster_idx as usize)
    }

    /// Returns the next range at or after `self.cursor` that is not used, or Err if such a range does not exist.
    fn find_next_free_range(&self, cursor: u32) -> Result<Range<ClusterIdx>, io::Error> {
        let max_cluster_idx = (self.partition_data.borrow().len() / self.cluster_size) as u32;
        let non_used_range = self.used_ranges.next_not_covered(cursor);
        let non_used_range = match non_used_range {
            NotCoveredRange::Bounded(range) => range,
            NotCoveredRange::Unbounded(start) => start..max_cluster_idx,
        };

        if non_used_range.is_empty() {
            Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "No free clusters left in the partition",
            ))
        } else {
            Ok(non_used_range)
        }
    }
}


// no first_valid_index because in our use case it's always 0
pub struct AllocatedReader<'a> {
    partition_data: &'a [u8],
    cluster_size: usize,
}

impl<'a> AllocatedReader<'a> {
    pub fn cluster(&self, idx: AllocatedClusterIdx) -> &'a [u8] {
        let start_byte = self.cluster_size * usize::from(idx);
        &self.partition_data[start_byte..start_byte + self.cluster_size]
    }
}
