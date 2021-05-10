use crate::fat::{BootSector, Cluster, ClusterIdx, FatFileIter, FatEntryIter, FatFile, FatPseudoDentryIter, FatPseudoDentry, FatTable};
use crate::util::ExactAlign;
use std::convert::TryFrom;
use std::mem::size_of;
use std::ops::Range;

const FIRST_DATA_CLUSTER: ClusterIdx = 2; // the first cluster containing data has the index 2

/// A FAT32 partition consists of 3 regions: the reserved sectors (which include the boot sector),
/// the file allocation table (FAT), and the data region.
pub struct FatPartition<'a> {
    boot_sector: &'a BootSector,
    fat_table: FatTable<'a>,
    data: &'a mut [u8],
}


// TODO ensure even an inconsistent FAT partition won't ever cause undefined behavior, remove unsafe where possible
impl<'a> FatPartition<'a> {
    /// SAFETY: Safety is only guaranteed if `partition_data` is a consistent FAT32 partition.
    pub unsafe fn new(partition_data: &'a mut [u8]) -> Self {
        let (bs_bytes, data_after_boot_sector) = partition_data.split_at_mut(size_of::<BootSector>());
        let boot_sector = &*(bs_bytes as *const [u8] as *const BootSector);

        let fat_table_range = Self::get_fat_table_range(boot_sector);
        let data_range = Self::get_data_range(boot_sector);

        let relative_fat_table_start = fat_table_range.start - bs_bytes.len();
        let data_after_reserved_sectors = &mut data_after_boot_sector[relative_fat_table_start..];
        let (fat_table_bytes, data_after_fat_table) = data_after_reserved_sectors.split_at_mut(fat_table_range.len());
        let fat_table_data = fat_table_bytes.exact_align_to::<ClusterIdx>();
        let fat_table = FatTable::new(fat_table_data);

        let relative_data_start = data_range.start - fat_table_range.end;
        let data = &mut data_after_fat_table[relative_data_start..];

        assert_eq!(data.len(), data_range.len(), "The partition size declared by FAT is inconsistent with the actual partition size.");

        Self { boot_sector, fat_table, data }
    }

    /// Returns the range in bytes of the first FAT table, relative to the partition start
    fn get_fat_table_range(boot_sector: &BootSector) -> Range<usize> {
        let fat_table_start_byte = usize::from(boot_sector.sectors_before_fat) * usize::from(boot_sector.bytes_per_sector);
        let fat_table_len = usize::try_from(boot_sector.sectors_per_fat).unwrap() * usize::from(boot_sector.bytes_per_sector);
        fat_table_start_byte .. fat_table_start_byte + fat_table_len
    }

    /// Returns the range in bytes of the data region, relative to the partition start
    fn get_data_range(boot_sector: &BootSector) -> Range<usize> {
        let sectors_before_data = usize::from(boot_sector.sectors_before_fat) + (usize::try_from(boot_sector.sectors_per_fat).unwrap() * usize::from(boot_sector.fat_count));
        let bytes_before_data = sectors_before_data * usize::from(boot_sector.bytes_per_sector);
        let partition_size = usize::from(boot_sector.bytes_per_sector) * usize::try_from(boot_sector.sector_count()).unwrap();
        bytes_before_data .. partition_size
    }

    pub fn boot_sector(&self) -> &BootSector {
        self.boot_sector
    }

    // TODO assert that alignments are exact. new function convert_slice?
    // TODO all the int type conversions (from, try_from)
    // TODO error concept: return options of results?
    // new function from_bytes that takes a byte slice and asserts the length is exact?

    pub fn serialize_directory_tree() { }

    pub fn fat_table(&self) -> FatTable {
        self.fat_table
    }

    pub fn cluster_size(&self) -> usize {
        usize::from(self.boot_sector.sectors_per_cluster) * usize::from(self.boot_sector.bytes_per_sector)
    }

    pub fn cluster(&self, cluster_idx: ClusterIdx) -> &Cluster {
        let cluster_size = self.cluster_size();
        let data_byte = usize::try_from(cluster_idx - FIRST_DATA_CLUSTER).unwrap() * cluster_size;
        &self.data[data_byte..data_byte+cluster_size]
    }

    pub fn cluster_mut(&mut self, cluster_idx: ClusterIdx) -> &mut Cluster {
        let cluster_size = self.cluster_size();
        let data_byte = usize::try_from(cluster_idx - FIRST_DATA_CLUSTER).unwrap() * cluster_size;
        &mut self.data[data_byte..data_byte+cluster_size]
    }

    /// Given the index of a directory's first cluster, iterate over the directory's content.
    /// SAFETY: safe if `first_cluster_idx` points to a cluster belonging to a directory
    pub unsafe fn dir_content_iter(&'a self, first_cluster_idx: ClusterIdx) -> impl Iterator<Item = FatFile> + 'a {
        // iterator over the indices of the clusters containing the directory's content
        let cluster_idx_iter = self.fat_table().file_cluster_iter(first_cluster_idx);
        let pseudo_dentry_iter = FatPseudoDentryIter::new(cluster_idx_iter, &self, self.boot_sector().dentries_per_cluster());
        FatFileIter::new(pseudo_dentry_iter)
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
        let fat_partition = unsafe { FatPartition::new(partition.as_mut_slice()) };
        let file_names: HashSet<_> = fat_partition
            .dir_content_iter(FIRST_DATA_CLUSTER)
            .map(|file| file.name)
            .collect();
        assert_eq!(file_names, expected_file_names);
    }
}
