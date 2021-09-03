use std::cell::Cell;
use std::marker::PhantomData;
use std::ops::Range;
use std::slice;

use anyhow::{bail, Result};
use type_variance::{Invariant, Lifetime};

// use crate::fat::ClusterIdx;
use crate::ranges::{NotCoveredRange, Ranges};

type AllocatorId<'id> = Invariant<Lifetime<'id>>;
type ClusterIdx = u32;

// TODO after reading, make directory dataclusters free
// TODO ensure DataClusterIdx can also not be constructed
/// An `AllocatedClusterIdx` represents a cluster that was allocated by an `Allocator` and functions as a token to
/// access that cluster, either through the `Allocator` itself or through the `AllocatedReader` derived from it.
/// Invariant: no two `AllocatedClusterIdx` may have the same value; otherwise, `Allocator::cluster_mut` might alias.
#[derive(PartialEq, PartialOrd)]
pub struct AllocatedClusterIdx<'id>(ClusterIdx, AllocatorId<'id>);

impl<'id> AllocatedClusterIdx<'id> {
    /// SAFETY: Cloning an `AllocatedClusterIdx` breaks the invariant! To avoid aliasing, the caller must ensure that
    /// the original and the clone are not used to access a cluster simultaneously.
    pub unsafe fn clone(&self) -> Self {
        Self(self.0, self.1)
    }

    /// SAFETY: This is safe since it cannot be converted back to an `AllocatedClusterIdx` or to a `DataClusterIdx`.
    // TODO make it so that it *actually* cannot be converted to a DataClusterIdx
    pub fn as_cluster_idx(&self) -> ClusterIdx {
        self.0
    }
}

impl From<AllocatedClusterIdx<'_>> for ClusterIdx {
    fn from(idx: AllocatedClusterIdx) -> Self {
        idx.0
    }
}

impl From<AllocatedClusterIdx<'_>> for usize {
    fn from(idx: AllocatedClusterIdx) -> Self {
        idx.0 as Self
    }
}

/// A newtype that can only be instantiated by an `Allocator` to ensure that a range of `AllocatedClusterIdx` can only
/// be used as an iterator if every cluster in that range has indeed been allocated.
pub struct AllocatedRange<'id>(Range<AllocatedClusterIdx<'id>>);

impl<'id> AllocatedRange<'id> {
    pub fn len(&self) -> usize {
        self.0.end.0 as usize - self.0.start.0 as usize
    }

    pub fn iter_mut(&mut self) -> AllocatedIterMut<'_, 'id> {
        AllocatedIterMut::new(self)
    }
}

impl<'id> From<AllocatedRange<'id>> for Range<AllocatedClusterIdx<'id>> {
    fn from(range: AllocatedRange<'id>) -> Self {
        range.0
    }
}

impl From<AllocatedRange<'_>> for Range<ClusterIdx> {
    fn from(range: AllocatedRange) -> Self {
        range.0.start.into()..range.0.end.into()
    }
}

pub struct AllocatedIterMut<'borrow, 'id>(Range<ClusterIdx>, PhantomData<&'borrow ()>, AllocatorId<'id>);
impl<'borrow, 'id> AllocatedIterMut<'borrow, 'id> {
    fn new(range: &'borrow mut AllocatedRange) -> Self {
        let range = range.0.start.0..range.0.end.0;
        Self(range, PhantomData, AllocatorId::default())
    }
}

impl<'id> Iterator for AllocatedIterMut<'_, 'id> {
    type Item = AllocatedClusterIdx<'id>;
    fn next(&mut self) -> Option<Self::Item> {
        self.0.next().map(|idx| AllocatedClusterIdx(idx, self.2))
    }
}

/// Allocates clusters that are not marked as in use (specifically, clusters that are marked as
/// free in the FAT and which will not be overwritten by Ext4 block group metadata). Callers are
/// guaranteed that a cluster allocated to them will not be accessed anywhere else. They can access
/// such a cluster through the methods `cluster` and `cluster_mut`.
#[derive(Debug)]
pub struct Allocator<'partition, 'id> {
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
    _lifetime: PhantomData<&'partition ()>,
    id: AllocatorId<'id>,
}

impl<'partition, 'id> Allocator<'partition, 'id> {
    pub unsafe fn do_with_allocator<F, ReturnType>(
        fs_ptr: *mut u8,
        fs_len: usize,
        cluster_size: usize,
        used_ranges: Ranges<ClusterIdx>,
        _lifetime: PhantomData<&'partition ()>,
        f: F,
    ) -> ReturnType
    where
        F: for<'new_id> FnOnce(Allocator<'partition, 'new_id>) -> ReturnType,
    {
        let allocator = Allocator::new(fs_ptr, fs_len, cluster_size, used_ranges, _lifetime, AllocatorId::default());
        f(allocator)
    }

