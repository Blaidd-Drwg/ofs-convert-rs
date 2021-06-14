use std::cell::Cell;
use std::marker::PhantomData;
use std::ops::Range;
use std::{io, slice};

use crate::fat::ClusterIdx;
use crate::ranges::{NotCoveredRange, Ranges};

// TODO after reading, make directory dataclusters free
// TODO ensure DataClusterIdx can also not be constructed
/// An AllocatedClusterIdx represents a cluster that was allocated by Allocator and functions as a token to access that
/// cluster, either through the Allocator itself or through the AllocatedReader derived from it. Invariant: no two
/// AllocatedClusterIdx can have the same value; otherwise, `Allocator::cluster_mut` might alias.
#[derive(PartialEq, PartialOrd)]
pub struct AllocatedClusterIdx(ClusterIdx);
impl AllocatedClusterIdx {
    /// SAFETY: Cloning an AllocatedClusterIdx breaks the invariant! To avoid aliasing, the caller must ensure that the
    /// original and the clone are not used to access a cluster simultaneously.
    pub unsafe fn clone(&self) -> Self {
        Self(self.0)
    }

    /// SAFETY: This is safe since it cannot be converted back to an `AllocatedClusterIdx` or to a `DataClusterIdx`.
    // TODO make it so that it *actually* cannot be converted to a DataClusterIdx
    pub fn as_cluster_idx(&self) -> ClusterIdx {
        self.0
    }
}

impl From<AllocatedClusterIdx> for ClusterIdx {
    fn from(idx: AllocatedClusterIdx) -> Self {
        idx.0
    }
}

impl From<AllocatedClusterIdx> for usize {
    fn from(idx: AllocatedClusterIdx) -> Self {
        idx.0 as usize
    }
}

/// A newtype that can only be instantiated by Allocator to ensure that a range of AllocatedClusterIdx can only be used
/// as an iterator if every cluster in that range has indeed been allocated.
pub struct AllocatedRange(Range<AllocatedClusterIdx>);

impl AllocatedRange {
    pub fn len(&self) -> usize {
        self.0.end.0 as usize - self.0.start.0 as usize
    }

    pub fn iter_mut(&mut self) -> AllocatedIterMut {
        AllocatedIterMut::new(self)
    }
}

impl From<AllocatedRange> for Range<AllocatedClusterIdx> {
    fn from(range: AllocatedRange) -> Self {
        range.0
    }
}

impl From<AllocatedRange> for Range<ClusterIdx> {
    fn from(range: AllocatedRange) -> Self {
        range.0.start.into()..range.0.end.into()
    }
}

pub struct AllocatedIterMut<'a>(Range<ClusterIdx>, PhantomData<&'a ()>);
impl<'a> AllocatedIterMut<'a> {
    fn new(range: &'a mut AllocatedRange) -> Self {
        let range = range.0.start.0..range.0.end.0;
        Self(range, PhantomData)
    }
}

impl Iterator for AllocatedIterMut<'_> {
    type Item = AllocatedClusterIdx;
    fn next(&mut self) -> Option<Self::Item> {
        self.0.next().map(AllocatedClusterIdx)
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
    partition_ptr: *mut u8,
    partition_len: usize,
    /// the cluster that the Allocator will try to allocate next.
    /// Invariant: first_valid_index <= cursor <= partition_len * cluster_size
    cursor: Cell<ClusterIdx>,
    /// clusters before this index are marked as used and cannot be accessed over the methods `cluster` and
    /// `cluster_mut`
    first_valid_index: ClusterIdx,
    /// clusters that will not be allocated
    used_ranges: Ranges<ClusterIdx>,
    cluster_size: usize,
    _lifetime: PhantomData<&'a ()>,
}

impl<'a> Allocator<'a> {
    /// SAFETY: Instantiating more than one Allocator can lead to undefined behavior, as mixing AllocatedClusterIdx
    /// allocated by different Allocators can lead to aliasing
    pub unsafe fn new(
        partition_ptr: *mut u8,
        partition_len: usize,
        cluster_size: usize,
        used_ranges: Ranges<ClusterIdx>,
        _lifetime: PhantomData<&'a ()>,
    ) -> Self {
        Self {
            partition_ptr,
            partition_len,
            cursor: Cell::new(0),
            first_valid_index: 0,
            used_ranges,
            cluster_size,
            _lifetime,
        }
    }

    pub fn forbid(&mut self, range: Range<ClusterIdx>) {
        self.used_ranges.insert(range);
    }

