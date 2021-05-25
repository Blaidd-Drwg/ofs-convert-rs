use crate::fat::{ROOT_FAT_IDX, BootSector, Cluster, FatTableIndex, ClusterIdx, FatFileIter, FatIdxIter, FatFile, FatDentry};
use crate::ranges::Ranges;
use crate::c_wrapper::{c_serialize_directory, c_serialize_file, StreamArchiver};
use crate::util::ExactAlign;
use std::convert::TryFrom;
use std::mem::size_of;
use std::ops::Range;


/// A FAT32 partition consists of 3 regions: the reserved sectors (which include the boot sector),
/// the file allocation table (FAT), and the data region.
pub struct FatPartition<'a> {
    boot_sector: &'a BootSector,
    fat_table: &'a [FatTableIndex],
    data: &'a mut [u8],
}


// TODO ensure even an inconsistent FAT partition won't ever cause undefined behavior, remove unsafe where possible
impl<'a> FatPartition<'a> {
    /// SAFETY: Safety is only guaranteed if `partition_data` is a consistent FAT32 partition.
    pub unsafe fn new(partition_data: &'a mut [u8]) -> Self {
        let (bs_bytes, data_after_boot_sector) = partition_data.split_at_mut(size_of::<BootSector>());
        let boot_sector = &*(bs_bytes as *const [u8] as *const BootSector);

        let fat_table_range = boot_sector.get_fat_table_range();

        let relative_fat_table_start = fat_table_range.start - bs_bytes.len();
        let data_after_reserved_sectors = &mut data_after_boot_sector[relative_fat_table_start..];
        let (fat_table_bytes, data_after_fat_table) = data_after_reserved_sectors.split_at_mut(fat_table_range.len());
        let fat_table = fat_table_bytes.exact_align_to::<FatTableIndex>();

        let mut data_range = boot_sector.get_data_range();
        data_range.start -= fat_table_range.end;
        data_range.end -= fat_table_range.end;
        let relative_data_range = data_range;
        let data = &mut data_after_fat_table[relative_data_range];

        Self { boot_sector, fat_table, data }
    }

    pub fn boot_sector(&self) -> &BootSector {
        self.boot_sector
    }

    // TODO all the int type conversions (from, try_from)
    // TODO error concept: return options of results?

    pub fn serialize_directory_tree(&mut self, stream_archiver: *mut StreamArchiver) {
        let root_file = FatFile {
            name: "".to_string(),
            lfn_entries: Vec::new(),
            dentry: FatDentry::root_dentry(),
            data_ranges: self.data_ranges(ROOT_FAT_IDX)
        };
        unsafe {
            self.serialize_directory(root_file, stream_archiver);
        }
    }

    unsafe fn serialize_directory(&self, file: FatFile, stream_archiver: *mut StreamArchiver) {
        let child_count_ptr = c_serialize_directory(&file, stream_archiver);
        let first_fat_idx = file.dentry.first_fat_index();
        for file in self.dir_content_iter(first_fat_idx) {
            *child_count_ptr += 1;
            if file.dentry.is_dir() {
                self.serialize_directory(file, stream_archiver);
            } else {
                self.serialize_file(file, stream_archiver);
            }
        }
    }

    fn serialize_file(&self, file: FatFile, stream_archiver: *mut StreamArchiver) {
        c_serialize_file(&file, stream_archiver);
    }

    pub fn fat_table(&self) -> &'a [FatTableIndex] {
        self.fat_table
    }

    pub fn cluster_size(&self) -> usize {
        usize::from(self.boot_sector.sectors_per_cluster) * usize::from(self.boot_sector.bytes_per_sector)
    }

    pub fn data_cluster(&self, fat_idx: FatTableIndex) -> &Cluster {
        let cluster_size = self.cluster_size();
        let start_byte = usize::try_from(fat_idx.to_data_cluster_idx()).unwrap() * cluster_size;
        &self.data[start_byte..start_byte+cluster_size]
    }

    pub fn data_cluster_mut(&mut self, fat_idx: FatTableIndex) -> &mut Cluster {
        let cluster_size = self.cluster_size();
        let start_byte = usize::try_from(fat_idx.to_data_cluster_idx()).unwrap() * cluster_size;
        &mut self.data[start_byte..start_byte+cluster_size]
    }

    /// Given the index of a directory's first cluster, iterate over the directory's content.
    /// SAFETY: safe if `first_fat_idx` points to a cluster belonging to a directory
    pub unsafe fn dir_content_iter(&'a self, first_fat_idx: FatTableIndex) -> impl Iterator<Item = FatFile> + 'a {
        FatFileIter::new(first_fat_idx, self, self.boot_sector().dentries_per_cluster())
    }

    /// Given a file's first FAT index, follow the FAT chain and collect all of the file's FAT indices into a list of adjacent ranges.
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::partition::Partition;
    use std::collections::HashSet;
    use std::iter::FromIterator;
    use std::array::IntoIter;

    #[test]
    fn iterates_over_dir_content() {
        static EXPECTED_FILE_NAMES: [&str; 20] = [
            "a", "adfdfafd", "asda", "asdf", "asdfdf", "asdfdfdfdf", "asds", "b", "c", "d",
            "dfdsafdsf", "e", "f", "fdfad", "fdfdfdfd", "g", "qwe", "qwew", "sdfsdf", "swag"
        ];
        let expected_file_names = HashSet::from_iter(EXPECTED_FILE_NAMES.iter().map(|s| s.to_string()));

        let mut partition = Partition::open("examples/fat.master.bak").unwrap();
        unsafe {
            let fat_partition = FatPartition::new(partition.as_mut_slice());
            let file_names: HashSet<_> = fat_partition
                .dir_content_iter(ROOT_FAT_IDX)
                .map(|file| file.name)
                .collect();
            assert_eq!(file_names, expected_file_names);
        }
    }
}
