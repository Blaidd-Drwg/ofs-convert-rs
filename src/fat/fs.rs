use std::convert::TryFrom;
use std::iter::Step;
use std::marker::PhantomData;
use std::mem::size_of;
use std::ops::RangeInclusive;
use std::slice;

use anyhow::Result;

use crate::allocator::Allocator;
use crate::ext4::Ext4Fs;
use crate::fat::{
    BootSector, Cluster, ClusterIdx, DataClusterIdx, FatFile, FatFileIter, FatIdxIter, FatTableIndex, ROOT_FAT_IDX,
};
use crate::ranges::Ranges;
use crate::util::{AddUsize, ExactAlign, FromU32};


/// A FAT32 partition consists of 3 regions: the reserved sectors (which include the boot sector),
/// the file allocation table (FAT), and the data region.
pub struct FatFs<'a> {
    boot_sector: &'a BootSector,
    fat_table: &'a [FatTableIndex],
    data_ptr: *const u8,
    data_len: usize,
    _lifetime: PhantomData<&'a ()>,
}

impl<'a> FatFs<'a> {
    /// SAFETY: The caller must guarantee that:
    /// - the `partition_len` bytes starting at `partition_ptr` are all valid memory;
    /// - this memory will remain valid for the lifetime 'a;
    /// - this memory represents a consistent FAT filesystem;
    /// - no pointer to one of the sections used by the FAT filesystem (i.e. the boot sector, the FAT table(s), and any
    ///   cluster that is not marked as free in the FAT table) will be dereferenced during the lifetime 'a.
    /// PANICS: Panics if inconsistencies are detected in the filesystem
    pub unsafe fn new(partition_ptr: *mut u8, partition_len: usize, _lifetime: PhantomData<&'a ()>) -> Result<Self> {
        assert!(size_of::<BootSector>() <= partition_len);
        // SAFETY: safe because a consistent FAT32 fs begins with a boot sector
        let boot_sector = unsafe { &*(partition_ptr as *const BootSector) }.validate()?;

        let fat_table_range = boot_sector.get_fat_table_range();
        assert!(fat_table_range.start > size_of::<BootSector>());
        assert!(fat_table_range.end <= partition_len);
        // SAFETY: Safe because the FAT table is within the partition
        let fat_table = unsafe {
            let fat_table_ptr = partition_ptr.add_usize(fat_table_range.start);
            let fat_table_bytes = slice::from_raw_parts(fat_table_ptr, fat_table_range.len());
            fat_table_bytes.exact_align_to::<FatTableIndex>()
        };

        let data_range = boot_sector.get_data_range();
        assert!(data_range.start > fat_table_range.end);
        assert!(data_range.end <= partition_len);

        Ok(Self {
            boot_sector,
            fat_table,
            // SAFETY: Safe because the data clusters are within the partition
            data_ptr: unsafe { partition_ptr.add_usize(data_range.start) },
            data_len: data_range.len(),
            _lifetime,
        })
    }

    /// SAFETY: The caller must guarantee that:
    /// - the `partition_len` bytes starting at `partition_ptr` are all valid memory;
    /// - this memory will remain valid for the lifetime 'a;
    /// - no pointer to this memory will be dereferenced during the lifetime 'a;
    /// - this memory represents a consistent FAT filesystem.
    pub unsafe fn new_with_allocator(
        partition_ptr: *mut u8,
        partition_len: usize,
        lifetime: PhantomData<&'a ()>,
    ) -> Result<(Self, Allocator)> {
        // We want to borrow the filesystem's memory twice: immutably in `FatFs` and mutably in `Allocator`. To avoid
        // aliasing, we divide the filesystem into used clusters (i.e. the reserved clusters, the FAT clusters, and the
        // data clusters that contain data) and unused clusters (i.e. the data clusters that contain no data).
        // `FatFs` will only ever dereference pointers to used clusters. `Allocator` will only ever dereference
        // pointers to unused clusters.
        unsafe {
            let instance = Self::new(partition_ptr, partition_len, lifetime)?;
            let allocator = Allocator::new(
                partition_ptr,
                instance.boot_sector.fs_size(),
                usize::fromx(instance.cluster_size()),
                instance.used_ranges(),
                lifetime,
            );
            Ok((instance, allocator))
        }
    }

