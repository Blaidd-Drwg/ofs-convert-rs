use std::cell::Cell;
use std::fmt::{Display, Formatter};
use std::marker::PhantomData;
use std::ops::Range;
use std::slice;

use anyhow::{bail, Result};

use crate::ext4::{BlockIdx, BlockIdx_from};
use crate::fat::ClusterIdx;
use crate::ranges::{NotCoveredRange, Ranges};

// TODO after/during serialization, mark directory dataclusters as free
/// An `AllocatedClusterIdx` represents a cluster that was allocated by an `Allocator` and functions as a token to
/// access that cluster, either through the `Allocator` itself or through the `AllocatedReader` derived from it.
/// Invariant: no two `AllocatedClusterIdx` may have the same value; otherwise, `Allocator::cluster_mut` might alias.
#[derive(PartialEq, PartialOrd)]
pub struct AllocatedClusterIdx(ClusterIdx);
impl AllocatedClusterIdx {
    /// SAFETY: Instantiating an `AllocatedClusterIdx` might break the invariant! To avoid aliasing, the caller must
    /// ensure that `idx` was originally created from an `AllocatedClusterIdx` and that the original and the clone are
    /// not used to access a cluster simultaneously.
    pub unsafe fn new(idx: ClusterIdx) -> Self {
        Self(idx)
    }

    /// SAFETY: Cloning an `AllocatedClusterIdx` breaks the invariant! To avoid aliasing, the caller must ensure that
    /// the original and the clone are not used to access a cluster simultaneously.
    pub unsafe fn clone(&self) -> Self {
        Self(self.0)
    }

    /// SAFETY: This is safe since it cannot be converted back to an `AllocatedClusterIdx` or to a `DataClusterIdx`.
    pub fn as_cluster_idx(&self) -> ClusterIdx {
        self.0
    }

    /// SAFETY: This is safe since it cannot be converted back to an `AllocatedClusterIdx` or to a `DataClusterIdx`.
    pub fn as_block_idx(&self) -> BlockIdx {
        BlockIdx_from(self.0)
    }
}

impl From<AllocatedClusterIdx> for ClusterIdx {
    fn from(idx: AllocatedClusterIdx) -> Self {
        idx.0
    }
}

impl From<AllocatedClusterIdx> for usize {
    fn from(idx: AllocatedClusterIdx) -> Self {
        idx.0 as Self
    }
}

impl Display for AllocatedClusterIdx {
    fn fmt(&self, formatter: &mut Formatter) -> std::result::Result<(), std::fmt::Error> {
        self.0.fmt(formatter)
    }
}

/// A newtype that can only be instantiated by an `Allocator` to ensure that a range of `AllocatedClusterIdx` can only
/// be used as an iterator if every cluster in that range has indeed been allocated.
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