    /// SAFETY: Instantiating more than one `Allocator` can lead to undefined behavior, as mixing `AllocatedClusterIdx`
    /// allocated by different `Allocator`s can lead to aliasing.
    unsafe fn new(
        fs_ptr: *mut u8,
        fs_len: usize,
        cluster_size: usize,
        used_ranges: Ranges<ClusterIdx>,
        _lifetime: PhantomData<&'partition ()>,
        id: AllocatorId<'id>,
    ) -> Self {
        Self {
            fs_ptr,
            fs_len,
            cursor: Cell::new(0),
            first_valid_index: 0,
            used_ranges,
            cluster_size,
            _lifetime,
            id,
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
    pub fn split_into_reader<'new_id>(self) -> (AllocatedReader<'partition, 'id>, Allocator<'partition, 'new_id>) {
        let cursor_byte = self.cursor.get() as usize * self.cluster_size;
        // TODO free ranges used for FAT dentries

        let reader = AllocatedReader {
            fs_ptr: self.fs_ptr,
            fs_len: cursor_byte,
            cluster_size: self.cluster_size,
            _lifetime: self._lifetime,
            id: self.id,
        };

        // SAFETY: safe since the `fs_len` bytes after `fs_ptr` are valid memory and because of the
        // invariant `cursor_byte <= fs_len`, the `new_fs_len` bytes after `new_fs_ptr` are valid
        // memory as well.
        let new_fs_ptr = unsafe { self.fs_ptr.add(cursor_byte) };
        let new_fs_len = self.fs_len - cursor_byte;

        let allocator = Allocator {
            fs_ptr: new_fs_ptr,
            fs_len: new_fs_len,
            first_valid_index: self.cursor.get(),
            cursor: self.cursor,
            used_ranges: self.used_ranges,
            cluster_size: self.cluster_size,
            _lifetime: self._lifetime,
            id: AllocatorId::<'new_id>::default(),
        };

        (reader, allocator)
    }

    pub fn allocate_one(&self) -> AllocatedClusterIdx<'id> {
        Range::from(self.allocate(1)).start
    }

    /// Returns a cluster range that may be exclusively used by the caller with 1 <= `range.len()` <= `max_length`.
    // TODO error handling
    pub fn allocate(&self, max_length: usize) -> AllocatedRange<'id> {
        let free_range = self
            .find_next_free_range(self.cursor.get())
            .expect("Oh no, no more free blocks :(((");
        let range_end = free_range.end.min(free_range.start + max_length as u32);
        self.cursor.set(range_end);
        AllocatedRange(AllocatedClusterIdx(free_range.start, self.id)..AllocatedClusterIdx(range_end, self.id))
    }

    pub fn cluster(&'partition self, idx: &AllocatedClusterIdx<'id>) -> &[u8] {
        let start_byte = self
            .cluster_start_byte(idx)
            .expect("Attempted to access an allocated cluster that has been made invalid");
        assert!(start_byte + self.cluster_size < self.fs_len);
        // SAFETY: The data is valid and since `idx` is unique and we borrowed it, nobody else can mutate the data.
        unsafe { slice::from_raw_parts(self.fs_ptr.add(start_byte), self.cluster_size) }
    }

    pub fn cluster_mut(&self, idx: &mut AllocatedClusterIdx<'id>) -> &mut [u8] {
        let start_byte = self
            .cluster_start_byte(idx)
            .expect("Attempted to access an allocated cluster that has been made invalid");
        assert!(start_byte + self.cluster_size <= self.fs_len);
        // SAFETY: The data is valid and since `idx` is unique and we borrowed it mutably, nobody else can access the
        // data.
        unsafe { slice::from_raw_parts_mut(self.fs_ptr.add(start_byte), self.cluster_size) }
    }

    /// SAFETY: Instantiating an `AllocatedClusterIdx` might break the invariant! To avoid aliasing, the caller must
    /// ensure that `idx` was originally created from an `AllocatedClusterIdx` and that the original and the clone are
    /// not used to access a cluster simultaneously.
    pub unsafe fn new_allocated_cluster_idx(&self, idx: ClusterIdx) -> AllocatedClusterIdx<'id> {
        AllocatedClusterIdx(idx, self.id)
    }

    pub fn free_block_count(&self) -> usize {
        self.used_ranges.free_element_count(self.cursor.get(), self.max_cluster_idx())
    }

    // TODO documentation
    /// Returns the position in `self.partition_data` at which the cluster `idx` starts or None if
    /// the cluster is not in `self.partition_data`.
    fn cluster_start_byte(&self, idx: &AllocatedClusterIdx) -> Option<usize> {
        idx.0
            .checked_sub(self.first_valid_index)
            .map(|relative_cluster_idx| self.cluster_size * relative_cluster_idx as usize)
    }

    /// Returns the next range at or after `self.cursor` that is not used, or Err if such a range does not exist.
    fn find_next_free_range(&self, cursor: u32) -> Result<Range<ClusterIdx>> {
        let non_used_range = self.used_ranges.next_not_covered(cursor);
        let non_used_range = match non_used_range {
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


// no first_valid_index because in our use case it's always 0
pub struct AllocatedReader<'partition, 'id> {
    fs_ptr: *const u8,
    fs_len: usize,
    cluster_size: usize,
    _lifetime: PhantomData<&'partition ()>,
    id: AllocatorId<'id>,
}

impl<'partition, 'id> AllocatedReader<'partition, 'id> {
    pub fn do_with_new<F, ReturnType>(
        fs_ptr: *mut u8,
        fs_len: usize,
        cluster_size: usize,
        _lifetime: PhantomData<&'partition ()>,
        f: F,
    ) -> ReturnType
    where
        F: for<'new_id> FnOnce(AllocatedReader<'partition, 'new_id>) -> ReturnType,
    {
        let reader = AllocatedReader::new(fs_ptr, fs_len, cluster_size, _lifetime, AllocatorId::default());
        f(reader)
    }

    fn new(fs_ptr: *mut u8, fs_len: usize, cluster_size: usize, _lifetime: PhantomData<&'partition ()>, id: AllocatorId<'id>) -> Self {
        Self {
            fs_ptr, fs_len, cluster_size, _lifetime, id
        }
    }

    pub fn cluster(&self, idx: AllocatedClusterIdx<'id>) -> &'partition [u8] {
        let start_byte = self.cluster_size * usize::from(idx);
        assert!(start_byte + self.cluster_size <= self.fs_len);
        // SAFETY: The data is valid and since `idx` is unique and we borrowed it, nobody can mutate the data.
        unsafe { slice::from_raw_parts(self.fs_ptr.add(start_byte), self.cluster_size) }
    }
}
