use std::convert::TryFrom;
use std::marker::PhantomData;
use std::mem::size_of;
use std::ops::Range;
use std::slice;

use crate::allocator::Allocator;
use crate::ext4::Ext4Partition;
use crate::fat::{
    BootSector, Cluster, ClusterIdx, DataClusterIdx, FatFile, FatFileIter, FatIdxIter, FatTableIndex, ROOT_FAT_IDX,
};
use crate::ranges::Ranges;
use crate::util::ExactAlign;


/// A FAT32 partition consists of 3 regions: the reserved sectors (which include the boot sector),
/// the file allocation table (FAT), and the data region.
pub struct FatPartition<'a> {
    boot_sector: &'a BootSector,
    fat_table: &'a [FatTableIndex],
    data_ptr: *const u8,
    data_len: usize,
    _lifetime: PhantomData<&'a ()>,
}

impl<'a> FatPartition<'a> {
    /// SAFETY: The caller must guarantee that:
    /// - the `partition_len` bytes starting at `partition_ptr` are all valid memory;
    /// - this memory will remain valid for the lifetime 'a;
    /// - this memory represents a consistent FAT partition;
    /// - no pointer to one of the sections used by the FAT partition (i.e. the boot sector, the FAT table(s), and any
    ///   cluster that is not marked as free in the FAT table) will be dereferenced during the lifetime 'a.
    pub unsafe fn new(partition_ptr: *mut u8, partition_len: usize, _lifetime: PhantomData<&'a ()>) -> Self {
        assert!(size_of::<BootSector>() <= partition_len);
        let boot_sector = &*(partition_ptr as *const BootSector);

        let fat_table_range = boot_sector.get_fat_table_range();
        assert!(fat_table_range.start > size_of::<BootSector>());
        assert!(fat_table_range.end <= partition_len);
        let fat_table_ptr = partition_ptr.add(fat_table_range.start);
        let fat_table_bytes = slice::from_raw_parts(fat_table_ptr, fat_table_range.len());
        let fat_table = fat_table_bytes.exact_align_to::<FatTableIndex>();

        let data_range = boot_sector.get_data_range();
        assert!(data_range.start > fat_table_range.end);
        assert!(data_range.end <= partition_len);

        Self {
            boot_sector,
            fat_table,
            data_ptr: partition_ptr.add(data_range.start),
            data_len: data_range.len(),
            _lifetime,
        }
    }

    /// SAFETY: The caller must guarantee that:
    /// - the `partition_len` bytes starting at `partition_ptr` are all valid memory;
    /// - this memory will remain valid for the lifetime 'a;
    /// - this memory represents a consistent FAT partition;
    /// - no pointer to this memory will be dereferenced during the lifetime 'a.
    pub unsafe fn new_with_allocator(
        partition_ptr: *mut u8,
        partition_len: usize,
        lifetime: PhantomData<&'a ()>,
    ) -> (Self, Allocator) {
        // We want to borrow the partition's memory twice: immutably in FatPartition and mutably in Allocator. To avoid
        // aliasing, we divide the partition into used clusters (i.e. the reserved clusters, the FAT clusters, and the
        // data clusters that contain data) and unused clusters (i.e. the data clusters that contain no data).
        // FatPartition will only ever dereference pointers to used clusters. Allocator will only ever dereference
        // pointers to unused clusters.
        let instance = Self::new(partition_ptr, partition_len, lifetime);
        let allocator = Allocator::new(
            partition_ptr,
            partition_len,
            instance.cluster_size(),
            instance.used_ranges(),
            lifetime,
        );
        (instance, allocator)
    }