/// Allocates clusters that are not marked as in use (specifically, clusters that are marked as free in the FAT and
/// which will not be overwritten by Ext4 block group metadata). Callers are guaranteed that a cluster allocated to them
/// will not be accessed anywhere else. They can access such a cluster through the methods `cluster` and `cluster_mut`.
// A pretty cool thing that Rust's type system allows you to do is using lifetimes to "brand" an `AllocatedClusterIdx`
// so that it can be ensured at compile-time that it's only ever used by the `Allocator` that instantiated it (see the
// `ghost-cell` crate for reference). Unfortunately, the only way to do so at the moment is quite hacky (the `Allocator`
// can only be used within a closure that is passed to its constructor), so we decided against it to not overcomplicated
// `Allocator`'s interface.
#[derive(Debug)]
pub struct Allocator<'a> {
    fs_ptr: *mut u8,
    fs_len: usize,
    /// the cluster that the Allocator will try to allocate next.
    /// Invariant: first_valid_index <= cursor <= fs_len * cluster_size
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
    /// SAFETY: Instantiating more than one `Allocator` can lead to undefined behavior, as mixing `AllocatedClusterIdx`
    /// allocated by different `Allocator`s can lead to aliasing.
    pub unsafe fn new(
        fs_ptr: *mut u8,
        fs_len: usize,
        cluster_size: usize,
        used_ranges: Ranges<ClusterIdx>,
        _lifetime: PhantomData<&'a ()>,
    ) -> Self {
        Self {
            fs_ptr,
            fs_len,
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

    /// Splits the `Allocator` into an `AllocatedReader` and an `Allocator`: the `AllocatedReader` can
    /// only read clusters that were allocated by `self`, the `Allocator` can only write and read
    /// clusters that could have been allocated by `self` but were not yet allocated.
    pub fn split_into_reader(self) -> (AllocatedReader<'a>, Self) {
        let cursor_byte = self.cursor.get() as usize * self.cluster_size;
        let reader = AllocatedReader {
            fs_ptr: self.fs_ptr,
            fs_len: cursor_byte,
            cluster_size: self.cluster_size,
            _lifetime: self._lifetime,
        };

        // SAFETY: safe since the `fs_len` bytes after `fs_ptr` are valid memory and because of the
        // invariant `cursor_byte <= fs_len`, the `new_fs_len` bytes after `new_fs_ptr` are valid
        // memory as well.
        let new_fs_ptr = unsafe { self.fs_ptr.add(cursor_byte) };
        let new_fs_len = self.fs_len - cursor_byte;

        let allocator = Self {
            fs_ptr: new_fs_ptr,
            fs_len: new_fs_len,
            first_valid_index: self.cursor.get(),
            cursor: self.cursor,
            used_ranges: self.used_ranges,
            cluster_size: self.cluster_size,
            _lifetime: self._lifetime,
        };

        (reader, allocator)
    }

    /// Returns a cluster that may be exclusively used by the caller.
    pub fn allocate_one(&self) -> Result<AllocatedClusterIdx> {
        Ok(Range::from(self.allocate(1)?).start)
    }

    /// Returns a cluster range that may be exclusively used by the caller with 1 <= `range.len()` <= `max_length`.
    pub fn allocate(&self, max_length: usize) -> Result<AllocatedRange> {
        let free_range = self.find_next_free_range(self.cursor.get())?;
        let range_end = free_range.end.min(free_range.start + max_length as u32);
        self.cursor.set(range_end);
        Ok(AllocatedRange(
            AllocatedClusterIdx(free_range.start)..AllocatedClusterIdx(range_end),
        ))
    }

    /// PANICS: Panics if `idx` out of bounds. This is only possible if `idx` was not allocated by `self`.
    pub fn cluster(&'a self, idx: &AllocatedClusterIdx) -> &[u8] {
        let start_byte = self
            .cluster_start_byte(idx)
            .unwrap_or_else(|| panic!("Attempted to access invalid cluster {}", idx));
        // SAFETY: The data is valid and since `idx` is unique and we borrowed it, nobody else can mutate the data.
        unsafe { slice::from_raw_parts(self.fs_ptr.add(start_byte), self.cluster_size) }
    }

    /// PANICS: Panics if `idx` out of bounds. This is only possible if `idx` was not allocated by `self`.
    pub fn cluster_mut(&self, idx: &mut AllocatedClusterIdx) -> &mut [u8] {
        let start_byte = self
            .cluster_start_byte(idx)
            .unwrap_or_else(|| panic!("Attempted to access invalid cluster {}", idx));
        // SAFETY: The data is valid and since `idx` is unique and we borrowed it mutably, nobody else can access the
        // data.
        unsafe { slice::from_raw_parts_mut(self.fs_ptr.add(start_byte), self.cluster_size) }
    }

    pub fn free_block_count(&self) -> usize {
        self.used_ranges.free_element_count(self.cursor.get(), self.max_cluster_idx())
    }

    /// Returns the offset from `self.fs_ptr` at which the cluster `idx` starts or None if the cluster is not covered by
    /// `self`, i.e. if the offset is not in `0..=self.fs_len - self.cluster_size`.
    fn cluster_start_byte(&self, idx: &AllocatedClusterIdx) -> Option<usize> {
        idx.0
            .checked_sub(self.first_valid_index)
            .map(|relative_cluster_idx| self.cluster_size * relative_cluster_idx as usize)
            .filter(|start_byte| start_byte + self.cluster_size <= self.fs_len)
    }

    /// Returns the next range at or after `self.cursor` that is not used, or Err if such a range does not exist.
    fn find_next_free_range(&self, cursor: u32) -> Result<Range<ClusterIdx>> {
        let non_used_range = match self.used_ranges.next_not_covered(cursor) {
            NotCoveredRange::Bounded(range) => range,
            NotCoveredRange::Unbounded(start) => start..self.max_cluster_idx(),
        };

        if non_used_range.is_empty() {
            bail!("No free clusters left in the filesystem")
        } else {
            Ok(non_used_range)
        }
    }

    fn max_cluster_idx(&self) -> ClusterIdx {
        self.first_valid_index + (self.fs_len / self.cluster_size) as u32
    }
}


/// Allows to read clusters that were allocated by the `Allocator` instance that produced `self`, but not to allocate
/// any clusters.
// no first_valid_index because in our use case it's always 0
pub struct AllocatedReader<'a> {
    fs_ptr: *const u8,
    fs_len: usize,
    cluster_size: usize,
    _lifetime: PhantomData<&'a ()>,
}

impl<'a> AllocatedReader<'a> {
    /// PANICS: Panics if `idx` out of bounds. This is only possible if `idx` was not allocated by the `Allocator` that
    /// produced `self`.
    pub fn cluster(&self, idx: &AllocatedClusterIdx) -> &'a [u8] {
        let start_byte = self
            .cluster_start_byte(idx)
            .unwrap_or_else(|| panic!("Attempted to access invalid cluster {}", idx));
        // SAFETY: The data is valid and since `idx` is unique and we borrowed it, nobody can mutate the data.
        unsafe { slice::from_raw_parts(self.fs_ptr.add(start_byte), self.cluster_size) }
    }

    /// Returns the offset from `self.fs_ptr` at which the cluster `idx` starts or None if the cluster is not covered by
    /// `self`, i.e. if the offset is not in `0..=self.fs_len - self.cluster_size`.
    fn cluster_start_byte(&self, idx: &AllocatedClusterIdx) -> Option<usize> {
        let start_byte = idx.0 as usize * self.cluster_size;
        if start_byte + self.cluster_size <= self.fs_len {
            Some(start_byte)
        } else {
            None
        }
    }
}
