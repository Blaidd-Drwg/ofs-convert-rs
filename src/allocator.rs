use std::cell::Cell;
use std::convert::TryFrom;
use std::fmt::{Display, Formatter};
use std::marker::PhantomData;
use std::ops::Range;
use std::slice;

use anyhow::{bail, Result};

use crate::ext4::BlockIdx;
use crate::fat::ClusterIdx;
use crate::ranges::{NotCoveredRange, Ranges};
use crate::util::{AddUsize, FromU32};

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
        BlockIdx::fromx(self.0)
    }
}

impl From<AllocatedClusterIdx> for ClusterIdx {
    fn from(idx: AllocatedClusterIdx) -> Self {
        idx.0
    }
}

impl From<AllocatedClusterIdx> for usize {
    fn from(idx: AllocatedClusterIdx) -> Self {
        usize::fromx(idx.0)
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
    pub fn len(&self) -> u32 {
        self.0.end.0 - self.0.start.0
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
    /// clusters outside this range can neither be allocated nor accessed over the methods `cluster` and `cluster_mut`
    valid_cluster_indices: Range<ClusterIdx>,
    /// the cluster that the Allocator will try to allocate next.
    /// Invariant: `valid_cluster_indices.contains(cursor.get())`
    cursor: Cell<ClusterIdx>,
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
        let valid_cluster_count =
            u32::try_from(fs_len / cluster_size).expect("FAT32 cannot have more than 2^32 clusters");
        Self {
            fs_ptr,
            cursor: Cell::new(0),
            valid_cluster_indices: 0..valid_cluster_count,
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

    /// Returns a cluster that may be exclusively used by the caller.
    pub fn allocate_one(&self) -> Result<AllocatedClusterIdx> {
        Ok(Range::from(self.allocate(1)?).start)
    }

    /// Returns a cluster range that may be exclusively used by the caller, with 1 <= `range.len()` <= `max_length`.
    pub fn allocate(&self, max_length: u32) -> Result<AllocatedRange> {
        let free_range = self.find_next_free_range(self.cursor.get())?;
        let desired_end = free_range.start.saturating_add(max_length);
        let range_end = free_range.end.min(desired_end);
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
        unsafe { slice::from_raw_parts(self.fs_ptr.add_usize(start_byte), self.cluster_size) }
    }

    /// PANICS: Panics if `idx` out of bounds. This is only possible if `idx` was not allocated by `self`.
    pub fn cluster_mut(&self, idx: &mut AllocatedClusterIdx) -> &mut [u8] {
        let start_byte = self
            .cluster_start_byte(idx)
            .unwrap_or_else(|| panic!("Attempted to access invalid cluster {}", idx));
        // SAFETY: The data is valid and since `idx` is unique and we borrowed it mutably, nobody else can access the
        // data.
        unsafe { slice::from_raw_parts_mut(self.fs_ptr.add_usize(start_byte), self.cluster_size) }
    }

    pub fn free_block_count(&self) -> usize {
        self.used_ranges
            .free_element_count(self.cursor.get(), self.fs_end_cluster_idx())
    }

    /// Returns the offset from `self.fs_ptr` at which the cluster `idx` starts or None if the cluster is not covered by
    /// `self`, i.e. if `idx` is not in `self.valid_cluster_indices`.
    fn cluster_start_byte(&self, idx: &AllocatedClusterIdx) -> Option<usize> {
        let cluster_idx = idx.as_cluster_idx();
        if self.valid_cluster_indices.contains(&cluster_idx) {
            self.cluster_size.checked_mul(usize::fromx(cluster_idx))
        } else {
            None
        }
    }

    /// Returns the next range at or after `self.cursor` that is not used, or Err if such a range does not exist.
    fn find_next_free_range(&self, cursor: u32) -> Result<Range<ClusterIdx>> {
        let non_used_range = match self.used_ranges.next_not_covered(cursor) {
            NotCoveredRange::Bounded(range) => range,
            NotCoveredRange::Unbounded(start) => start..self.fs_end_cluster_idx(),
        };

        if non_used_range.is_empty() {
            bail!("No free clusters left in the filesystem")
        } else {
            Ok(non_used_range)
        }
    }

    fn fs_end_cluster_idx(&self) -> ClusterIdx {
        self.valid_cluster_indices.end
    }

    /// Splits the `Allocator` into an `AllocatedReader` and an `Allocator`: the `AllocatedReader` can
    /// only read clusters that were allocated by `self`, the `Allocator` can only write and read
    /// clusters that could have been allocated by `self` but were not yet allocated.
    pub fn split_into_reader(self) -> (AllocatedReader<'a>, Self) {
        let reader = AllocatedReader {
            fs_ptr: self.fs_ptr,
            valid_cluster_indices: self.valid_cluster_indices.start..self.cursor.get(),
            cluster_size: self.cluster_size,
            _lifetime: self._lifetime,
        };

        let allocator = Self {
            fs_ptr: self.fs_ptr,
            valid_cluster_indices: self.cursor.get()..self.valid_cluster_indices.end,
            cursor: self.cursor,
            used_ranges: self.used_ranges,
            cluster_size: self.cluster_size,
            _lifetime: self._lifetime,
        };

        (reader, allocator)
    }
}


/// Allows to read clusters that were allocated by the `Allocator` instance that produced `self`, but not to allocate
/// any clusters.
pub struct AllocatedReader<'a> {
    fs_ptr: *const u8,
    valid_cluster_indices: Range<ClusterIdx>,
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
        unsafe { slice::from_raw_parts(self.fs_ptr.add_usize(start_byte), self.cluster_size) }
    }

    /// Returns the offset from `self.fs_ptr` at which the cluster `idx` starts or None if the cluster is not covered by
    /// `self`, i.e. if `idx` is not in `self.valid_cluster_indices`.
    fn cluster_start_byte(&self, idx: &AllocatedClusterIdx) -> Option<usize> {
        let cluster_idx = idx.as_cluster_idx();
        if self.valid_cluster_indices.contains(&cluster_idx) {
            self.cluster_size.checked_mul(usize::fromx(cluster_idx))
        } else {
            None
        }
    }
}