    /// SAFETY: Safe if no block in `SuperBlock::from(self.boot_sector).block_group_overhead_ranges()` is accessed for
    /// the duration of the lifetime 'a
    pub unsafe fn into_ext4(self) -> Result<Ext4Fs<'a>> {
        let start_ptr = self.boot_sector as *const _ as *mut u8;
        // SAFETY: Safe since `start_ptr` is the start of a consistent filesystem described by `boot_sector`.
        unsafe { Ext4Fs::from(start_ptr, self.boot_sector) }
    }

    pub fn boot_sector(&self) -> &BootSector {
        self.boot_sector
    }

    pub fn fat_table(&self) -> &'a [FatTableIndex] {
        self.fat_table
    }

    pub fn cluster_size(&self) -> u32 {
        self.boot_sector.cluster_size()
    }

    pub fn dentries_per_cluster(&self) -> usize {
        self.boot_sector.dentries_per_cluster()
    }

    pub fn cluster_from_data_cluster(&self, data_cluster_idx: DataClusterIdx) -> ClusterIdx {
        ClusterIdx::from(data_cluster_idx) + self.boot_sector.first_data_cluster()
    }

    pub fn cluster_count(&self) -> u32 {
        self.boot_sector.cluster_count()
    }

    pub fn is_used(&self, data_cluster_idx: DataClusterIdx) -> bool {
        !self.fat_table[data_cluster_idx.to_fat_index()].is_free()
    }

    /// PANICS: Panics if `data_cluster_idx` is not a valid, in-use data cluster.
    pub fn data_cluster(&self, data_cluster_idx: DataClusterIdx) -> &Cluster {
        assert!(self.is_used(data_cluster_idx));
        let cluster_size = usize::fromx(self.cluster_size());
        let start_byte = usize::from(data_cluster_idx) * cluster_size;
        assert!(start_byte + cluster_size <= self.data_len);
        unsafe {
            // SAFETY: safe because the cluster is within the partition.
            let ptr = self.data_ptr.add_usize(start_byte);
            // SAFETY: safe because the memory is valid and cannot be mutated without borrowing `self` as mut.
            slice::from_raw_parts(ptr, cluster_size)
        }
    }

    /// Given the index of a directory's first cluster, iterate over the directory's content.
    /// SAFETY: safe if `first_fat_idx` points to a cluster belonging to a directory
    pub unsafe fn dir_content_iter(&'a self, first_fat_idx: FatTableIndex) -> impl Iterator<Item = FatFile> + 'a {
        unsafe { FatFileIter::new(first_fat_idx, self) }
    }

    /// Given a file's first FAT index, follow the FAT chain and collect all of the file's FAT indices into a list of
    /// adjacent ranges.
    pub fn data_ranges(&'a self, first_fat_idx: FatTableIndex) -> Vec<RangeInclusive<DataClusterIdx>> {
        if first_fat_idx.is_zero_length_file() {
            return Vec::new();
        }

        let first_data_cluster_idx = first_fat_idx.to_data_cluster_idx();
        let mut current_range = first_data_cluster_idx..=first_data_cluster_idx;
        let mut ranges = Vec::new();

        for fat_idx in FatIdxIter::new(first_fat_idx, self.fat_table()).skip(1) {
            let next_data_cluster_idx = fat_idx.to_data_cluster_idx();
            if DataClusterIdx::steps_between(current_range.end(), &next_data_cluster_idx) == Some(1) {
                current_range = current_range.into_inner().0..=next_data_cluster_idx;
            } else {
                ranges.push(current_range);
                current_range = next_data_cluster_idx..=next_data_cluster_idx;
            }
        }
        ranges.push(current_range);
        ranges
    }

    /// Returns the occupied clusters in the filesystem
    pub fn used_ranges(&self) -> Ranges<ClusterIdx> {
        let mut ranges = Ranges::new();
        let first_data_cluster_idx = self.boot_sector.first_data_cluster();
        let non_data_range = 0..first_data_cluster_idx;
        ranges.insert(non_data_range);

        // could be optimized to build bigger ranges and call `ranges.insert` less often
        for (fat_idx, &fat_cell) in self.fat_table().iter().enumerate().skip(usize::from(ROOT_FAT_IDX)) {
            if !fat_cell.is_free() {
                let range_start = FatTableIndex::try_from(fat_idx).unwrap().to_cluster_idx(self.boot_sector());
                ranges.insert(range_start..range_start + 1);
            }
        }
        ranges
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;
    use std::iter::FromIterator;

    use super::*;
    use crate::fat::ROOT_FAT_IDX;
    use crate::partition::Partition;
    use crate::util::tests::backup_copy;

    #[test]
    fn iterates_over_dir_content() {
        const FAT_IMAGE_PATH: &str = "test/example_fat.img";
        const EXPECTED_FILE_NAMES: [&str; 10] = [
            "allocator.rs",
            "bitmap.rs",
            "ext4",
            "fat",
            "lohi.rs",
            "main.rs",
            "partition.rs",
            "ranges.rs",
            "serialization",
            "util.rs",
        ];
        let expected_file_names = HashSet::from_iter(EXPECTED_FILE_NAMES.iter().map(|s| s.to_string()));
        let file_copy = backup_copy(FAT_IMAGE_PATH).unwrap();

        let mut partition = Partition::open(file_copy.path()).unwrap();
        let file_names: HashSet<_> = unsafe {
            let fat_fs = FatFs::new(partition.as_mut_ptr(), partition.len(), PhantomData).unwrap();
            fat_fs.dir_content_iter(ROOT_FAT_IDX).map(|file| file.name).collect()
        };
        assert_eq!(file_names, expected_file_names);
    }
}