    pub fn block_size(&self) -> usize {
        self.cluster_size
    }

    /// Splits the Allocator into an AllocatedReader and an Allocator: the AllocatedReader can
    /// only read clusters that were allocated by `self`, the Allocator can only write and read
    /// clusters that could have been allocated by `self` but were not yet allocated.
    pub fn split_into_reader(self) -> (AllocatedReader<'a>, Self) {
        let cursor_byte = self.cursor.get() as usize * self.cluster_size;
        // TODO free ranges used for FAT dentries

        let reader = AllocatedReader {
            partition_ptr: self.partition_ptr,
            partition_len: cursor_byte,
            cluster_size: self.cluster_size,
            _lifetime: self._lifetime,
        };

        // SAFETY: safe since the `partition_len` bytes after `partition_ptr` are valid memory and because of the
        // invariant `cursor_byte <= partition_len`, the `new_partition_len` bytes after `new_partition_ptr` are valid
        // memory as well.
        let new_partition_ptr = unsafe { self.partition_ptr.add(cursor_byte) };
        let new_partition_len = self.partition_len - cursor_byte;

        let allocator = Self {
            partition_ptr: new_partition_ptr,
            partition_len: new_partition_len,
            first_valid_index: self.cursor.get(),
            cursor: self.cursor,
            used_ranges: self.used_ranges,
            cluster_size: self.cluster_size,
            _lifetime: self._lifetime,
        };

        (reader, allocator)
    }

    pub fn allocate_one(&self) -> AllocatedClusterIdx {
        Range::from(self.allocate(1)).start
    }

    /// Returns a cluster range that may be exclusively used by the caller with 1 <= `range.len()` <= `max_length`.
    // TODO error handling
    pub fn allocate(&self, max_length: usize) -> AllocatedRange {
        let free_range = self
            .find_next_free_range(self.cursor.get())
            .expect("Oh no, no more free blocks :(((");
        let range_end = free_range.end.min(free_range.start + max_length as u32);
        self.cursor.set(range_end);
        AllocatedRange(AllocatedClusterIdx(free_range.start)..AllocatedClusterIdx(range_end))
    }

    pub fn cluster(&'a self, idx: &AllocatedClusterIdx) -> &[u8] {
        let start_byte = self
            .cluster_start_byte(idx)
            .expect("Attempted to access an allocated cluster that has been made invalid");
        assert!(start_byte + self.cluster_size < self.partition_len);
        // SAFETY: The data is valid and since `idx` is unique and we borrowed it, nobody else can mutate the data.
        unsafe { slice::from_raw_parts(self.partition_ptr.add(start_byte), self.cluster_size) }
    }

    pub fn cluster_mut(&self, idx: &mut AllocatedClusterIdx) -> &mut [u8] {
        let start_byte = self
            .cluster_start_byte(idx)
            .expect("Attempted to access an allocated cluster that has been made invalid");
        assert!(start_byte + self.cluster_size < self.partition_len);
        // SAFETY: The data is valid and since `idx` is unique and we borrowed it mutably, nobody else can access the
        // data.
        unsafe { slice::from_raw_parts_mut(self.partition_ptr.add(start_byte), self.cluster_size) }
    }

    /// Returns the position in `self.partition_data` at which the cluster `idx` starts or None if
    /// the cluster is not in `self.partition_data`.
    fn cluster_start_byte(&self, idx: &AllocatedClusterIdx) -> Option<usize> {
        idx.0
            .checked_sub(self.first_valid_index)
            .map(|relative_cluster_idx| self.cluster_size * relative_cluster_idx as usize)
    }

    /// Returns the next range at or after `self.cursor` that is not used, or Err if such a range does not exist.
    fn find_next_free_range(&self, cursor: u32) -> Result<Range<ClusterIdx>, io::Error> {
        let max_cluster_idx = (self.partition_len / self.cluster_size) as u32;
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
    partition_ptr: *const u8,
    partition_len: usize,
    cluster_size: usize,
    _lifetime: PhantomData<&'a ()>,
}

impl<'a> AllocatedReader<'a> {
    pub fn cluster(&self, idx: AllocatedClusterIdx) -> &'a [u8] {
        let start_byte = self.cluster_size * usize::from(idx);
        assert!(start_byte + self.cluster_size <= self.partition_len);
        // SAFETY: The data is valid and since `idx` is unique and we borrowed it mutably, nobody else can mutate the
        // data.
        unsafe { slice::from_raw_parts(self.partition_ptr.add(start_byte), self.cluster_size) }
    }
}