    pub fn into_ext4(self) -> Ext4Partition<'a> {
        let partition_data = unsafe {
            let start_ptr = self.boot_sector as *const _ as *mut u8;
            let end_ptr = self.data_ptr.add(self.data_len);
            let len = end_ptr.offset_from(start_ptr);
            std::slice::from_raw_parts_mut(start_ptr, usize::try_from(len).unwrap())
        };
        Ext4Partition::from(partition_data, self.boot_sector)
    }

    pub fn boot_sector(&self) -> &BootSector {
        self.boot_sector
    }

    // TODO all the int type conversions (from, try_from)
    // TODO error concept: return options of results? error chain?

    pub fn fat_table(&self) -> &'a [FatTableIndex] {
        self.fat_table
    }

    pub fn cluster_size(&self) -> usize {
        self.boot_sector.cluster_size()
    }

    pub fn dentries_per_cluster(&self) -> usize {
        self.boot_sector.dentries_per_cluster()
    }

    // TODO these conversions are a mess
    pub fn cluster_idx_to_data_cluster_idx(&self, cluster_idx: ClusterIdx) -> Result<DataClusterIdx, &str> {
        let data_cluster_idx = cluster_idx.checked_sub(self.boot_sector.first_data_cluster());
        match data_cluster_idx {
            Some(data_cluster_idx) => Ok(DataClusterIdx::new(data_cluster_idx)),
            None => Err("cluster_idx is not a data cluster index"),
        }
    }

    pub fn data_cluster(&self, data_cluster_idx: DataClusterIdx) -> &Cluster {
        let cluster_size = self.cluster_size();
        let start_byte = usize::from(data_cluster_idx) * cluster_size;
        assert!(start_byte + cluster_size <= self.data_len);
        // SAFETY: safe because the memory is valid and cannot be mutated without borrowing `self` as mut.
        unsafe { slice::from_raw_parts(self.data_ptr.add(start_byte), cluster_size) }
    }

    /// Given the index of a directory's first cluster, iterate over the directory's content.
    /// SAFETY: safe if `first_fat_idx` points to a cluster belonging to a directory
    pub unsafe fn dir_content_iter(&'a self, first_fat_idx: FatTableIndex) -> impl Iterator<Item = FatFile> + 'a {
        FatFileIter::new(first_fat_idx, self)
    }

    /// Given a file's first FAT index, follow the FAT chain and collect all of the file's FAT indices into a list of
    /// adjacent ranges.
    pub fn data_ranges(&'a self, first_fat_idx: FatTableIndex) -> Vec<Range<ClusterIdx>> {
        if first_fat_idx.is_zero_length_file() {
            return Vec::new();
        }

        let first_cluster_idx = first_fat_idx.to_cluster_idx(self.boot_sector());
        let mut current_range = first_cluster_idx..first_cluster_idx; // we don't use RangeInclusive because it does not allow mutating end
        let mut ranges = Vec::new();

        for fat_idx in FatIdxIter::new(first_fat_idx, self.fat_table()) {
            let cluster_idx = fat_idx.to_cluster_idx(self.boot_sector());
            if cluster_idx == current_range.end {
                current_range.end += 1;
            } else {
                ranges.push(current_range);
                current_range = cluster_idx..cluster_idx + 1;
            }
        }
        ranges.push(current_range);
        ranges
    }

    fn is_free(&self, fat_idx: FatTableIndex) -> bool {
        self.fat_table()[fat_idx].is_free()
    }

    /// Returns the occupied clusters in the partition
    pub fn used_ranges(&self) -> Ranges<ClusterIdx> {
        let mut ranges = Ranges::new();
        let first_data_cluster_idx = self.boot_sector.first_data_cluster();
        let non_data_range = 0..first_data_cluster_idx;
        ranges.insert(non_data_range);

        // could be optimized to build bigger ranges and call `ranges.insert` less often
        for (fat_idx, &fat_cell) in self.fat_table().iter().enumerate().skip(u32::from(ROOT_FAT_IDX) as usize) {
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
    use std::array::IntoIter;
    use std::collections::HashSet;
    use std::iter::FromIterator;

    use super::*;
    use crate::fat::ROOT_FAT_IDX;
    use crate::partition::Partition;

    #[test]
    fn iterates_over_dir_content() {
        static EXPECTED_FILE_NAMES: [&str; 20] = [
            "a",
            "adfdfafd",
            "asda",
            "asdf",
            "asdfdf",
            "asdfdfdfdf",
            "asds",
            "b",
            "c",
            "d",
            "dfdsafdsf",
            "e",
            "f",
            "fdfad",
            "fdfdfdfd",
            "g",
            "qwe",
            "qwew",
            "sdfsdf",
            "swag",
        ];
        let expected_file_names = HashSet::from_iter(EXPECTED_FILE_NAMES.iter().map(|s| s.to_string()));

        let mut partition = Partition::open("examples/fat.master.bak").unwrap();
        unsafe {
            let fat_partition = FatPartition::new(partition.as_mut_slice());
            let file_names: HashSet<_> = fat_partition.dir_content_iter(ROOT_FAT_IDX).map(|file| file.name).collect();
            assert_eq!(file_names, expected_file_names);
        }
    }
}
